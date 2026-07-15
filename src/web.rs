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

use std::collections::{BTreeMap, BTreeSet};
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
        .routes(routes![index, api_summary, delete_session_route])
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

/// Documented concurrent-task cap on the Jules **Ultra** plan. The API does not
/// expose the caller's plan, so this is the published Ultra value used as a
/// reference ceiling rather than a per-account limit.
const CONCURRENT_CAP: usize = 60;

/// Maximum number of repo rows rendered in the inferred-schedule heatmap. Extra
/// repos are collapsed into a "N more hidden" note so the grid stays readable.
const HEATMAP_MAX_ROWS: usize = 12;

/// A (repo, hour) cell is treated as a likely recurring/scheduled pattern once
/// it has appeared on at least this many distinct calendar dates.
const RECURRENCE_MIN_DAYS: usize = 2;

/// Active (non-terminal) states in display order, used for the headline
/// composition breakdown. The union of these is exactly [`is_active_state`], so
/// the composition always sums to [`count_running`].
const ACTIVE_STATES: [SessionState; 6] = [
    SessionState::InProgress,
    SessionState::Queued,
    SessionState::AwaitingUserFeedback,
    SessionState::AwaitingPlanApproval,
    SessionState::Planning,
    SessionState::Paused,
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

/// Count sessions currently in a single state.
fn count_in_state(sessions: &[Session], state: SessionState) -> usize {
    sessions
        .iter()
        .filter(|session| session_state(session) == state)
        .count()
}

/// Composition of the active (non-terminal) total: `(state, count)` for each
/// active state with a non-zero count, in [`ACTIVE_STATES`] display order. The
/// counts always sum to [`count_running`].
fn active_composition(sessions: &[Session]) -> Vec<(SessionState, usize)> {
    ACTIVE_STATES
        .iter()
        .filter_map(|&state| {
            let count = count_in_state(sessions, state);
            (count > 0).then_some((state, count))
        })
        .collect()
}

/// Percentage (0-100) of the documented concurrent cap currently in use.
fn cap_pct(active: usize) -> usize {
    (active * 100 / CONCURRENT_CAP).min(100)
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

/// Extract `(date, hour)` from an RFC3339 timestamp, where `date` is the
/// `YYYY-MM-DD` prefix and `hour` is the UTC hour `0..=23`. Reuses the fixed
/// layout validated by [`hour_bucket_key`], so malformed or out-of-range values
/// return `None`.
fn date_and_hour(timestamp: &str) -> Option<(String, u8)> {
    // Validate the RFC3339 prefix layout once, then slice out the parts.
    hour_bucket_key(timestamp)?;
    let date = timestamp.get(0..10)?.to_string();
    let hour: u8 = timestamp.get(11..13)?.parse().ok()?;
    if hour > 23 {
        return None;
    }
    Some((date, hour))
}

/// A single heatmap cell for one `(repo, hour)`: how many sessions were created
/// in that hour-of-day, and on how many distinct calendar dates (the recurrence
/// signal).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HeatCell {
    /// Total sessions created in this hour-of-day across all history.
    count: usize,
    /// Distinct calendar dates those sessions fall on.
    days: usize,
}

impl HeatCell {
    /// Whether this cell looks like a recurring/scheduled pattern.
    fn is_recurring(self) -> bool {
        self.days >= RECURRENCE_MIN_DAYS
    }
}

/// One repo's row in the inferred-schedule heatmap: 24 hourly cells (index =
/// UTC hour `0..=23`) plus the row total.
struct HeatRow {
    repo: String,
    cells: [HeatCell; 24],
    total: usize,
}

/// Aggregate sessions into per-repo, per-hour heatmap rows (UTC). Only sessions
/// with a parseable `create_time` contribute. Rows are sorted by descending
/// total activity, then repo name, so the busiest repos surface first.
fn heatmap_rows(sessions: &[Session], labels: &BTreeMap<String, String>) -> Vec<HeatRow> {
    // repo -> per-hour (session count, set of distinct dates)
    type HourAcc = [(usize, BTreeSet<String>); 24];
    let mut acc: BTreeMap<String, HourAcc> = BTreeMap::new();

    for session in sessions {
        let Some(timestamp) = session.create_time.as_deref() else {
            continue;
        };
        let Some((date, hour)) = date_and_hour(timestamp) else {
            continue;
        };
        let repo = repo_label(session, labels);
        let entry = acc
            .entry(repo)
            .or_insert_with(|| std::array::from_fn(|_| (0, BTreeSet::new())));
        let cell = &mut entry[hour as usize];
        cell.0 += 1;
        cell.1.insert(date);
    }

    let mut rows: Vec<HeatRow> = acc
        .into_iter()
        .map(|(repo, hours)| {
            let cells: [HeatCell; 24] = std::array::from_fn(|h| HeatCell {
                count: hours[h].0,
                days: hours[h].1.len(),
            });
            let total = cells.iter().map(|cell| cell.count).sum();
            HeatRow { repo, cells, total }
        })
        .collect();

    rows.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.repo.cmp(&b.repo)));
    rows
}

/// Sum of activity per hour across every repo (index = UTC hour). Drives the
/// heatmap footer and peak-hour highlighting.
fn hour_totals(rows: &[HeatRow]) -> [usize; 24] {
    let mut totals = [0usize; 24];
    for row in rows {
        for (total, cell) in totals.iter_mut().zip(row.cells.iter()) {
            *total += cell.count;
        }
    }
    totals
}

/// Flag the "peak" hours: those whose column total is at least 80% of the
/// busiest hour (and non-zero). These are where new work is most likely to bump
/// the concurrent cap.
fn peak_hours(totals: &[usize; 24]) -> [bool; 24] {
    let max = totals.iter().copied().max().unwrap_or(0);
    let mut peaks = [false; 24];
    if max == 0 {
        return peaks;
    }
    let threshold = (max * 8).div_ceil(10);
    for (peak, &total) in peaks.iter_mut().zip(totals.iter()) {
        *peak = total >= threshold && total > 0;
    }
    peaks
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
    /// Bare session id (no `sessions/` prefix), used for the delete route.
    id: String,
    repo: String,
    title: String,
    state: SessionState,
    create_time: Option<String>,
    update_time: Option<String>,
    url: Option<String>,
}

/// Bare session id (no `sessions/` prefix) suitable for the delete route path
/// segment. Prefers the explicit `id` field, falling back to the resource name.
fn session_ident(session: &Session) -> String {
    if let Some(id) = &session.id {
        if !id.trim().is_empty() {
            return id.clone();
        }
    }
    session
        .name
        .strip_prefix("sessions/")
        .unwrap_or(&session.name)
        .to_string()
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
            id: session_ident(session),
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

/// Delete a single session by bare id (for example `s-42`). The dashboard's
/// per-row ✕ control POSTs here; on success the client reloads so all headline
/// numbers recompute from fresh data.
///
/// Returns `200 OK` on success, `400 Bad Request` for an invalid id, and
/// `502 Bad Gateway` when the Jules API rejects or cannot service the deletion.
/// The API `Result` is always handled — the handler never panics on failure.
#[post("/sessions/{id}/delete")]
async fn delete_session_route(Path(id): Path<String>) -> (StatusCode, String) {
    let Some(context) = CONTEXT.get() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "dashboard context is not initialized".to_string(),
        );
    };

    match context.client.delete_session(&id).await {
        Ok(()) => (StatusCode::OK, format!("deleted session {id}")),
        Err(error) => {
            let status = match &error {
                JulesError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_GATEWAY,
            };
            (status, format!("failed to delete session {id}: {error}"))
        }
    }
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
    let by_active = active_composition(sessions);
    let by_hour = count_by_hour(sessions);
    let rows = build_rows(sessions, labels, params);
    let max_hour = by_hour.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let max_state = by_state.iter().map(|(_, c)| *c).max().unwrap_or(0);

    // Inferred-schedule heatmap data (UTC, from historical create_time).
    let heat_rows = heatmap_rows(sessions, labels);
    let heat_totals = hour_totals(&heat_rows);
    let heat_peaks = peak_hours(&heat_totals);
    let heat_max_cell = heat_rows
        .iter()
        .flat_map(|row| row.cells.iter())
        .map(|cell| cell.count)
        .max()
        .unwrap_or(0);
    let heat_hidden = heat_rows.len().saturating_sub(HEATMAP_MAX_ROWS);

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
                    div class="headline-label" { "active sessions (non-terminal)" }
                    @if by_active.is_empty() {
                        div class="headline-sub muted" { "nothing running right now" }
                    } @else {
                        div class="composition" {
                            @for (state, count) in &by_active {
                                span class="chip"
                                    style={ "border-color:" (state_color(*state)) "66;color:" (state_color(*state)) ";" } {
                                    span class="chip-dot" style={ "background:" (state_color(*state)) ";" } {}
                                    (count) " " (state_display(*state))
                                }
                            }
                        }
                    }
                    div class="capacity" {
                        div class="capacity-track" {
                            div class="capacity-fill" style={ "width:" (cap_pct(running)) "%;" } {}
                        }
                        div class="capacity-label" {
                            (running) " / " (CONCURRENT_CAP)
                            span class="muted" { " · Ultra plan limit: " (CONCURRENT_CAP) " concurrent tasks" }
                        }
                        div class="capacity-note muted" {
                            "Jules' docs don't specify whether queued sessions count toward the "
                            (CONCURRENT_CAP) "-concurrent cap, so treat this as an upper bound."
                        }
                    }
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
                h2 { "Inferred schedule — load by hour (UTC)" }
                p class="disclaimer" {
                    "The Jules API does not expose scheduled tasks — this is inferred from "
                    "when sessions were historically created (UTC), not the actual schedule."
                }
                @if heat_rows.is_empty() {
                    p class="muted" {
                        "Not enough history to infer a schedule yet — no sessions have a "
                        "parseable creation time."
                    }
                } @else {
                    div class="heatmap-scroll" {
                        div class="heatmap" {
                            div class="heat-row heat-head" {
                                div class="heat-repo" { "repo \\ hour" }
                                @for hour in 0u8..24 {
                                    div class="heat-hh" { (format!("{hour:02}")) }
                                }
                            }
                            @for row in heat_rows.iter().take(HEATMAP_MAX_ROWS) {
                                div class="heat-row" {
                                    div class="heat-repo mono" title=(row.repo) { (truncate(&row.repo, 22)) }
                                    @for (hour, cell) in row.cells.iter().enumerate() {
                                        div.heat-cell.recurring[cell.is_recurring()]
                                            style=(heat_style(cell.count, heat_max_cell))
                                            title=(cell_title(&row.repo, hour, *cell)) {
                                            @if cell.count > 0 { (cell.count) }
                                        }
                                    }
                                }
                            }
                            div class="heat-row heat-foot" {
                                div class="heat-repo" { "all repos" }
                                @for (total, peak) in heat_totals.iter().zip(heat_peaks.iter()) {
                                    div.heat-tot.peak[*peak] { (total) }
                                }
                            }
                        }
                    }
                    @if heat_hidden > 0 {
                        p class="muted" {
                            "Showing the top " (HEATMAP_MAX_ROWS) " repos by activity · "
                            (heat_hidden) " more hidden."
                        }
                    }
                    p class="caption muted" {
                        "Ringed cells recurred on ≥ " (RECURRENCE_MIN_DAYS)
                        " distinct dates — a likely recurring pattern. Highlighted footer hours "
                        "are peak load, where new sessions are most likely to bump the "
                        (CONCURRENT_CAP) "-concurrent cap."
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
                            th class="kill-col" aria-label="Delete" { "" }
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
                                td class="kill-cell" {
                                    button type="button" class="kill"
                                        data-id=(row.id)
                                        data-title=(truncate(&row.title, 60))
                                        title="Delete session"
                                        aria-label="Delete session" { "✕" }
                                }
                            }
                        }
                    }
                }
                @if rows.is_empty() {
                    p class="muted" { "No sessions match the current filters." }
                }
            }

            script { (PreEscaped(DELETE_SCRIPT)) }
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

/// Inline background style for a heatmap cell, scaling the fill opacity by
/// intensity relative to the busiest cell. Zero-count cells stay dark.
fn heat_style(count: usize, max: usize) -> String {
    if count == 0 || max == 0 {
        return "background:#0b1120;".to_string();
    }
    // Integer alpha ramp 22%..100% (no float casts, pedantic-clippy friendly).
    let alpha = 22 + (count * 78 / max);
    format!("background:rgb(56 189 248 / {alpha}%);")
}

/// Tooltip text for a heatmap cell.
fn cell_title(repo: &str, hour: usize, cell: HeatCell) -> String {
    if cell.count == 0 {
        format!("{repo} · {hour:02}:00 UTC · no sessions")
    } else {
        format!(
            "{repo} · {hour:02}:00 UTC · {} sessions across {} days",
            cell.count, cell.days
        )
    }
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
.headline-sub { margin-top: 10px; font-size: 13px; }
.composition { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 14px; }
.chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    border: 1px solid transparent;
    background: #0b1120;
    font-variant-numeric: tabular-nums;
}
.chip-dot { width: 8px; height: 8px; border-radius: 999px; display: inline-block; }
.capacity { margin-top: 16px; }
.capacity-track {
    background: #0b1120;
    border: 1px solid #1e293b;
    border-radius: 6px;
    height: 12px;
    overflow: hidden;
}
.capacity-fill {
    height: 100%;
    background: linear-gradient(90deg, #22c55e, #f59e0b);
    border-radius: 6px;
    min-width: 2px;
    transition: width 0.2s;
}
.capacity-label { margin-top: 8px; font-size: 13px; font-weight: 600; font-variant-numeric: tabular-nums; }
.capacity-label .muted { font-weight: 400; }
.capacity-note { margin-top: 6px; font-size: 11px; line-height: 1.4; }
.disclaimer {
    margin: -6px 0 16px;
    font-size: 12px;
    color: #fbbf24;
    background: #2a2410;
    border: 1px solid #4d3c12;
    border-radius: 8px;
    padding: 8px 12px;
    line-height: 1.4;
}
.caption { margin-top: 12px; font-size: 12px; line-height: 1.4; }
.heatmap-scroll { overflow-x: auto; padding-bottom: 4px; }
.heatmap { display: flex; flex-direction: column; gap: 3px; min-width: 760px; }
.heat-row {
    display: grid;
    grid-template-columns: 150px repeat(24, 1fr);
    gap: 3px;
    align-items: stretch;
}
.heat-repo {
    display: flex;
    align-items: center;
    font-size: 12px;
    color: #cbd5e1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-right: 6px;
}
.heat-head .heat-repo, .heat-foot .heat-repo { color: #94a3b8; font-weight: 600; }
.heat-hh {
    text-align: center;
    font-size: 10px;
    color: #64748b;
    font-variant-numeric: tabular-nums;
}
.heat-cell {
    position: relative;
    height: 26px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: #0b1120;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    border: 1px solid #16203a;
}
.heat-cell.recurring {
    box-shadow: 0 0 0 2px #f59e0b inset;
    border-color: #f59e0b;
}
.heat-tot {
    height: 22px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: #94a3b8;
    font-variant-numeric: tabular-nums;
    background: #0b1120;
    border: 1px solid #16203a;
}
.heat-tot.peak {
    color: #0b1120;
    background: #f59e0b;
    font-weight: 700;
    border-color: #f59e0b;
}
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
.kill-col { width: 34px; }
.kill-cell { text-align: center; width: 34px; }
.kill {
    background: transparent;
    border: 1px solid transparent;
    color: #475569;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 2px 7px;
    border-radius: 6px;
    font-variant-numeric: tabular-nums;
    transition: color 0.15s, background 0.15s, border-color 0.15s;
}
.kill:hover { color: #ef4444; background: #ef444422; border-color: #ef444455; }
.kill.armed { color: #fff; background: #ef4444; border-color: #ef4444; font-weight: 700; }
.kill:disabled { opacity: 0.55; cursor: default; }
";

/// Inline script powering the per-row ✕ delete control. First click *arms* the
/// button (a two-click confirm, no modal); a second click within a few seconds
/// POSTs to `/sessions/{id}/delete`. On success the page reloads so every
/// headline/heatmap/count recomputes from fresh data; on failure it surfaces
/// the server message and leaves the page untouched.
const DELETE_SCRIPT: &str = r#"
(function () {
    function disarm(btn) {
        btn.dataset.armed = '0';
        btn.classList.remove('armed');
        btn.textContent = '✕';
        btn.title = 'Delete session';
        if (btn._t) { clearTimeout(btn._t); btn._t = null; }
    }
    document.addEventListener('click', function (e) {
        var btn = e.target.closest('.kill');
        if (!btn) { return; }
        e.preventDefault();
        if (btn.disabled) { return; }
        if (btn.dataset.armed !== '1') {
            document.querySelectorAll('.kill[data-armed="1"]').forEach(disarm);
            btn.dataset.armed = '1';
            btn.classList.add('armed');
            btn.textContent = '✕?';
            btn.title = 'Click again to confirm delete of ' + (btn.dataset.title || btn.dataset.id);
            btn._t = setTimeout(function () { disarm(btn); }, 4000);
            return;
        }
        if (btn._t) { clearTimeout(btn._t); btn._t = null; }
        btn.disabled = true;
        btn.textContent = '…';
        var id = btn.dataset.id;
        fetch('/sessions/' + encodeURIComponent(id) + '/delete', { method: 'POST' })
            .then(function (r) {
                if (r.ok) { location.reload(); return; }
                return r.text().then(function (t) {
                    window.alert('Delete failed: ' + (t || ('HTTP ' + r.status)));
                    btn.disabled = false;
                    disarm(btn);
                });
            })
            .catch(function (err) {
                window.alert('Delete failed: ' + err);
                btn.disabled = false;
                disarm(btn);
            });
    });
})();
"#;

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

    /// Build a session tagged with a GitHub repo and a creation timestamp.
    fn session_repo(repo: &str, create_time: &str) -> Session {
        let mut s = session(Some(SessionState::InProgress), Some(create_time));
        let (owner, name) = repo.split_once('/').unwrap();
        s.source = Some(Source {
            name: format!("sources/{repo}"),
            id: None,
            display_name: None,
            description: None,
            github_repo: Some(GithubRepoContext {
                owner: owner.to_string(),
                repo: name.to_string(),
                is_private: None,
                default_branch: None,
                branches: Vec::new(),
            }),
        });
        s
    }

    #[test]
    fn count_in_state_and_active_composition() {
        let sessions = vec![
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::Queued), None),
            session(Some(SessionState::AwaitingUserFeedback), None),
            session(Some(SessionState::Completed), None), // terminal, excluded
            session(Some(SessionState::Failed), None),    // terminal, excluded
        ];
        assert_eq!(count_in_state(&sessions, SessionState::InProgress), 2);
        assert_eq!(count_in_state(&sessions, SessionState::Queued), 1);
        assert_eq!(count_in_state(&sessions, SessionState::Paused), 0);

        // Composition is in ACTIVE_STATES order, non-zero only, and sums to the
        // running total.
        let comp = active_composition(&sessions);
        assert_eq!(
            comp,
            vec![
                (SessionState::InProgress, 2),
                (SessionState::Queued, 1),
                (SessionState::AwaitingUserFeedback, 1),
            ]
        );
        let sum: usize = comp.iter().map(|(_, c)| *c).sum();
        assert_eq!(sum, count_running(&sessions));
    }

    #[test]
    fn cap_pct_clamps_to_100() {
        assert_eq!(cap_pct(0), 0);
        assert_eq!(cap_pct(30), 50);
        assert_eq!(cap_pct(60), 100);
        assert_eq!(cap_pct(120), 100); // over the cap still clamps
    }

    #[test]
    fn date_and_hour_extracts_utc_parts() {
        assert_eq!(
            date_and_hour("2026-07-15T09:22:11Z"),
            Some(("2026-07-15".to_string(), 9))
        );
        assert_eq!(
            date_and_hour("2026-01-02T23:59:59Z"),
            Some(("2026-01-02".to_string(), 23))
        );
        // Malformed / out-of-range inputs are rejected.
        assert_eq!(date_and_hour("garbage"), None);
        assert_eq!(date_and_hour("2026-07-15"), None);
    }

    #[test]
    fn heatmap_rows_aggregate_and_detect_recurrence() {
        let labels = BTreeMap::new();
        let sessions = vec![
            // acme/api at hour 9 across two distinct dates (3 sessions total).
            session_repo("acme/api", "2026-07-10T09:00:00Z"),
            session_repo("acme/api", "2026-07-11T09:30:00Z"),
            session_repo("acme/api", "2026-07-11T09:45:00Z"),
            // acme/web: one session, single date/hour.
            session_repo("acme/web", "2026-07-10T14:00:00Z"),
        ];
        let rows = heatmap_rows(&sessions, &labels);
        assert_eq!(rows.len(), 2);

        // Busiest repo first.
        assert_eq!(rows[0].repo, "acme/api");
        assert_eq!(rows[0].total, 3);
        let api_9 = rows[0].cells[9];
        assert_eq!(api_9.count, 3);
        assert_eq!(api_9.days, 2);
        assert!(api_9.is_recurring()); // >= 2 distinct dates

        assert_eq!(rows[1].repo, "acme/web");
        let web_14 = rows[1].cells[14];
        assert_eq!(web_14.count, 1);
        assert_eq!(web_14.days, 1);
        assert!(!web_14.is_recurring());

        // Column totals + peak detection.
        let totals = hour_totals(&rows);
        assert_eq!(totals[9], 3);
        assert_eq!(totals[14], 1);
        assert_eq!(totals[0], 0);
        let peaks = peak_hours(&totals);
        assert!(peaks[9]); // busiest hour is a peak
        assert!(!peaks[0]); // empty hour is never a peak
    }

    #[test]
    fn heatmap_rows_skip_sessions_without_timestamps() {
        let labels = BTreeMap::new();
        let sessions = vec![
            session_repo("acme/api", "not-a-timestamp"),
            session(Some(SessionState::InProgress), None),
        ];
        assert!(heatmap_rows(&sessions, &labels).is_empty());
    }

    #[test]
    fn truncate_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcde…");
    }

    #[test]
    fn session_ident_prefers_id_then_strips_prefix() {
        // Missing id falls back to the resource name with the prefix stripped.
        let mut s = session(None, None); // name = "sessions/x"
        assert_eq!(session_ident(&s), "x");
        // An explicit id wins.
        s.id = Some("s-99".to_string());
        assert_eq!(session_ident(&s), "s-99");
    }

    #[test]
    fn session_row_renders_delete_control() {
        let mut s = session(Some(SessionState::Queued), Some("2026-07-15T09:00:00Z"));
        s.id = Some("s-42".to_string());
        s.title = Some("zombie task".to_string());

        let labels = BTreeMap::new();
        let params = IndexParams::default();
        let html = render_dashboard(std::slice::from_ref(&s), &labels, &params).into_string();

        // The ✕ button carries the bare session id used to build the route.
        assert!(html.contains(r#"class="kill""#));
        assert!(html.contains(r#"data-id="s-42""#));
        assert!(html.contains(r#"aria-label="Delete session""#));
        // The inline script POSTs to /sessions/{id}/delete for that id.
        assert!(html.contains("'/sessions/' + encodeURIComponent(id) + '/delete'"));
    }

    /// Renders the dashboard with a synthetic dataset and writes it to disk for
    /// manual/visual inspection. Ignored by default; run with:
    /// `cargo test --features web -- --ignored render_demo_dashboard --nocapture`
    #[test]
    #[ignore = "writes an HTML file for manual screenshotting"]
    fn render_demo_dashboard() {
        let mut sessions: Vec<Session> = Vec::new();
        let mut push = |repo: &str, state: SessionState, ts: &str| {
            let mut s = session_repo(repo, ts);
            s.state = Some(state);
            s.title = Some(format!("{repo}: automated task"));
            sessions.push(s);
        };

        // Recurring pattern: acme/api at 09:00 UTC across five distinct dates.
        for day in 10..=14 {
            push(
                "acme/api",
                SessionState::InProgress,
                &format!("2026-07-{day:02}T09:{:02}:00Z", day * 3 % 60),
            );
        }
        // A spread of InProgress work concentrated in the afternoon peak.
        let repos = ["acme/api", "acme/web", "acme/worker", "octo/cat", "globex/billing"];
        for i in 0..26 {
            let repo = repos[i % repos.len()];
            let hour = 13 + (i % 4); // 13..16 afternoon peak
            let day = 10 + (i % 5);
            push(
                repo,
                SessionState::InProgress,
                &format!("2026-07-{day:02}T{hour:02}:{:02}:00Z", i * 7 % 60),
            );
        }
        // Queued backlog spread across morning/evening hours.
        for i in 0..29 {
            let repo = repos[(i + 2) % repos.len()];
            let hour = (7 + i * 3) % 24;
            let day = 11 + (i % 4);
            push(
                repo,
                SessionState::Queued,
                &format!("2026-07-{day:02}T{hour:02}:{:02}:00Z", i * 11 % 60),
            );
        }
        // Awaiting feedback, planning, paused, and completed sessions.
        for i in 0..4 {
            push(
                repos[i % repos.len()],
                SessionState::AwaitingUserFeedback,
                &format!("2026-07-13T10:{:02}:00Z", i * 9),
            );
        }
        for i in 0..3 {
            push(
                repos[i % repos.len()],
                SessionState::Planning,
                &format!("2026-07-12T16:{:02}:00Z", i * 13),
            );
        }
        for i in 0..2 {
            push(
                repos[i % repos.len()],
                SessionState::Paused,
                &format!("2026-07-14T20:{:02}:00Z", i * 17),
            );
        }
        for i in 0..5 {
            push(
                repos[i % repos.len()],
                SessionState::Completed,
                &format!("2026-07-11T18:{:02}:00Z", i * 7),
            );
        }

        // Populate the labels map so the "repositories" stat is accurate.
        let mut labels = BTreeMap::new();
        for repo in repos {
            labels.insert(format!("sources/{repo}"), repo.to_string());
        }

        let params = IndexParams::default();
        let html = render_dashboard(&sessions, &labels, &params).into_string();

        let out = std::env::var("DEMO_OUT").unwrap_or_else(|_| "dashboard.html".to_string());
        std::fs::write(&out, html).expect("write dashboard html");
        eprintln!(
            "wrote {out} · {} sessions · running={}",
            sessions.len(),
            count_running(&sessions)
        );
    }
}
