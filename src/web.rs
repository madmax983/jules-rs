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
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use autumn_web::prelude::*;
use serde::Serialize;
use tokio::task::JoinSet;

use crate::github::{GithubClient, JulesMatcher, Pr};
use crate::{
    JulesClient, JulesError, ListSessionsParams, ListSourcesParams, Session, SessionState, Source,
    TimeoutPolicy,
};

/// Short-TTL for the in-memory dashboard cache. Slightly under the 10s
/// meta-refresh so data stays fresh but rapid tab clicks (and the auto-refresh)
/// are served from cache instead of re-hitting the Jules and GitHub APIs.
const CACHE_TTL: Duration = Duration::from_secs(8);

/// A value stamped with the [`Instant`] it was fetched, for TTL checks.
struct TimedCache<T> {
    fetched_at: Instant,
    value: T,
}

/// Process-wide, short-lived cache shared behind a [`Mutex`]. Holds the Jules
/// `list_sessions`/`list_sources` results (used by every session tab) and the
/// per-repo GitHub PR listing (used by the PRs tab), each keyed by fetch time.
#[derive(Default)]
struct DashboardCache {
    sessions: Option<TimedCache<Arc<Vec<Session>>>>,
    sources: Option<TimedCache<Arc<Vec<Source>>>>,
    prs: BTreeMap<(String, String), TimedCache<Arc<Vec<Pr>>>>,
}

/// Whether a cache entry fetched at `fetched_at` is still fresh at `now` under
/// `ttl`. Pure and unit-tested so the TTL logic needs no live fetches.
fn is_fresh(fetched_at: Instant, now: Instant, ttl: Duration) -> bool {
    now.duration_since(fetched_at) < ttl
}

/// Shared, process-wide request context. Built once at startup and read by the
/// stateless route handlers registered through the `routes!` macro.
struct WebContext {
    client: JulesClient,
    /// GitHub client for the PRs tab, present only when `GITHUB_TOKEN` is set.
    github: Option<Arc<GithubClient>>,
    /// Short-TTL cache so switching tabs / the 10s auto-refresh reuse recent
    /// data instead of re-hitting the upstream APIs on every request.
    cache: Arc<Mutex<DashboardCache>>,
}

/// Lock the cache mutex, recovering the guard even if a previous holder panicked
/// (a poisoned cache is never worse than a stale one).
fn lock_cache(cache: &Mutex<DashboardCache>) -> std::sync::MutexGuard<'_, DashboardCache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
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
    let github = GithubClient::from_env().map(Arc::new);
    CONTEXT
        .set(WebContext {
            client,
            github,
            cache: Arc::new(Mutex::new(DashboardCache::default())),
        })
        .map_err(|_| "web context was already initialized".to_string())?;

    autumn_web::app()
        .routes(routes![
            index,
            api_summary,
            delete_session_route,
            close_all_prs
        ])
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

/// Serve an `Option`-slot cache: return the cached value while it is still fresh,
/// otherwise run `fetch`, store the result in the slot `slot` selects, and return
/// it. The mutex guard is always dropped before `fetch.await`, so no lock is held
/// across the network call. Backs [`cached_sessions`] and [`cached_sources`].
async fn cached_slot<T, Fut>(
    cache: &Mutex<DashboardCache>,
    slot: impl for<'a> Fn(&'a mut DashboardCache) -> &'a mut Option<TimedCache<Arc<T>>>,
    fetch: Fut,
) -> Result<Arc<T>, JulesError>
where
    Fut: std::future::Future<Output = Result<T, JulesError>>,
{
    let now = Instant::now();
    {
        let mut guard = lock_cache(cache);
        if let Some(entry) = slot(&mut guard) {
            if is_fresh(entry.fetched_at, now, CACHE_TTL) {
                return Ok(Arc::clone(&entry.value));
            }
        }
    }
    let fresh = Arc::new(fetch.await?);
    {
        let mut guard = lock_cache(cache);
        *slot(&mut guard) = Some(TimedCache {
            fetched_at: Instant::now(),
            value: Arc::clone(&fresh),
        });
    }
    Ok(fresh)
}

/// Return every session, served from the short-TTL cache when a recent fetch is
/// still fresh and re-fetching (then caching) otherwise. The mutex guard is
/// always dropped before the `await`, so no lock is held across the network call.
async fn cached_sessions(context: &WebContext) -> Result<Arc<Vec<Session>>, JulesError> {
    cached_slot(
        &context.cache,
        |cache| &mut cache.sessions,
        fetch_all_sessions(&context.client),
    )
    .await
}

/// Return every source, served from the short-TTL cache when fresh. Mirrors
/// [`cached_sessions`]; the lock is never held across the `await`.
async fn cached_sources(context: &WebContext) -> Result<Arc<Vec<Source>>, JulesError> {
    cached_slot(
        &context.cache,
        |cache| &mut cache.sources,
        fetch_all_sources(&context.client),
    )
    .await
}

/// List one repo's open pulls, served from the short-TTL cache when fresh. Used
/// by the PRs-tab render only; the destructive close path always re-fetches
/// live (uncached) so its re-classification is authoritative. The lock is never
/// held across the `await`.
async fn cached_open_pulls(
    github: &GithubClient,
    cache: &Mutex<DashboardCache>,
    owner: &str,
    repo: &str,
) -> Result<Arc<Vec<Pr>>, crate::github::GithubError> {
    let key = (owner.to_string(), repo.to_string());
    let now = Instant::now();
    {
        let guard = lock_cache(cache);
        if let Some(entry) = guard.prs.get(&key) {
            if is_fresh(entry.fetched_at, now, CACHE_TTL) {
                return Ok(Arc::clone(&entry.value));
            }
        }
    }
    let fresh = Arc::new(github.list_open_pulls(owner, repo).await?);
    {
        let mut guard = lock_cache(cache);
        guard.prs.insert(
            key,
            TimedCache {
                fetched_at: Instant::now(),
                value: Arc::clone(&fresh),
            },
        );
    }
    Ok(fresh)
}

/// Drop any cached PR listing for `owner/repo` so the next PRs-tab render
/// re-fetches live. Called after a mass-close so the tab reflects the closes.
fn invalidate_pr_cache(cache: &Mutex<DashboardCache>, owner: &str, repo: &str) {
    lock_cache(cache)
        .prs
        .remove(&(owner.to_string(), repo.to_string()));
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
    /// Selected dashboard tab. Drives which view renders. Absent or unknown
    /// values fall back to the default `in-progress` landing view. Kept in the
    /// query string so the 10s meta-refresh preserves the active tab.
    tab: Option<String>,
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

/// Construct a single table [`Row`] from a session, decoupled from any
/// filtering so it can be reused by both the Sessions table and the dedicated
/// In Progress view.
fn make_row(session: &Session, labels: &BTreeMap<String, String>) -> Row {
    Row {
        id: session_ident(session),
        repo: repo_label(session, labels),
        title: session_title(session),
        state: session_state(session),
        create_time: session.create_time.clone(),
        update_time: session.update_time.clone(),
        url: session.url.clone(),
    }
}

/// Every session currently in the `InProgress` state, across all repos, in the
/// order they were supplied. Callers sort as needed. Pure and unit-tested.
fn sessions_in_progress(sessions: &[Session]) -> Vec<&Session> {
    sessions
        .iter()
        .filter(|session| session_state(session) == SessionState::InProgress)
        .collect()
}

/// Build, filter, and sort the table rows from the sessions.
fn build_rows(
    sessions: &[Session],
    labels: &BTreeMap<String, String>,
    params: &IndexParams,
) -> Vec<Row> {
    let mut rows: Vec<Row> = sessions
        .iter()
        .map(|session| make_row(session, labels))
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

    let sessions = match cached_sessions(context).await {
        Ok(sessions) => sessions,
        Err(error) => return render_error(&format!("failed to load sessions: {error}")),
    };
    let sources = match cached_sources(context).await {
        Ok(sources) => sources,
        Err(error) => return render_error(&format!("failed to load sources: {error}")),
    };

    let labels = source_labels(&sources);

    // The PRs tab needs async, network-fetched data — do the GitHub work only
    // when it is the selected tab so the other tabs stay fast.
    if selected_tab(&params) == "prs" {
        let Some(github) = context.github.as_ref() else {
            return render_prs_dashboard(&[], false);
        };
        let matcher = JulesMatcher::from_env();
        let repos = distinct_repos(&labels);
        let groups = fetch_pr_groups(
            Arc::clone(github),
            Arc::clone(&context.cache),
            &matcher,
            repos,
            current_unix(),
        )
        .await;
        return render_prs_dashboard(&groups, true);
    }

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

/// Form fields `POSTed` by the guarded "close all Jules PRs" control.
///
/// Every field beyond `owner`/`repo`/`confirm` is optional so a slightly
/// different browser payload (an unchecked checkbox omits its field entirely,
/// an older cached page omits `shown`) still deserializes instead of rejecting
/// the whole form with a 4xx and closing nothing.
#[derive(Debug, serde::Deserialize)]
struct CloseAllForm {
    owner: String,
    repo: String,
    /// The typed `owner/repo` confirmation string.
    confirm: String,
    /// Present (`"on"`/`"true"`/`"1"`) only when the "delete head branches" box
    /// was checked. An HTML checkbox omits its field entirely when unchecked, so
    /// this MUST stay optional — a `bool` here would fail to deserialize and
    /// reject the whole form.
    #[serde(default)]
    delete_branches: Option<String>,
    /// Comma-separated PR numbers the page listed for this repo, so the server
    /// can report any that were shown but no longer classify as Jules on
    /// re-fetch. Optional: absence just skips the reconciliation.
    #[serde(default)]
    shown: Option<String>,
}

/// Parse a comma-separated list of PR numbers (as carried in the `shown` hidden
/// field), skipping blanks and unparseable entries.
fn parse_pr_numbers(raw: Option<&str>) -> Vec<u64> {
    raw.map(|value| {
        value
            .split(',')
            .filter_map(|part| part.trim().parse::<u64>().ok())
            .collect()
    })
    .unwrap_or_default()
}

/// Guarded mass-close of every Jules-authored open PR in one repository.
///
/// Safety invariants enforced here, server-side, regardless of page state:
/// 1. `confirm` must equal `owner/repo` exactly (whitespace-trimmed), or the
///    request is rejected and nothing is closed.
/// 2. The open PR list is re-fetched and re-classified with [`JulesMatcher`];
///    only PRs the server independently flags as Jules are ever closed. A
///    client-supplied list of PR numbers is never trusted for *closing* — it is
///    used only to report PRs that were *shown* but no longer match.
///
/// The handler never fails silently: it always renders a results page (no
/// auto-refresh) that states exactly what matched, closed, failed, or no longer
/// matches, with an explicit reason banner whenever zero PRs were closed. It
/// also emits `eprintln!` diagnostics (token presence as a bool only — never the
/// value — plus the confirm result and per-PR close status).
#[post("/prs/close-all")]
async fn close_all_prs(Form(form): Form<CloseAllForm>) -> (StatusCode, Markup) {
    let Some(context) = CONTEXT.get() else {
        eprintln!("[close-all] context not initialized");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            render_close_error("dashboard context is not initialized"),
        );
    };
    let Some(github) = context.github.as_ref() else {
        eprintln!("[close-all] token_present=false — GITHUB_TOKEN not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            render_close_error(
                "GITHUB_TOKEN is not configured on the server, so PRs cannot be closed.",
            ),
        );
    };

    let owner = form.owner.trim().to_string();
    let repo = form.repo.trim().to_string();
    let expected = format!("{owner}/{repo}");
    let matcher = JulesMatcher::from_env();
    let listed = parse_pr_numbers(form.shown.as_deref());
    let confirm_ok = form.confirm.trim() == expected;

    // 1. Confirm gate. On mismatch, render the results page (not a bare 4xx) so
    //    the user sees the exact reason nothing was closed.
    if !confirm_ok {
        eprintln!(
            "[close-all] repo={expected} token_present=true confirm_ok=false shown={} → nothing closed",
            listed.len()
        );
        let summary = build_close_summary(&form.confirm, &owner, &repo, &listed, &[], Vec::new());
        return (
            StatusCode::BAD_REQUEST,
            render_close_result(&summary, &matcher),
        );
    }

    let delete_branches = form
        .delete_branches
        .as_deref()
        .is_some_and(|value| matches!(value, "on" | "true" | "1"));

    // 2. Re-fetch and re-classify server-side — never trust the page's list for
    //    what to close. Always fetch live (uncached) so this is authoritative.
    let targets: Vec<Pr> = match github.list_open_pulls(&owner, &repo).await {
        Ok(prs) => prs.into_iter().filter(|pr| matcher.is_jules(pr)).collect(),
        Err(error) => {
            eprintln!("[close-all] repo={expected} list_open_pulls failed: {error}");
            return (
                StatusCode::BAD_GATEWAY,
                render_close_error(&format!(
                    "Failed to list open pulls for {expected}, so nothing was closed: {error}"
                )),
            );
        }
    };
    let reclassified: Vec<u64> = targets.iter().map(|pr| pr.number).collect();

    eprintln!(
        "[close-all] repo={expected} token_present=true confirm_ok=true delete_branches={delete_branches} shown={} matched={}",
        listed.len(),
        reclassified.len()
    );

    let outcomes = close_prs(Arc::clone(github), &owner, &repo, targets, delete_branches).await;
    for outcome in &outcomes {
        eprintln!(
            "[close-all] repo={expected} pr=#{} ok={} msg={}",
            outcome.number, outcome.ok, outcome.message
        );
    }

    let mut summary = build_close_summary(&form.confirm, &owner, &repo, &listed, &reclassified, outcomes);
    summary.delete_branches = delete_branches;

    eprintln!(
        "[close-all] repo={expected} matched={} closed={} failed={} shown_but_unmatched={}",
        summary.matched,
        summary.closed,
        summary.failed,
        summary.unmatched.len()
    );

    // Reflect the closes on the PRs tab immediately.
    if summary.closed > 0 {
        invalidate_pr_cache(&context.cache, &owner, &repo);
    }

    (StatusCode::OK, render_close_result(&summary, &matcher))
}

/// Pure results model for a mass-close, decoupled from both rendering and the
/// network so the summary logic is unit-testable. Built by
/// [`build_close_summary`] from the typed confirmation, the repo, the PR numbers
/// the page *showed*, the numbers the server *re-classified* as Jules, and the
/// per-PR close outcomes.
struct CloseSummary {
    /// `owner/repo` this close targeted.
    target: String,
    /// Whether the typed confirmation matched `target` (trimmed).
    confirm_ok: bool,
    /// Whether head-branch deletion was requested (set by the handler).
    delete_branches: bool,
    /// How many PRs the page showed for this repo.
    shown: usize,
    /// How many PRs the server independently re-classified as Jules.
    matched: usize,
    /// How many closed successfully.
    closed: usize,
    /// How many close attempts failed.
    failed: usize,
    /// PR numbers shown on the page that no longer classify as Jules on re-fetch
    /// (or are no longer open) and were therefore NOT closed.
    unmatched: Vec<u64>,
    /// Per-PR close outcomes, sorted by PR number.
    outcomes: Vec<CloseOutcome>,
}

impl CloseSummary {
    /// An explicit reason nothing was closed, or `None` when at least one PR
    /// closed. Drives the results-page banner so the outcome is never ambiguous.
    fn zero_reason(&self, matcher: &JulesMatcher) -> Option<String> {
        if self.closed > 0 {
            return None;
        }
        if !self.confirm_ok {
            return Some(format!(
                "Confirmation text did not match `{}`, so nothing was closed.",
                self.target
            ));
        }
        if self.matched == 0 {
            return Some(format!(
                "No open PRs matched the Jules filter on re-fetch (authors=[{}], marker='{}'), so nothing was closed.",
                matcher.authors_display(),
                matcher.marker_display()
            ));
        }
        // matched > 0 but none closed → every attempt failed.
        Some(format!(
            "All {} matched {} failed to close — see the per-PR errors below.",
            self.matched,
            pr_word(self.matched)
        ))
    }
}

/// Build the pure [`CloseSummary`] from the raw inputs. `listed` is what the
/// page showed; `reclassified` is what the server independently flagged as
/// Jules; `outcomes` is the result of attempting to close the reclassified set.
fn build_close_summary(
    typed_confirm: &str,
    owner: &str,
    repo: &str,
    listed: &[u64],
    reclassified: &[u64],
    outcomes: Vec<CloseOutcome>,
) -> CloseSummary {
    let target = format!("{owner}/{repo}");
    let confirm_ok = typed_confirm.trim() == target;
    let matched = reclassified.len();
    let closed = outcomes.iter().filter(|outcome| outcome.ok).count();
    let failed = outcomes.len() - closed;

    let matched_set: BTreeSet<u64> = reclassified.iter().copied().collect();
    let mut unmatched: Vec<u64> = listed
        .iter()
        .copied()
        .filter(|number| !matched_set.contains(number))
        .collect();
    unmatched.sort_unstable();
    unmatched.dedup();

    CloseSummary {
        target,
        confirm_ok,
        delete_branches: false,
        shown: listed.len(),
        matched,
        closed,
        failed,
        unmatched,
        outcomes,
    }
}

// ── Rendering ────────────────────────────────────────────────────

/// Shared page chrome: dark theme and inline CSS. When `auto_refresh` is set a
/// 10s meta-refresh tag is emitted; the PRs tab and the close-all results page
/// pass `false` so the refresh cannot interrupt typing a confirmation or re-hit
/// GitHub on a timer.
fn page_shell(title: &str, body: &Markup, auto_refresh: bool) -> Markup {
    html! {
        (PreEscaped("<!DOCTYPE html>"))
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                @if auto_refresh {
                    meta http-equiv="refresh" content="10";
                }
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
        true,
    )
}

/// The tabs shown in the dashboard nav, in display order: `(slug, label)`.
const TABS: [(&str, &str); 5] = [
    ("in-progress", "In Progress"),
    ("overview", "Overview"),
    ("sessions", "Sessions"),
    ("schedule", "Schedule"),
    ("prs", "PRs"),
];

/// Resolve the selected tab slug from the query params, defaulting to the
/// `in-progress` landing view for absent or unrecognized values.
fn selected_tab(params: &IndexParams) -> &'static str {
    match params.tab.as_deref() {
        Some("overview") => "overview",
        Some("sessions") => "sessions",
        Some("schedule") => "schedule",
        Some("prs") => "prs",
        _ => "in-progress",
    }
}

/// The tab navigation bar, with the active tab highlighted. Rendered on every
/// view. Links carry the tab in the query string so the 10s meta-refresh keeps
/// the current tab selected.
fn render_tabs(active: &str) -> Markup {
    html! {
        nav class="tabs" {
            @for (slug, label) in TABS {
                a class=(if slug == active { "tab active" } else { "tab" })
                    href=(format!("/?tab={slug}")) { (label) }
            }
        }
    }
}

/// Reusable state badge span for a session row.
fn state_badge(state: SessionState) -> Markup {
    html! {
        span class="badge"
            style={ "background:" (state_color(state)) "22;color:" (state_color(state)) ";border-color:" (state_color(state)) "55;" } {
            (state_display(state))
        }
    }
}

/// Reusable per-row ✕ delete control, wired to [`DELETE_SCRIPT`].
fn delete_button(row: &Row) -> Markup {
    html! {
        button type="button" class="kill"
            data-id=(row.id)
            data-title=(truncate(&row.title, 60))
            title="Delete session"
            aria-label="Delete session" { "✕" }
    }
}

/// Wrap a tab body in the shared dashboard chrome (top bar, tab nav, inline
/// scripts). `auto_refresh` toggles the 10s meta-refresh — passed `false` for
/// the PRs tab so it never re-hits GitHub on a timer or interrupts typing.
fn dashboard_page(active: &str, body: &Markup, auto_refresh: bool) -> Markup {
    let subtitle = if auto_refresh {
        "cross-repo session activity · auto-refreshing"
    } else {
        "cross-repo Jules PR management"
    };
    page_shell(
        "Jules Dashboard",
        &html! {
            header class="topbar" {
                h1 { "Jules Dashboard" }
                span class="muted" { (subtitle) }
            }
            (render_tabs(active))
            (body)
            script { (PreEscaped(DELETE_SCRIPT)) }
            script { (PreEscaped(PR_CONFIRM_SCRIPT)) }
        },
        auto_refresh,
    )
}

/// Render the full populated dashboard: shared chrome, the tab nav, and the
/// selected tab's body. Dispatches on the `tab` query param; the default is the
/// dedicated In Progress landing view.
///
/// The PRs tab needs async, network-fetched data, so the `index` handler renders
/// it directly via [`render_prs_dashboard`]; here it degrades to the no-token
/// notice so the pure, session-only path stays synchronous and testable.
fn render_dashboard(
    sessions: &[Session],
    labels: &BTreeMap<String, String>,
    params: &IndexParams,
) -> Markup {
    let active = selected_tab(params);
    if active == "prs" {
        return render_prs_dashboard(&[], false);
    }
    let body = match active {
        "overview" => render_overview(sessions, labels),
        "sessions" => render_sessions(sessions, labels, params),
        "schedule" => render_schedule(sessions, labels),
        _ => render_in_progress(sessions, labels),
    };
    dashboard_page(active, &body, true)
}

/// Render the PRs tab inside the shared dashboard chrome. Takes already-fetched
/// data so it stays synchronous and unit-testable; the `index` handler supplies
/// the live groups, the demo test supplies sample groups.
fn render_prs_dashboard(groups: &[RepoPrGroup], has_token: bool) -> Markup {
    dashboard_page("prs", &render_prs(groups, has_token), false)
}

/// Dedicated In Progress landing view: every `InProgress` session across all
/// repos, most-recently-updated first, each with a per-row delete control.
fn render_in_progress(sessions: &[Session], labels: &BTreeMap<String, String>) -> Markup {
    let mut rows: Vec<Row> = sessions_in_progress(sessions)
        .into_iter()
        .map(|session| make_row(session, labels))
        .collect();
    // Newest activity first (fall back to creation time when update is absent).
    rows.sort_by(|a, b| {
        b.update_time
            .cmp(&a.update_time)
            .then_with(|| b.create_time.cmp(&a.create_time))
    });
    let count = rows.len();

    html! {
        section class="panel ip-panel" {
            h2 {
                (count) " " (if count == 1 { "session" } else { "sessions" }) " in progress"
            }
            @if rows.is_empty() {
                p class="empty" { "No sessions in progress right now." }
            } @else {
                div class="ip-list" {
                    @for row in &rows {
                        div class="ip-row" {
                            div class="ip-repo mono" title=(row.repo) { (truncate(&row.repo, 28)) }
                            div class="ip-title" {
                                @if let Some(url) = &row.url {
                                    a href=(url) target="_blank" rel="noreferrer" { (truncate(&row.title, 80)) }
                                } @else {
                                    (truncate(&row.title, 80))
                                }
                            }
                            div class="ip-badge" { (state_badge(row.state)) }
                            div class="ip-times mono muted" {
                                "created " (format_time(row.create_time.as_deref()))
                                " · updated " (format_time(row.update_time.as_deref()))
                            }
                            div class="ip-kill" { (delete_button(row)) }
                        }
                    }
                }
            }
        }
    }
}

/// Overview view: the headline cards (running count, active composition,
/// capacity vs the concurrent cap), the per-state bars, and the created-per-hour
/// timeline.
fn render_overview(sessions: &[Session], labels: &BTreeMap<String, String>) -> Markup {
    let running = count_running(sessions);
    let by_state = count_by_state(sessions);
    let by_active = active_composition(sessions);
    let by_hour = count_by_hour(sessions);
    let max_hour = by_hour.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let max_state = by_state.iter().map(|(_, c)| *c).max().unwrap_or(0);

    html! {
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
    }
}

/// Schedule view: the inferred per-repo, per-hour load heatmap (UTC).
fn render_schedule(sessions: &[Session], labels: &BTreeMap<String, String>) -> Markup {
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

    html! {
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
    }
}

/// Sessions view: the full session table with its state/repo/sort filters.
fn render_sessions(
    sessions: &[Session],
    labels: &BTreeMap<String, String>,
    params: &IndexParams,
) -> Markup {
    let rows = build_rows(sessions, labels, params);

    html! {
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
                            td { (state_badge(row.state)) }
                            td class="mono muted" { (format_time(row.create_time.as_deref())) }
                            td class="mono muted" { (format_time(row.update_time.as_deref())) }
                            td class="kill-cell" { (delete_button(row)) }
                        }
                    }
                }
            }
            @if rows.is_empty() {
                p class="muted" { "No sessions match the current filters." }
            }
        }
    }
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

// ── PRs tab ──────────────────────────────────────────────────────

/// Result of one repo's cached PR fetch: `(owner, repo, listing)`.
type PrFetchResult = (String, String, Result<Arc<Vec<Pr>>, crate::github::GithubError>);

/// Max concurrent per-repo PR-list requests during a PRs-tab render.
const PR_FETCH_CONCURRENCY: usize = 6;

/// Max concurrent close/delete requests during a mass-close.
const PR_CLOSE_CONCURRENCY: usize = 5;

/// Max PRs listed per repo panel before collapsing to a "+N more" note.
const PR_LIST_CAP: usize = 50;

/// A single Jules PR as rendered in a repo panel.
struct PrView {
    number: u64,
    title: String,
    age: String,
    html_url: String,
}

/// One repository's group of open Jules PRs, for the PRs tab.
struct RepoPrGroup {
    owner: String,
    repo: String,
    prs: Vec<PrView>,
}

impl RepoPrGroup {
    /// `owner/repo` label used as the panel heading and confirmation target.
    fn label(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// The outcome of closing one PR during a mass-close.
struct CloseOutcome {
    number: u64,
    title: String,
    ok: bool,
    message: String,
}

/// Current wall-clock time as whole seconds since the Unix epoch.
fn current_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Distinct `(owner, repo)` pairs parsed from the source labels. Only labels
/// shaped exactly like `owner/repo` (a single slash, both parts non-empty) are
/// scanned; display-name labels are skipped.
fn distinct_repos(labels: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut set: BTreeSet<(String, String)> = BTreeSet::new();
    for label in labels.values() {
        if let Some((owner, repo)) = label.split_once('/') {
            if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') {
                set.insert((owner.to_string(), repo.to_string()));
            }
        }
    }
    set.into_iter().collect()
}

/// Parse an RFC3339 `YYYY-MM-DDTHH:MM:SS...` timestamp to whole seconds since
/// the Unix epoch, using a pure civil-date calculation (no date dependency).
/// Returns `None` for values that do not match the fixed layout.
fn rfc3339_to_unix(timestamp: &str) -> Option<i64> {
    if timestamp.len() < 19 {
        return None;
    }
    let year: i64 = timestamp.get(0..4)?.parse().ok()?;
    let month: i64 = timestamp.get(5..7)?.parse().ok()?;
    let day: i64 = timestamp.get(8..10)?.parse().ok()?;
    let hour: i64 = timestamp.get(11..13)?.parse().ok()?;
    let minute: i64 = timestamp.get(14..16)?.parse().ok()?;
    let second: i64 = timestamp.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    // Days-from-civil (Howard Hinnant), epoch shifted to 0000-03-01.
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let year_of_era = y - era * 400;
    let month_shift = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shift + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Compact age string for a PR created at `created_at`, relative to `now_unix`.
/// Sub-day ages render as `Nh`, longer ages as `Nd`. Unparseable timestamps
/// render as `?`.
fn pr_age(created_at: &str, now_unix: u64) -> String {
    let Some(created) = rfc3339_to_unix(created_at) else {
        return "?".to_string();
    };
    let now = i64::try_from(now_unix).unwrap_or(i64::MAX);
    let diff = now - created;
    if diff <= 0 {
        return "0h".to_string();
    }
    let hours = diff / 3_600;
    if hours < 24 {
        format!("{hours}h")
    } else {
        format!("{}d", hours / 24)
    }
}

/// Convert a fetched [`Pr`] into a renderable [`PrView`].
fn pr_view(pr: &Pr, now_unix: u64) -> PrView {
    PrView {
        number: pr.number,
        title: pr.title.clone(),
        age: pr_age(&pr.created_at, now_unix),
        html_url: pr.html_url.clone(),
    }
}

/// Fetch open pulls for each repo in parallel (bounded concurrency), filter to
/// Jules PRs, and group by repo. Repos that error or have zero Jules PRs are
/// omitted. Groups are sorted by descending Jules-PR count, then repo label.
async fn fetch_pr_groups(
    github: Arc<GithubClient>,
    cache: Arc<Mutex<DashboardCache>>,
    matcher: &JulesMatcher,
    repos: Vec<(String, String)>,
    now_unix: u64,
) -> Vec<RepoPrGroup> {
    let mut groups: Vec<RepoPrGroup> = Vec::new();

    for chunk in repos.chunks(PR_FETCH_CONCURRENCY) {
        let mut set: JoinSet<PrFetchResult> = JoinSet::new();
        for (owner, repo) in chunk {
            let github = Arc::clone(&github);
            let cache = Arc::clone(&cache);
            let owner = owner.clone();
            let repo = repo.clone();
            set.spawn(async move {
                let result = cached_open_pulls(&github, &cache, &owner, &repo).await;
                (owner, repo, result)
            });
        }
        while let Some(joined) = set.join_next().await {
            let Ok((owner, repo, Ok(prs))) = joined else {
                continue;
            };
            let mut views: Vec<PrView> = prs
                .iter()
                .filter(|pr| matcher.is_jules(pr))
                .map(|pr| pr_view(pr, now_unix))
                .collect();
            if views.is_empty() {
                continue;
            }
            // Newest PR number first within a repo.
            views.sort_by(|a, b| b.number.cmp(&a.number));
            groups.push(RepoPrGroup { owner, repo, prs: views });
        }
    }

    groups.sort_by(|a, b| {
        b.prs
            .len()
            .cmp(&a.prs.len())
            .then_with(|| a.label().cmp(&b.label()))
    });
    groups
}

/// Close each target PR (bounded concurrency), optionally deleting its head
/// branch, and collect a per-PR outcome. Outcomes are returned sorted by PR
/// number ascending.
async fn close_prs(
    github: Arc<GithubClient>,
    owner: &str,
    repo: &str,
    targets: Vec<Pr>,
    delete_branches: bool,
) -> Vec<CloseOutcome> {
    let mut outcomes: Vec<CloseOutcome> = Vec::new();

    for chunk in targets.chunks(PR_CLOSE_CONCURRENCY) {
        let mut set: JoinSet<CloseOutcome> = JoinSet::new();
        for pr in chunk {
            let github = Arc::clone(&github);
            let owner = owner.to_string();
            let repo = repo.to_string();
            let number = pr.number;
            let title = pr.title.clone();
            let head_ref = pr.head_ref.clone();
            set.spawn(async move {
                match github.close_pull(&owner, &repo, number).await {
                    Ok(()) => {
                        if delete_branches {
                            if let Err(error) =
                                github.delete_branch(&owner, &repo, &head_ref).await
                            {
                                return CloseOutcome {
                                    number,
                                    title,
                                    ok: true,
                                    message: format!("closed · branch delete failed: {error}"),
                                };
                            }
                        }
                        CloseOutcome {
                            number,
                            title,
                            ok: true,
                            message: "closed".to_string(),
                        }
                    }
                    Err(error) => CloseOutcome {
                        number,
                        title,
                        ok: false,
                        message: error.to_string(),
                    },
                }
            });
        }
        while let Some(joined) = set.join_next().await {
            if let Ok(outcome) = joined {
                outcomes.push(outcome);
            }
        }
    }

    outcomes.sort_by_key(|outcome| outcome.number);
    outcomes
}

/// Pluralized "PR"/"PRs" label for a count.
fn pr_word(count: usize) -> &'static str {
    if count == 1 { "PR" } else { "PRs" }
}

/// Render the PRs tab body from already-fetched data.
fn render_prs(groups: &[RepoPrGroup], has_token: bool) -> Markup {
    if !has_token {
        return html! {
            section class="panel pr-notice" {
                h2 { "Jules PR management" }
                p {
                    "Set " code { "GITHUB_TOKEN" } " in the server environment to enable "
                    "Jules PR management."
                }
                p class="muted" {
                    "Once a token is present this tab lists every open Jules-authored PR per "
                    "repository and lets you close them all with one guarded click. Identification "
                    "is configurable via " code { "JULES_PR_AUTHORS" } " (comma-separated author "
                    "logins) and " code { "JULES_PR_MARKER" } " (a PR-body substring)."
                }
            }
        };
    }

    let total: usize = groups.iter().map(|group| group.prs.len()).sum();

    html! {
        @if groups.is_empty() {
            section class="panel" {
                p class="empty" {
                    "No open Jules PRs found across your repositories. Nice and clean."
                }
            }
        } @else {
            section class="panel pr-headline" {
                h2 {
                    (total) " open Jules " (pr_word(total)) " across "
                    (groups.len()) " " (if groups.len() == 1 { "repo" } else { "repos" })
                }
                p class="muted" {
                    "Close every Jules-authored PR in a repo with one guarded click. The server "
                    "re-lists and re-classifies each PR before closing, so a non-Jules PR is never "
                    "touched — even if this page is stale."
                }
            }
            @for group in groups {
                (render_pr_group(group))
            }
        }
    }
}

/// Comma-separated PR numbers shown for a repo, carried in the form's `shown`
/// hidden field so the server can reconcile shown-vs-matched after re-fetch.
fn shown_numbers(group: &RepoPrGroup) -> String {
    group
        .prs
        .iter()
        .map(|pr| pr.number.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Render one repository's panel: a count badge, the PR list (capped), and the
/// guarded type-to-confirm close-all form.
fn render_pr_group(group: &RepoPrGroup) -> Markup {
    let count = group.prs.len();
    let target = group.label();
    let hidden = count.saturating_sub(PR_LIST_CAP);

    html! {
        section class="panel pr-panel" {
            div class="pr-panel-head" {
                span class="pr-repo mono" { (target) }
                span class="pr-badge" { (count) " Jules " (pr_word(count)) }
            }
            div class="pr-list" {
                @for pr in group.prs.iter().take(PR_LIST_CAP) {
                    div class="pr-row" {
                        span class="pr-num mono muted" { "#" (pr.number) }
                        a class="pr-title" href=(pr.html_url) target="_blank" rel="noreferrer" {
                            (truncate(&pr.title, 90))
                        }
                        span class="pr-age mono muted" { (pr.age) }
                    }
                }
            }
            @if hidden > 0 {
                p class="muted pr-more" { "+" (hidden) " more not shown" }
            }
            details class="pr-confirm" {
                summary class="pr-toggle" {
                    "Close all " (count) " Jules " (pr_word(count)) " in this repo"
                }
                form class="pr-form" method="post" action="/prs/close-all" {
                    input type="hidden" name="owner" value=(group.owner);
                    input type="hidden" name="repo" value=(group.repo);
                    input type="hidden" name="shown" value=(shown_numbers(group));
                    p class="pr-warn" {
                        "This permanently closes " (count) " open pull " (pr_word(count))
                        " in " strong { (target) } ". This cannot be undone from here."
                    }
                    label class="pr-confirm-label" {
                        "Type " code { (target) } " to confirm:"
                        input class="pr-confirm-input" type="text" name="confirm"
                            autocomplete="off" spellcheck="false"
                            data-expect=(target) placeholder=(target);
                    }
                    label class="pr-branch-label" {
                        input type="checkbox" name="delete_branches" value="on";
                        " Also delete the head branches"
                    }
                    button type="submit" class="pr-danger-btn" disabled {
                        "Close " (count) " " (pr_word(count))
                    }
                }
            }
        }
    }
}

/// Collapse whitespace and truncate a (possibly multi-line JSON) API error body
/// to a compact single line for the per-PR results row.
fn short_msg(message: &str) -> String {
    let collapsed = message.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&collapsed, 160)
}

/// Results page after a mass-close. Always renders a clear outcome: headline
/// counts (shown / matched / closed / failed), an explicit reason banner when
/// zero PRs were closed, any PRs that were shown but no longer match, and a
/// per-PR line with trimmed API errors. No auto-refresh.
fn render_close_result(summary: &CloseSummary, matcher: &JulesMatcher) -> Markup {
    let zero_reason = summary.zero_reason(matcher);

    page_shell(
        "Jules Dashboard — close results",
        &html! {
            header class="topbar" {
                h1 { "Jules Dashboard" }
            }
            section class="panel" {
                h2 { "Close results for " (summary.target) }
                p class="pr-result-summary" {
                    span class="pr-ok" { (summary.closed) " closed" }
                    " · "
                    span class=(if summary.failed > 0 { "pr-fail" } else { "muted" }) {
                        (summary.failed) " failed"
                    }
                    " · "
                    span class="muted" {
                        (summary.matched) " matched · " (summary.shown) " shown"
                    }
                }
                @if let Some(reason) = &zero_reason {
                    div class="pr-reason" { (reason) }
                }
                @if !summary.unmatched.is_empty() {
                    p class="pr-warn-note" {
                        (summary.unmatched.len()) " " (pr_word(summary.unmatched.len()))
                        " shown on the page no longer match the Jules filter on re-fetch and "
                        "were NOT closed: "
                        span class="mono" {
                            @for (i, number) in summary.unmatched.iter().enumerate() {
                                @if i > 0 { ", " }
                                "#" (number)
                            }
                        }
                    }
                }
                @if summary.delete_branches {
                    p class="muted" { "Head branch deletion was requested for each closed PR." }
                }
                @if summary.outcomes.is_empty() {
                    p class="muted" {
                        "No Jules PRs were closed. See the reason above for why."
                    }
                } @else {
                    div class="pr-list" {
                        @for outcome in &summary.outcomes {
                            div class="pr-row" {
                                span class="pr-num mono muted" { "#" (outcome.number) }
                                span class="pr-title" { (truncate(&outcome.title, 80)) }
                                span class=(if outcome.ok { "pr-ok" } else { "pr-fail" }) {
                                    (if outcome.ok { "✓ " } else { "✗ " }) (short_msg(&outcome.message))
                                }
                            }
                        }
                    }
                }
                p class="pr-back" { a href="/?tab=prs" { "← Back to PRs" } }
            }
        },
        false,
    )
}

/// Error results page for a rejected or failed mass-close (no auto-refresh).
fn render_close_error(message: &str) -> Markup {
    page_shell(
        "Jules Dashboard — close error",
        &html! {
            header class="topbar" {
                h1 { "Jules Dashboard" }
            }
            div class="error-card" {
                h2 { "Nothing was closed" }
                p { (message) }
                p class="pr-back" { a href="/?tab=prs" { "← Back to PRs" } }
            }
        },
        false,
    )
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
.topbar { display: flex; align-items: baseline; gap: 16px; margin-bottom: 20px; }
.topbar h1 { font-size: 22px; margin: 0; letter-spacing: 0.3px; }
.tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 24px;
    border-bottom: 1px solid #1e293b;
    padding-bottom: 0;
}
.tab {
    display: inline-block;
    padding: 9px 16px;
    font-size: 13px;
    font-weight: 600;
    color: #94a3b8;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 10px 10px 0 0;
    margin-bottom: -1px;
    text-decoration: none;
    transition: color 0.15s, background 0.15s, border-color 0.15s;
}
.tab:hover { color: #e2e8f0; background: #111a2e; text-decoration: none; }
.tab.active {
    color: #22c55e;
    background: #111a2e;
    border-color: #1e293b;
    border-bottom: 1px solid #111a2e;
}
.ip-panel h2 { font-size: 18px; color: #e2e8f0; margin-bottom: 18px; }
.empty {
    color: #94a3b8;
    font-size: 14px;
    padding: 28px 4px;
    text-align: center;
}
.ip-list { display: flex; flex-direction: column; gap: 8px; }
.ip-row {
    display: grid;
    grid-template-columns: 190px 1fr auto auto 34px;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    background: #0f1a30;
    border: 1px solid #1e293b;
    border-radius: 10px;
}
.ip-row:hover { border-color: #22c55e55; }
.ip-repo { color: #cbd5e1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ip-title { font-size: 14px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ip-times { font-size: 11px; white-space: nowrap; }
.ip-badge, .ip-kill { display: flex; align-items: center; justify-content: center; }
@media (max-width: 720px) {
    .ip-row { grid-template-columns: 1fr auto 34px; }
    .ip-repo { grid-column: 1 / 2; }
    .ip-title { grid-column: 1 / -1; order: 3; }
    .ip-times { grid-column: 1 / -1; order: 4; }
}
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
.pr-notice h2 { color: #e2e8f0; }
.pr-notice code, .pr-panel code, .pr-form code {
    background: #0b1120;
    border: 1px solid #1e293b;
    border-radius: 5px;
    padding: 1px 6px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    color: #7dd3fc;
}
.pr-headline h2 { color: #e2e8f0; font-size: 18px; }
.pr-panel { padding: 16px 20px; }
.pr-panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
}
.pr-repo { font-size: 14px; color: #e2e8f0; }
.pr-badge {
    background: #3b82f622;
    color: #93c5fd;
    border: 1px solid #3b82f655;
    border-radius: 999px;
    padding: 3px 11px;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
}
.pr-list { display: flex; flex-direction: column; gap: 4px; }
.pr-row {
    display: grid;
    grid-template-columns: 56px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    border-bottom: 1px solid #16203a;
}
.pr-num { text-align: right; }
.pr-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
.pr-age { white-space: nowrap; }
.pr-more { margin: 8px 0 0; font-size: 12px; }
.pr-confirm { margin-top: 14px; }
.pr-toggle {
    cursor: pointer;
    display: inline-block;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 600;
    color: #fca5a5;
    background: #2a1216;
    border: 1px solid #7f1d1d;
    list-style: none;
}
.pr-toggle::-webkit-details-marker { display: none; }
.pr-toggle:hover { background: #3a1a1f; }
.pr-form {
    margin-top: 12px;
    padding: 16px;
    border: 1px solid #7f1d1d;
    border-radius: 10px;
    background: #1a0e11;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 560px;
}
.pr-warn { margin: 0; font-size: 13px; color: #fecaca; line-height: 1.5; }
.pr-warn strong { color: #fff; }
.pr-confirm-label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: #e2e8f0; }
.pr-confirm-input {
    background: #0b1120;
    border: 1px solid #7f1d1d;
    border-radius: 8px;
    padding: 9px 11px;
    color: #e2e8f0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 13px;
}
.pr-confirm-input:focus { outline: none; border-color: #ef4444; }
.pr-branch-label { display: flex; align-items: center; gap: 8px; font-size: 13px; color: #cbd5e1; }
.pr-danger-btn {
    align-self: flex-start;
    padding: 9px 18px;
    border-radius: 8px;
    border: 1px solid #ef4444;
    background: #ef4444;
    color: #fff;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    font-variant-numeric: tabular-nums;
}
.pr-danger-btn:hover:enabled { background: #dc2626; }
.pr-danger-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.pr-result-summary { font-size: 15px; font-weight: 600; font-variant-numeric: tabular-nums; }
.pr-ok { color: #22c55e; }
.pr-fail { color: #ef4444; }
.pr-reason {
    margin: 14px 0;
    padding: 12px 14px;
    font-size: 13px;
    line-height: 1.5;
    color: #fecaca;
    background: #2a1216;
    border: 1px solid #7f1d1d;
    border-radius: 8px;
}
.pr-warn-note {
    margin: 12px 0;
    padding: 10px 14px;
    font-size: 13px;
    line-height: 1.5;
    color: #fbbf24;
    background: #2a2410;
    border: 1px solid #4d3c12;
    border-radius: 8px;
}
.pr-back { margin-top: 18px; }
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

/// Inline script for the type-to-confirm mass-close form. The destructive
/// submit button stays disabled until the text input's value exactly matches
/// the panel's `owner/repo` (carried in `data-expect`). Server-side validation
/// re-checks this, so the script is a UX guard, not the safety boundary.
const PR_CONFIRM_SCRIPT: &str = r"
(function () {
    document.addEventListener('input', function (e) {
        var input = e.target.closest('.pr-confirm-input');
        if (!input) { return; }
        var form = input.closest('form');
        var btn = form ? form.querySelector('.pr-danger-btn') : null;
        if (!btn) { return; }
        btn.disabled = input.value.trim() !== input.dataset.expect;
    });
})();
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
    fn sessions_in_progress_filters_to_in_progress_only() {
        let sessions = vec![
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::Queued), None),
            session(Some(SessionState::InProgress), None),
            session(Some(SessionState::Completed), None),
            session(Some(SessionState::Paused), None),
            session(None, None),
        ];
        let running = sessions_in_progress(&sessions);
        assert_eq!(running.len(), 2);
        assert!(running
            .iter()
            .all(|s| session_state(s) == SessionState::InProgress));
    }

    #[test]
    fn in_progress_tab_is_default_and_lists_running_sessions() {
        let mut running = session(Some(SessionState::InProgress), Some("2026-07-15T09:00:00Z"));
        running.title = Some("busy task".to_string());
        let queued = session(Some(SessionState::Queued), Some("2026-07-15T09:00:00Z"));

        let labels = BTreeMap::new();
        // No tab param → defaults to the In Progress landing view.
        let params = IndexParams::default();
        let html =
            render_dashboard(&[running, queued], &labels, &params).into_string();

        assert!(html.contains("in progress"));
        assert!(html.contains("busy task"));
        // Tab nav is present with In Progress marked active.
        assert!(html.contains(r#"class="tab active""#));
        assert!(html.contains("/?tab=overview"));
    }

    #[test]
    fn empty_in_progress_tab_shows_friendly_message() {
        let sessions = vec![
            session(Some(SessionState::Completed), None),
            session(Some(SessionState::Queued), None),
        ];
        let labels = BTreeMap::new();
        let params = IndexParams::default();
        let html = render_dashboard(&sessions, &labels, &params).into_string();
        assert!(html.contains("No sessions in progress right now."));
    }

    #[test]
    fn session_row_renders_delete_control() {
        let mut s = session(Some(SessionState::Queued), Some("2026-07-15T09:00:00Z"));
        s.id = Some("s-42".to_string());
        s.title = Some("zombie task".to_string());

        let labels = BTreeMap::new();
        // The full table (with a Queued row) lives on the Sessions tab.
        let params = IndexParams {
            tab: Some("sessions".to_string()),
            ..IndexParams::default()
        };
        let html = render_dashboard(std::slice::from_ref(&s), &labels, &params).into_string();

        // The ✕ button carries the bare session id used to build the route.
        assert!(html.contains(r#"class="kill""#));
        assert!(html.contains(r#"data-id="s-42""#));
        assert!(html.contains(r#"aria-label="Delete session""#));
        // The inline script POSTs to /sessions/{id}/delete for that id.
        assert!(html.contains("'/sessions/' + encodeURIComponent(id) + '/delete'"));
    }

    #[test]
    fn schedule_tab_renders_heatmap() {
        let labels = BTreeMap::new();
        let sessions = vec![
            session_repo("acme/api", "2026-07-10T09:00:00Z"),
            session_repo("acme/api", "2026-07-11T09:30:00Z"),
        ];
        let params = IndexParams {
            tab: Some("schedule".to_string()),
            ..IndexParams::default()
        };
        let html = render_dashboard(&sessions, &labels, &params).into_string();
        assert!(html.contains("Inferred schedule"));
    }

    #[test]
    fn overview_tab_renders_headline_and_capacity() {
        let labels = BTreeMap::new();
        let sessions = vec![session(Some(SessionState::InProgress), None)];
        let params = IndexParams {
            tab: Some("overview".to_string()),
            ..IndexParams::default()
        };
        let html = render_dashboard(&sessions, &labels, &params).into_string();
        assert!(html.contains("Sessions by state"));
        assert!(html.contains("Ultra plan limit"));
    }

    #[test]
    fn rfc3339_to_unix_matches_known_epochs() {
        // 2026-07-16T00:00:00Z is 1_784_160_000 seconds since the epoch.
        assert_eq!(rfc3339_to_unix("2026-07-16T00:00:00Z"), Some(1_784_160_000));
        assert_eq!(rfc3339_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_to_unix("1970-01-02T00:00:01Z"), Some(86_401));
        // Malformed / out-of-range inputs are rejected.
        assert_eq!(rfc3339_to_unix("nope"), None);
        assert_eq!(rfc3339_to_unix("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn pr_age_uses_hours_then_days() {
        let base = 1_784_160_000_u64; // 2026-07-16T00:00:00Z
        // Same instant → 0h.
        assert_eq!(pr_age("2026-07-16T00:00:00Z", base), "0h");
        // 5 hours old.
        assert_eq!(pr_age(&unix_to_rfc3339(base - 5 * 3_600), base), "5h");
        // 3 days old.
        assert_eq!(pr_age(&unix_to_rfc3339(base - 3 * 86_400), base), "3d");
        // Future timestamps clamp to 0h, unparseable to ?.
        assert_eq!(pr_age("2030-01-01T00:00:00Z", base), "0h");
        assert_eq!(pr_age("garbage", base), "?");
    }

    /// Helper: round-trip a unix time back to RFC3339 for age tests, reusing the
    /// same civil-date math the parser inverts.
    fn unix_to_rfc3339(secs: u64) -> String {
        let secs = i64::try_from(secs).unwrap();
        let days = secs.div_euclid(86_400);
        let tod = secs.rem_euclid(86_400);
        let (hour, minute, second) = (tod / 3_600, (tod % 3_600) / 60, tod % 60);
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if month <= 2 { year + 1 } else { year };
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
    }

    #[test]
    fn distinct_repos_parses_owner_repo_labels_only() {
        let mut labels = BTreeMap::new();
        labels.insert("sources/a".to_string(), "acme/api".to_string());
        labels.insert("sources/b".to_string(), "acme/api".to_string()); // dup
        labels.insert("sources/c".to_string(), "octo/cat".to_string());
        labels.insert("sources/d".to_string(), "just-a-display-name".to_string());
        labels.insert("sources/e".to_string(), "too/many/slashes".to_string());
        let repos = distinct_repos(&labels);
        assert_eq!(
            repos,
            vec![
                ("acme".to_string(), "api".to_string()),
                ("octo".to_string(), "cat".to_string()),
            ]
        );
    }

    #[test]
    fn prs_tab_without_token_shows_setup_notice() {
        let html = render_prs(&[], false).into_string();
        assert!(html.contains("GITHUB_TOKEN"));
        assert!(html.contains("JULES_PR_AUTHORS"));
    }

    #[test]
    fn prs_tab_with_token_empty_shows_clean_state() {
        let html = render_prs(&[], true).into_string();
        assert!(html.contains("No open Jules PRs"));
    }

    #[test]
    fn prs_tab_renders_groups_and_confirm_form() {
        let groups = sample_pr_groups();
        let html = render_prs_dashboard(&groups, true).into_string();
        // Tab nav shows PRs active, and no auto-refresh meta on this tab.
        assert!(html.contains("/?tab=prs"));
        assert!(!html.contains(r#"http-equiv="refresh""#));
        // Headline + a repo panel with a count badge.
        assert!(html.contains("open Jules"));
        assert!(html.contains("acme/api"));
        assert!(html.contains("Jules PRs"));
        // The guarded close form: hidden fields, type-to-confirm input, disabled
        // destructive button, and the branch-delete checkbox.
        assert!(html.contains(r#"action="/prs/close-all""#));
        assert!(html.contains(r#"name="confirm""#));
        assert!(html.contains(r#"data-expect="acme/api""#));
        assert!(html.contains(r#"name="delete_branches""#));
        assert!(html.contains("<button type=\"submit\" class=\"pr-danger-btn\" disabled"));
    }

    /// A matcher with fixed config, independent of the environment, for the
    /// close-result render tests.
    fn test_matcher() -> JulesMatcher {
        JulesMatcher::from_env()
    }

    #[test]
    fn close_result_page_summarizes_outcomes() {
        let outcomes = vec![
            CloseOutcome {
                number: 10,
                title: "nova: a".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 11,
                title: "nova: b".to_string(),
                ok: false,
                message: "github API error 403: nope".to_string(),
            },
        ];
        let summary = build_close_summary("acme/api", "acme", "api", &[10, 11], &[10, 11], outcomes);
        let html = render_close_result(&summary, &test_matcher()).into_string();
        assert!(html.contains("1 closed"));
        assert!(html.contains("1 failed"));
        assert!(html.contains("/?tab=prs"));
        assert!(!html.contains(r#"http-equiv="refresh""#));
        // One PR closed, so there is no zero-reason banner.
        assert!(!html.contains("nothing was closed"));
    }

    #[test]
    fn parse_pr_numbers_skips_blanks_and_junk() {
        assert_eq!(parse_pr_numbers(None), Vec::<u64>::new());
        assert_eq!(parse_pr_numbers(Some("")), Vec::<u64>::new());
        assert_eq!(parse_pr_numbers(Some("10, 11 ,,x,12")), vec![10, 11, 12]);
    }

    #[test]
    fn build_close_summary_confirm_mismatch_closes_nothing() {
        // Typed confirmation does not match owner/repo.
        let summary = build_close_summary("wrong", "acme", "api", &[10, 11], &[], Vec::new());
        assert!(!summary.confirm_ok);
        assert_eq!(summary.matched, 0);
        assert_eq!(summary.closed, 0);
        let reason = summary.zero_reason(&test_matcher()).expect("reason");
        assert!(reason.contains("did not match"));
        assert!(reason.contains("acme/api"));
    }

    #[test]
    fn build_close_summary_zero_matched_reports_filter() {
        // Confirm matches, but the re-fetch reclassified nothing as Jules; the
        // two shown PRs are reported as no longer matching.
        let summary = build_close_summary("acme/api", "acme", "api", &[10, 11], &[], Vec::new());
        assert!(summary.confirm_ok);
        assert_eq!(summary.matched, 0);
        assert_eq!(summary.closed, 0);
        assert_eq!(summary.unmatched, vec![10, 11]);
        let reason = summary.zero_reason(&test_matcher()).expect("reason");
        assert!(reason.contains("No open PRs matched the Jules filter"));
        assert!(reason.contains("authors="));
        assert!(reason.contains("marker="));
    }

    #[test]
    fn build_close_summary_partial_failure_counts_and_reconciles() {
        // Shown [10,11,12]; server reclassified [10,11] (12 no longer matches);
        // 10 closed, 11 failed.
        let outcomes = vec![
            CloseOutcome {
                number: 10,
                title: "a".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 11,
                title: "b".to_string(),
                ok: false,
                message: "github API error 403: forbidden".to_string(),
            },
        ];
        let summary =
            build_close_summary("acme/api", "acme", "api", &[10, 11, 12], &[10, 11], outcomes);
        assert_eq!(summary.shown, 3);
        assert_eq!(summary.matched, 2);
        assert_eq!(summary.closed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.unmatched, vec![12]);
        // At least one closed → no zero-reason banner.
        assert!(summary.zero_reason(&test_matcher()).is_none());
    }

    #[test]
    fn build_close_summary_all_success_has_no_reason() {
        let outcomes = vec![
            CloseOutcome {
                number: 10,
                title: "a".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 11,
                title: "b".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
        ];
        let summary = build_close_summary("acme/api", "acme", "api", &[10, 11], &[10, 11], outcomes);
        assert_eq!(summary.closed, 2);
        assert_eq!(summary.failed, 0);
        assert!(summary.unmatched.is_empty());
        assert!(summary.zero_reason(&test_matcher()).is_none());
    }

    #[test]
    fn build_close_summary_all_failed_reports_all_failed_reason() {
        let outcomes = vec![CloseOutcome {
            number: 10,
            title: "a".to_string(),
            ok: false,
            message: "github API error 403: forbidden".to_string(),
        }];
        let summary = build_close_summary("acme/api", "acme", "api", &[10], &[10], outcomes);
        assert_eq!(summary.closed, 0);
        assert_eq!(summary.failed, 1);
        let reason = summary.zero_reason(&test_matcher()).expect("reason");
        assert!(reason.contains("failed to close"));
    }

    #[test]
    fn close_result_renders_unmatched_note_and_reason() {
        // Confirm ok, nothing matched, one shown PR reported as unmatched.
        let summary = build_close_summary("acme/api", "acme", "api", &[10], &[], Vec::new());
        let html = render_close_result(&summary, &test_matcher()).into_string();
        assert!(html.contains("No open PRs matched the Jules filter"));
        assert!(html.contains("were NOT closed"));
        assert!(html.contains("#10"));
        assert!(!html.contains(r#"http-equiv="refresh""#));
    }

    #[test]
    fn is_fresh_respects_ttl() {
        let ttl = Duration::from_secs(8);
        let start = Instant::now();
        // Just fetched → fresh.
        assert!(is_fresh(start, start, ttl));
        // Within TTL → fresh.
        assert!(is_fresh(start, start + Duration::from_secs(7), ttl));
        // At/after TTL → stale.
        assert!(!is_fresh(start, start + Duration::from_secs(8), ttl));
        assert!(!is_fresh(start, start + Duration::from_secs(20), ttl));
    }

    #[test]
    fn shown_numbers_joins_pr_numbers() {
        let group = RepoPrGroup {
            owner: "acme".to_string(),
            repo: "api".to_string(),
            prs: vec![
                PrView {
                    number: 12,
                    title: "a".to_string(),
                    age: "1h".to_string(),
                    html_url: String::new(),
                },
                PrView {
                    number: 10,
                    title: "b".to_string(),
                    age: "2h".to_string(),
                    html_url: String::new(),
                },
            ],
        };
        assert_eq!(shown_numbers(&group), "12,10");
    }

    /// Sample PR groups for the demo screenshot and the render tests.
    fn sample_pr_groups() -> Vec<RepoPrGroup> {
        let view = |number: u64, title: &str, age: &str| PrView {
            number,
            title: title.to_string(),
            age: age.to_string(),
            html_url: format!("https://github.com/acme/api/pull/{number}"),
        };
        vec![
            RepoPrGroup {
                owner: "acme".to_string(),
                repo: "api".to_string(),
                prs: vec![
                    view(4321, "nova: bump serde and tidy imports", "2h"),
                    view(4319, "jules: add retry backoff to the poller", "6h"),
                    view(4302, "feature/rename config keys for clarity", "1d"),
                    view(4288, "nova/refactor the session cache layer", "3d"),
                ],
            },
            RepoPrGroup {
                owner: "acme".to_string(),
                repo: "web".to_string(),
                prs: vec![
                    view(210, "jules-tidy the dashboard styles", "4h"),
                    view(205, "nova: extract the header component", "2d"),
                ],
            },
            RepoPrGroup {
                owner: "octo".to_string(),
                repo: "cat".to_string(),
                prs: vec![view(77, "jules: document the deploy flow", "5d")],
            },
        ]
    }

    /// Renders the close-all RESULTS page from synthetic data and writes it to
    /// disk for manual/visual inspection of the visible-error-reporting fix.
    /// Ignored by default; run with:
    /// `cargo test --features web -- --ignored render_demo_close_results --nocapture`
    #[test]
    #[ignore = "writes an HTML file for manual screenshotting"]
    fn render_demo_close_results() {
        // 5 matched server-side, 4 closed OK, 1 failed with an HTTP-403 reason.
        let outcomes = vec![
            CloseOutcome {
                number: 4288,
                title: "nova/refactor the session cache layer".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 4302,
                title: "feature/rename config keys for clarity".to_string(),
                ok: true,
                message: "closed · branch delete failed: github API error 422: reference does not exist".to_string(),
            },
            CloseOutcome {
                number: 4319,
                title: "jules: add retry backoff to the poller".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 4321,
                title: "nova: bump serde and tidy imports".to_string(),
                ok: true,
                message: "closed".to_string(),
            },
            CloseOutcome {
                number: 4330,
                title: "jules: regenerate the OpenAPI client".to_string(),
                ok: false,
                message: "github API error 403: {\"message\":\"Resource not accessible by personal access token\",\"documentation_url\":\"https://docs.github.com/rest\"}".to_string(),
            },
        ];
        // Shown on the page: the 5 matched plus 2 that no longer classify.
        let shown = [4288, 4302, 4319, 4321, 4330, 4111, 3999];
        let matched = [4288, 4302, 4319, 4321, 4330];
        let mut summary =
            build_close_summary("acme/api", "acme", "api", &shown, &matched, outcomes);
        summary.delete_branches = true;

        let html = render_close_result(&summary, &JulesMatcher::from_env()).into_string();
        let out = std::env::var("DEMO_OUT").unwrap_or_else(|_| "close-results.html".to_string());
        std::fs::write(&out, html).expect("write close-results html");
        eprintln!(
            "wrote {out} · matched={} closed={} failed={} shown_but_unmatched={}",
            summary.matched,
            summary.closed,
            summary.failed,
            summary.unmatched.len()
        );
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

        // Render the tab named by DEMO_TAB (default: the In Progress landing view).
        // The PRs tab needs network-fetched data in the live server, so here it
        // renders from sample groups (with the confirm form visible) instead.
        let demo_tab = std::env::var("DEMO_TAB").ok();
        let html = if demo_tab.as_deref() == Some("prs") {
            render_prs_dashboard(&sample_pr_groups(), true).into_string()
        } else {
            let params = IndexParams {
                tab: demo_tab,
                ..IndexParams::default()
            };
            render_dashboard(&sessions, &labels, &params).into_string()
        };

        let out = std::env::var("DEMO_OUT").unwrap_or_else(|_| "dashboard.html".to_string());
        std::fs::write(&out, html).expect("write dashboard html");
        eprintln!(
            "wrote {out} · {} sessions · running={}",
            sessions.len(),
            count_running(&sessions)
        );
    }
}
