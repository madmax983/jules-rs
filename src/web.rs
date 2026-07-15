//! Cross-repo web dashboard for Jules sessions.
//!
//! This module powers the `jules-web` binary. It fetches every session across
//! every repository from the Jules API, aggregates the data into headline
//! numbers, per-state and per-hour breakdowns, and renders a self-contained
//! HTML dashboard using [`autumn_web`] + Maud. A companion `GET /api/summary`
//! endpoint exposes the same aggregates as JSON.
//!
//! All aggregation logic lives in small pure functions so it can be unit
//! tested without any network access.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use autumn_web::prelude::*;
use serde::Serialize;

use crate::{
    JulesClient, JulesError, ListSessionsParams, ListSourcesParams, Session, SessionState, Source,
    TimeoutPolicy,
};

/// Shared, process-wide request context. Built once at startup and read by the
/// stateless route handlers registered through the `routes!` macro.
struct WebContext {
    client: JulesClient,
}

static CONTEXT: OnceLock<WebContext> = OnceLock::new();

/// Build a [`JulesClient`] from the environment, mirroring `director`'s
/// `build_client`: `JULES_API_KEY` is required and `JULES_API_URL` overrides
/// the base URL when set.
///
/// # Errors
///
/// Returns a human-readable error string when `JULES_API_KEY` is missing or
/// when the underlying client fails to build (for example an invalid base URL).
pub fn build_client_from_env() -> Result<JulesClient, String> {
    let api_key = std::env::var("JULES_API_KEY")
        .map_err(|_| "missing environment variable `JULES_API_KEY`".to_string())?;

    let mut builder = JulesClient::builder(api_key).timeout_policy(TimeoutPolicy {
        request_timeout: Duration::from_secs(45),
    });

    if let Ok(api_url) = std::env::var("JULES_API_URL") {
        if !api_url.trim().is_empty() {
            builder = builder.base_url(api_url);
        }
    }

    builder.build().map_err(|error| error.to_string())
}

/// Start the dashboard HTTP server.
///
/// Builds the Jules client from the environment, stores it in the shared
/// context, and hands control to the `autumn-web` runtime. The listen port is
/// controlled by `autumn-web` configuration (`AUTUMN_SERVER__PORT`, default
/// `3000`).
///
/// # Errors
///
/// Returns an error string when the client cannot be built from the
/// environment. Any failure inside a request handler is rendered as a friendly
/// error page rather than propagated here.
pub async fn run() -> Result<(), String> {
    let client = build_client_from_env()?;
    CONTEXT
        .set(WebContext { client })
        .map_err(|_| "web context was already initialized".to_string())?;

    autumn_web::app()
        .routes(routes![index, api_summary])
        .run()
        .await;

    Ok(())
}

// ── Data fetching ────────────────────────────────────────────────

/// Paginate `list_sessions` fully, returning every session across all repos.
async fn fetch_all_sessions(client: &JulesClient) -> Result<Vec<Session>, JulesError> {
    let mut all = Vec::new();
    let mut page_token = None;
    loop {
        let response = client
            .list_sessions(ListSessionsParams {
                page_size: Some(100),
                page_token: page_token.clone(),
                filter: None,
            })
            .await?;
        all.extend(response.sessions);
        match response.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }
    Ok(all)
}

/// Paginate `list_sources` fully, returning every source.
async fn fetch_all_sources(client: &JulesClient) -> Result<Vec<Source>, JulesError> {
    let mut all = Vec::new();
    let mut page_token = None;
    loop {
        let response = client
            .list_sources(ListSourcesParams {
                page_size: Some(100),
                page_token: page_token.clone(),
            })
            .await?;
        all.extend(response.sources);
        match response.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }
    Ok(all)
}

// ── Pure aggregation helpers ─────────────────────────────────────

/// All session states in a stable display order.
const ALL_STATES: [SessionState; 10] = [
    SessionState::InProgress,
    SessionState::Queued,
    SessionState::Planning,
    SessionState::AwaitingPlanApproval,
    SessionState::AwaitingUserFeedback,
    SessionState::Paused,
    SessionState::Completed,
    SessionState::Failed,
    SessionState::SessionStateUnspecified,
    SessionState::Unknown,
];

/// Whether a state counts as "running right now" (non-terminal, real work).
fn is_active_state(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Queued
            | SessionState::Planning
            | SessionState::AwaitingPlanApproval
            | SessionState::AwaitingUserFeedback
            | SessionState::InProgress
            | SessionState::Paused
    )
}

/// Resolve a session's state, treating a missing state as `Unknown`.
fn session_state(session: &Session) -> SessionState {
    session.state.unwrap_or(SessionState::Unknown)
}

/// Count sessions currently running (in any active, non-terminal state).
fn count_running(sessions: &[Session]) -> usize {
    sessions
        .iter()
        .filter(|session| is_active_state(session_state(session)))
        .count()
}

/// Count sessions per state, returned in the fixed [`ALL_STATES`] order,
/// omitting states with a zero count.
fn count_by_state(sessions: &[Session]) -> Vec<(SessionState, usize)> {
    ALL_STATES
        .iter()
        .filter_map(|&state| {
            let count = sessions
                .iter()
                .filter(|session| session_state(session) == state)
                .count();
            (count > 0).then_some((state, count))
        })
        .collect()
}

/// Extract a stable `YYYY-MM-DD HH:00` hour-bucket key from an RFC3339
/// timestamp such as `2026-07-15T12:34:56Z`. Returns `None` for values that do
/// not match the fixed prefix layout.
fn hour_bucket_key(timestamp: &str) -> Option<String> {
    let bytes = timestamp.as_bytes();
    if bytes.len() < 13 {
        return None;
    }
    // Positions: YYYY(0-3) -(4) MM(5-6) -(7) DD(8-9) T(10) HH(11-12)
    let is_digit = |i: usize| bytes[i].is_ascii_digit();
    let ok = is_digit(0)
        && is_digit(1)
        && is_digit(2)
        && is_digit(3)
        && bytes[4] == b'-'
        && is_digit(5)
        && is_digit(6)
        && bytes[7] == b'-'
        && is_digit(8)
        && is_digit(9)
        && (bytes[10] == b'T' || bytes[10] == b' ')
        && is_digit(11)
        && is_digit(12);
    if !ok {
        return None;
    }
    let date = &timestamp[0..10];
    let hour = &timestamp[11..13];
    Some(format!("{date} {hour}:00"))
}

/// Bucket sessions by the hour of their `create_time`, returning
/// `(hour_key, count)` pairs sorted ascending by hour. Sessions with a missing
/// or malformed timestamp are skipped.
fn count_by_hour(sessions: &[Session]) -> Vec<(String, usize)> {
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    for session in sessions {
        if let Some(key) = session
            .create_time
            .as_deref()
            .and_then(hour_bucket_key)
        {
            *buckets.entry(key).or_insert(0) += 1;
        }
    }
    buckets.into_iter().collect()
}

/// Human-readable label for a source: `owner/repo` when GitHub metadata is
/// present, else the display name, else the raw resource name.
fn source_label(source: &Source) -> String {
    if let Some(github) = &source.github_repo {
        format!("{}/{}", github.owner, github.repo)
    } else if let Some(display_name) = &source.display_name {
        display_name.clone()
    } else {
        source.name.clone()
    }
}

/// Build a map from source resource name to its human-readable label.
fn source_labels(sources: &[Source]) -> BTreeMap<String, String> {
    sources
        .iter()
        .map(|source| (source.name.clone(), source_label(source)))
        .collect()
}

/// Resolve the repo label for a session, preferring the embedded source, then
/// the source context (looked up in `labels`), then falling back gracefully.
fn repo_label(session: &Session, labels: &BTreeMap<String, String>) -> String {
    if let Some(source) = &session.source {
        return source_label(source);
    }
    if let Some(context) = &session.source_context {
        if let Some(label) = labels.get(&context.source) {
            return label.clone();
        }
        return context.source.clone();
    }
    "(unknown)".to_string()
}

/// Stable machine name for a state, used for filtering and JSON.
fn state_name(state: SessionState) -> &'static str {
    match state {
        SessionState::SessionStateUnspecified => "SessionStateUnspecified",
        SessionState::Queued => "Queued",
        SessionState::Planning => "Planning",
        SessionState::AwaitingPlanApproval => "AwaitingPlanApproval",
        SessionState::AwaitingUserFeedback => "AwaitingUserFeedback",
        SessionState::InProgress => "InProgress",
        SessionState::Paused => "Paused",
        SessionState::Completed => "Completed",
        SessionState::Failed => "Failed",
        SessionState::Unknown => "Unknown",
    }
}

/// Short human label for a state badge.
fn state_display(state: SessionState) -> &'static str {
    match state {
        SessionState::SessionStateUnspecified => "Unspecified",
        SessionState::Queued => "Queued",
        SessionState::Planning => "Planning",
        SessionState::AwaitingPlanApproval => "Awaiting Plan",
        SessionState::AwaitingUserFeedback => "Awaiting Feedback",
        SessionState::InProgress => "In Progress",
        SessionState::Paused => "Paused",
        SessionState::Completed => "Completed",
        SessionState::Failed => "Failed",
        SessionState::Unknown => "Unknown",
    }
}

/// Accent color (hex) for a state, used for badges and bars.
fn state_color(state: SessionState) -> &'static str {
    match state {
        SessionState::InProgress => "#22c55e",
        SessionState::Queued => "#3b82f6",
        SessionState::Planning => "#6366f1",
        SessionState::AwaitingPlanApproval | SessionState::AwaitingUserFeedback => "#f59e0b",
        SessionState::Paused => "#a855f7",
        SessionState::Completed => "#94a3b8",
        SessionState::Failed => "#ef4444",
        SessionState::SessionStateUnspecified | SessionState::Unknown => "#64748b",
    }
}

/// Best-effort human title for a session row.
fn session_title(session: &Session) -> String {
    session
        .title
        .as_deref()
        .or(session.prompt.as_deref())
        .or(session.description.as_deref())
        .unwrap_or("(no title)")
        .to_string()
}

/// Truncate a string to `max` characters, appending an ellipsis when cut.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// Format a raw RFC3339 timestamp for compact display (`YYYY-MM-DD HH:MM`).
fn format_time(timestamp: Option<&str>) -> String {
    match timestamp {
        Some(ts) if ts.len() >= 16 => format!("{} {}", &ts[0..10], &ts[11..16]),
        Some(ts) => ts.to_string(),
        None => "—".to_string(),
    }
}

// ── JSON summary ─────────────────────────────────────────────────

/// Per-state count entry in the JSON summary.
#[derive(Debug, Serialize)]
struct StateCount {
    state: String,
    count: usize,
}

/// Per-hour count entry in the JSON summary.
#[derive(Debug, Serialize)]
struct HourCount {
    hour: String,
    count: usize,
}

/// Aggregated dashboard summary, serialized by `GET /api/summary`.
#[derive(Debug, Serialize)]
struct Summary {
    total: usize,
    running: usize,
    by_state: Vec<StateCount>,
    by_hour: Vec<HourCount>,
}

impl Summary {
    fn from_sessions(sessions: &[Session]) -> Self {
        Self {
            total: sessions.len(),
            running: count_running(sessions),
            by_state: count_by_state(sessions)
                .into_iter()
                .map(|(state, count)| StateCount {
                    state: state_name(state).to_string(),
                    count,
                })
                .collect(),
            by_hour: count_by_hour(sessions)
                .into_iter()
                .map(|(hour, count)| HourCount { hour, count })
                .collect(),
        }
    }
}

// ── Table filtering / sorting ────────────────────────────────────

/// Query parameters accepted by the dashboard index page.
#[derive(Debug, Default, serde::Deserialize)]
struct IndexParams {
    state: Option<String>,
    repo: Option<String>,
    sort: Option<String>,
}

/// A pre-rendered table row, decoupled from Maud so it stays unit-testable.
struct Row {
    repo: String,
    title: String,
    state: SessionState,
    create_time: Option<String>,
    update_time: Option<String>,
    url: Option<String>,
}

/// Build, filter, and sort the table rows from the sessions.
fn build_rows(
    sessions: &[Session],
    labels: &BTreeMap<String, String>,
    params: &IndexParams,
) -> Vec<Row> {
    let mut rows: Vec<Row> = sessions
        .iter()
        .map(|session| Row {
            repo: repo_label(session, labels),
            title: session_title(session),
            state: session_state(session),
            create_time: session.create_time.clone(),
            update_time: session.update_time.clone(),
            url: session.url.clone(),
        })
        .filter(|row| {
            params
                .state
                .as_deref()
                .is_none_or(|want| state_name(row.state).eq_ignore_ascii_case(want))
        })
        .filter(|row| {
            params
                .repo
                .as_deref()
                .is_none_or(|want| row.repo.eq_ignore_ascii_case(want))
        })
        .collect();

    match params.sort.as_deref() {
        Some("created_asc") => rows.sort_by(|a, b| a.create_time.cmp(&b.create_time)),
        Some("updated_desc") => rows.sort_by(|a, b| b.update_time.cmp(&a.update_time)),
        Some("updated_asc") => rows.sort_by(|a, b| a.update_time.cmp(&b.update_time)),
        // Default: newest created first.
        _ => rows.sort_by(|a, b| b.create_time.cmp(&a.create_time)),
    }
    rows
}

// ── Handlers ─────────────────────────────────────────────────────

#[get("/")]
async fn index(Query(params): Query<IndexParams>) -> Markup {
    let Some(context) = CONTEXT.get() else {
        return render_error("dashboard context is not initialized");
    };

    let sessions = match fetch_all_sessions(&context.client).await {
        Ok(sessions) => sessions,
        Err(error) => return render_error(&format!("failed to load sessions: {error}")),
    };
    let sources = match fetch_all_sources(&context.client).await {
        Ok(sources) => sources,
        Err(error) => return render_error(&format!("failed to load sources: {error}")),
    };

    let labels = source_labels(&sources);
    render_dashboard(&sessions, &labels, &params)
}

#[get("/api/summary")]
async fn api_summary() -> Result<Json<Summary>, (StatusCode, Json<serde_json::Value>)> {
    let context = CONTEXT.get().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "dashboard context is not initialized" })),
        )
    })?;

    let sessions = fetch_all_sessions(&context.client).await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("failed to load sessions: {error}") })),
        )
    })?;

    Ok(Json(Summary::from_sessions(&sessions)))
}

// ── Rendering ────────────────────────────────────────────────────

/// Shared page chrome: dark theme, inline CSS, and a 10s auto-refresh meta tag.
fn page_shell(title: &str, body: &Markup) -> Markup {
    html! {
        (PreEscaped("<!DOCTYPE html>"))
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta http-equiv="refresh" content="10";
                title { (title) }
                style { (PreEscaped(STYLES)) }
            }
            body {
                div class="wrap" {
                    (body)
                }
            }
        }
    }
}

/// Friendly error page rendered when the Jules API cannot be reached.
fn render_error(message: &str) -> Markup {
    page_shell(
        "Jules Dashboard — error",
        &html! {
            header class="topbar" {
                h1 { "Jules Dashboard" }
            }
            div class="error-card" {
                h2 { "Could not load dashboard" }
                p { (message) }
                p class="muted" { "The page will retry automatically in a few seconds." }
            }
        },
    )
}

/// Render the full populated dashboard.
#[allow(clippy::too_many_lines)]
fn render_dashboard(
    sessions: &[Session],
    labels: &BTreeMap<String, String>,
    params: &IndexParams,
) -> Markup {
    let running = count_running(sessions);
    let by_state = count_by_state(sessions);
    let by_hour = count_by_hour(sessions);
    let rows = build_rows(sessions, labels, params);
    let max_hour = by_hour.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let max_state = by_state.iter().map(|(_, c)| *c).max().unwrap_or(0);

    page_shell(
        "Jules Dashboard",
        &html! {
            header class="topbar" {
                h1 { "Jules Dashboard" }
                span class="muted" { "cross-repo session activity · auto-refreshing" }
            }

            section class="cards" {
                div class="card headline" {
                    div class="headline-num" { (running) }
                    div class="headline-label" { "sessions running now" }
                }
                div class="card" {
                    div class="stat-num" { (sessions.len()) }
                    div class="stat-label" { "total sessions" }
                }
                div class="card" {
                    div class="stat-num" { (labels.len()) }
                    div class="stat-label" { "repositories" }
                }
            }

            section class="panel" {
                h2 { "Sessions by state" }
                div class="bars" {
                    @for (state, count) in &by_state {
                        div class="bar-row" {
                            div class="bar-label" { (state_display(*state)) }
                            div class="bar-track" {
                                div class="bar-fill"
                                    style={ "width:" (bar_pct(*count, max_state)) "%;background:" (state_color(*state)) ";" } {}
                            }
                            div class="bar-count" { (count) }
                        }
                    }
                }
            }

            section class="panel" {
                h2 { "Sessions created per hour" }
                @if by_hour.is_empty() {
                    p class="muted" { "No timestamped sessions to chart." }
                } @else {
                    div class="timeline" {
                        @for (hour, count) in &by_hour {
                            div class="tl-col" title={ (hour) " · " (count) " sessions" } {
                                div class="tl-count" { (count) }
                                div class="tl-bar" style={ "height:" (col_px(*count, max_hour)) "px;" } {}
                                div class="tl-hour" { (hour_short(hour)) }
                            }
                        }
                    }
                }
            }

            section class="panel" {
                h2 { "Sessions" }
                @if let Some(state) = &params.state {
                    p class="muted" { "filtered by state = " (state) }
                }
                @if let Some(repo) = &params.repo {
                    p class="muted" { "filtered by repo = " (repo) }
                }
                table {
                    thead {
                        tr {
                            th { "Repo" }
                            th { "Title" }
                            th { "State" }
                            th { "Created" }
                            th { "Updated" }
                        }
                    }
                    tbody {
                        @for row in &rows {
                            tr {
                                td class="mono" { (row.repo) }
                                td {
                                    @if let Some(url) = &row.url {
                                        a href=(url) target="_blank" rel="noreferrer" { (truncate(&row.title, 70)) }
                                    } @else {
                                        (truncate(&row.title, 70))
                                    }
                                }
                                td {
                                    span class="badge"
                                        style={ "background:" (state_color(row.state)) "22;color:" (state_color(row.state)) ";border-color:" (state_color(row.state)) "55;" } {
                                        (state_display(row.state))
                                    }
                                }
                                td class="mono muted" { (format_time(row.create_time.as_deref())) }
                                td class="mono muted" { (format_time(row.update_time.as_deref())) }
                            }
                        }
                    }
                }
                @if rows.is_empty() {
                    p class="muted" { "No sessions match the current filters." }
                }
            }
        },
    )
}

/// Percentage width for a horizontal bar (0-100), guarding against zero max.
fn bar_pct(count: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    // Reserve a small floor so non-zero bars are always visible.
    let pct = count * 100 / max;
    pct.max(3)
}

/// Pixel height for a timeline column, guarding against zero max.
fn col_px(count: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    let px = count * 150 / max;
    px.max(6)
}

/// Shorten a `YYYY-MM-DD HH:00` key to a compact `HH:00` axis label.
fn hour_short(key: &str) -> String {
    key.split_once(' ')
        .map_or_else(|| key.to_string(), |(_, time)| time.to_string())
}

/// Inline stylesheet for the dashboard (dark theme).
const STYLES: &str = r"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body {
    margin: 0;
    background: #0b1120;
    color: #e2e8f0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
}
.wrap { max-width: 1200px; margin: 0 auto; padding: 28px 24px 60px; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
.muted { color: #94a3b8; }
.topbar { display: flex; align-items: baseline; gap: 16px; margin-bottom: 24px; }
.topbar h1 { font-size: 22px; margin: 0; letter-spacing: 0.3px; }
.cards { display: grid; grid-template-columns: 2fr 1fr 1fr; gap: 16px; margin-bottom: 24px; }
.card {
    background: #111a2e;
    border: 1px solid #1e293b;
    border-radius: 14px;
    padding: 22px 24px;
}
.card.headline {
    background: linear-gradient(135deg, #0f2a1a, #111a2e);
    border-color: #14532d;
}
.headline-num { font-size: 64px; font-weight: 700; line-height: 1; color: #22c55e; }
.headline-label { margin-top: 8px; color: #86efac; font-size: 14px; letter-spacing: 0.4px; }
.stat-num { font-size: 40px; font-weight: 700; line-height: 1; }
.stat-label { margin-top: 8px; color: #94a3b8; font-size: 13px; }
.panel {
    background: #111a2e;
    border: 1px solid #1e293b;
    border-radius: 14px;
    padding: 20px 24px;
    margin-bottom: 20px;
}
.panel h2 { font-size: 15px; margin: 0 0 16px; color: #cbd5e1; font-weight: 600; }
.bars { display: flex; flex-direction: column; gap: 10px; }
.bar-row { display: grid; grid-template-columns: 150px 1fr 44px; align-items: center; gap: 12px; }
.bar-label { font-size: 13px; color: #cbd5e1; }
.bar-track { background: #0b1120; border-radius: 6px; height: 20px; overflow: hidden; }
.bar-fill { height: 100%; border-radius: 6px; min-width: 2px; transition: width 0.2s; }
.bar-count { font-size: 13px; text-align: right; color: #e2e8f0; font-variant-numeric: tabular-nums; }
.timeline {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    overflow-x: auto;
    padding-bottom: 4px;
    min-height: 190px;
}
.tl-col { display: flex; flex-direction: column; align-items: center; justify-content: flex-end; min-width: 34px; }
.tl-count { font-size: 11px; color: #94a3b8; margin-bottom: 4px; font-variant-numeric: tabular-nums; }
.tl-bar { width: 24px; background: linear-gradient(180deg, #38bdf8, #2563eb); border-radius: 4px 4px 0 0; }
.tl-hour { font-size: 10px; color: #64748b; margin-top: 6px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
thead th {
    text-align: left;
    padding: 8px 10px;
    color: #94a3b8;
    font-weight: 600;
    border-bottom: 1px solid #1e293b;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
tbody td { padding: 9px 10px; border-bottom: 1px solid #16203a; vertical-align: top; }
tbody tr:hover { background: #0f1a30; }
a { color: #7dd3fc; text-decoration: none; }
a:hover { text-decoration: underline; }
.badge {
    display: inline-block;
    padding: 2px 9px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    border: 1px solid transparent;
    white-space: nowrap;
}
.error-card {
    background: #2a1216;
    border: 1px solid #7f1d1d;
    border-radius: 14px;
    padding: 24px;
}
.error-card h2 { margin-top: 0; color: #fca5a5; }
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GithubRepoContext, SourceContext};

    fn session(state: Option<SessionState>, create_time: Option<&str>) -> Session {
        Session {
            name: "sessions/x".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state,
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: create_time.map(ToString::to_string),
            update_time: None,
            url: None,
            source: None,
            plan: None,
            output: None,
            outputs: Vec::new(),
        }
    }

    #[test]
    fn counts_sessions_per_state() {
        let sessions = vec![
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::Completed), None),
            session(Some(SessionState::Failed), None),
            session(None, None),
        ];
        let counts = count_by_state(&sessions);
        let lookup = |state: SessionState| {
            counts
                .iter()
                .find(|(s, _)| *s == state)
                .map_or(0, |(_, c)| *c)
        };
        assert_eq!(lookup(SessionState::InProgress), 2);
        assert_eq!(lookup(SessionState::Completed), 1);
        assert_eq!(lookup(SessionState::Failed), 1);
        // Missing state falls back to Unknown.
        assert_eq!(lookup(SessionState::Unknown), 1);
        // Zero-count states are omitted entirely.
        assert!(counts.iter().all(|(_, c)| *c > 0));
    }

    #[test]
    fn headline_counts_only_active_states() {
        let sessions = vec![
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::Queued), None),
            session(Some(SessionState::Planning), None),
            session(Some(SessionState::AwaitingUserFeedback), None),
            session(Some(SessionState::Paused), None),
            session(Some(SessionState::Completed), None), // terminal
            session(Some(SessionState::Failed), None),    // terminal
            session(None, None),                          // Unknown, not active
        ];
        // 5 active: InProgress, Queued, Planning, AwaitingUserFeedback, Paused.
        assert_eq!(count_running(&sessions), 5);
    }

    #[test]
    fn buckets_by_hour_and_skips_malformed() {
        let sessions = vec![
            session(None, Some("2026-07-15T12:00:00Z")),
            session(None, Some("2026-07-15T12:59:59Z")),
            session(None, Some("2026-07-15T13:05:00Z")),
            session(None, Some("not-a-timestamp")),
            session(None, None),
        ];
        let buckets = count_by_hour(&sessions);
        assert_eq!(
            buckets,
            vec![
                ("2026-07-15 12:00".to_string(), 2),
                ("2026-07-15 13:00".to_string(), 1),
            ]
        );
    }

    #[test]
    fn hour_bucket_key_rejects_malformed() {
        assert!(hour_bucket_key("garbage").is_none());
        assert!(hour_bucket_key("2026-07-15").is_none());
        assert_eq!(
            hour_bucket_key("2026-07-15T09:22:11Z").as_deref(),
            Some("2026-07-15 09:00")
        );
    }

    #[test]
    fn repo_label_prefers_github_then_context_then_unknown() {
        let mut labels = BTreeMap::new();
        labels.insert("sources/abc".to_string(), "acme/widgets".to_string());

        // Embedded source with github metadata wins.
        let mut s1 = session(None, None);
        s1.source = Some(Source {
            name: "sources/abc".to_string(),
            id: None,
            display_name: None,
            description: None,
            github_repo: Some(GithubRepoContext {
                owner: "octo".to_string(),
                repo: "cat".to_string(),
                is_private: None,
                default_branch: None,
                branches: Vec::new(),
            }),
        });
        assert_eq!(repo_label(&s1, &labels), "octo/cat");

        // Source context resolved via labels map.
        let mut s2 = session(None, None);
        s2.source_context = Some(SourceContext {
            source: "sources/abc".to_string(),
            github_repo_context: None,
        });
        assert_eq!(repo_label(&s2, &labels), "acme/widgets");

        // No source at all.
        let s3 = session(None, None);
        assert_eq!(repo_label(&s3, &labels), "(unknown)");
    }

    #[test]
    fn truncate_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcde…");
    }
}
