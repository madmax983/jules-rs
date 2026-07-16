//! Minimal GitHub REST client for cross-repo Jules PR management.
//!
//! This module powers the "PRs" tab of the `jules-web` dashboard. It lists a
//! repository's open pull requests, closes them, and (optionally) deletes their
//! head branches. It is deliberately tiny — a struct holding a
//! [`reqwest::Client`], a [`thiserror`] error type, and a handful of async
//! methods — mirroring the conventions of [`crate::client`] rather than pulling
//! in a heavyweight GitHub SDK.
//!
//! Identifying which PRs were authored by Jules is handled by the pure,
//! unit-tested [`JulesMatcher`]: a PR matches when its author login is in the
//! configured set **or** its body carries the Jules task marker. See the
//! `jules-pr-identification` team note for the ground-truth reasoning.

use std::collections::HashSet;

use reqwest::Client;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

/// Default GitHub REST API base URL.
const DEFAULT_API_URL: &str = "https://api.github.com";

/// `User-Agent` sent on every request — GitHub rejects requests without one.
const USER_AGENT: &str = "jules-rs-web";

/// GitHub REST API version pin.
const API_VERSION: &str = "2022-11-28";

/// Page size for the pulls listing endpoint (GitHub's maximum).
const PER_PAGE: usize = 100;

/// Default comma-separated author logins treated as Jules bots.
const DEFAULT_JULES_AUTHOR: &str = "google-labs-jules[bot]";

/// Default body substring that marks a PR as authored by Jules.
const DEFAULT_JULES_MARKER: &str = "created automatically by Jules for task";

/// Secondary, always-checked body marker: the Jules task permalink.
const JULES_TASK_LINK: &str = "jules.google.com/task/";

/// Errors that can occur while talking to the GitHub REST API.
#[derive(Debug, Error)]
pub enum GithubError {
    /// Network transport error while sending or receiving a request.
    #[error("github request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// GitHub returned a non-success HTTP status.
    #[error("github API error {status}: {message}")]
    Api {
        /// HTTP status code from the response.
        status: u16,
        /// Response body (best-effort), useful for diagnostics.
        message: String,
    },
}

/// Async HTTP client for the subset of the GitHub REST API the dashboard needs.
pub struct GithubClient {
    http: Client,
    token: String,
    base_url: String,
}

/// Nested `user` object in a PR payload.
#[derive(Debug, Clone, Default, Deserialize)]
struct RawUser {
    #[serde(default)]
    login: String,
}

/// Nested `head` object in a PR payload.
#[derive(Debug, Clone, Default, Deserialize)]
struct RawHead {
    #[serde(rename = "ref", default)]
    ref_: String,
    #[serde(default)]
    sha: String,
}

/// Raw PR shape as returned by the GitHub pulls endpoint. Flattened into the
/// public [`Pr`] via [`From`].
#[derive(Debug, Clone, Deserialize)]
struct RawPr {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    user: RawUser,
    #[serde(default)]
    head: RawHead,
}

/// A pull request, with the fields the dashboard cares about lifted to the top
/// level. Deserializes directly from a GitHub PR JSON object.
#[derive(Debug, Clone)]
pub struct Pr {
    /// PR number within the repository.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// PR body (Markdown), when present.
    pub body: Option<String>,
    /// Web URL of the PR.
    pub html_url: String,
    /// RFC3339 creation timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
    pub created_at: String,
    /// Login of the PR author (`user.login`).
    pub author_login: String,
    /// Head branch name (`head.ref`).
    pub head_ref: String,
    /// Head commit SHA (`head.sha`), used to query check runs.
    pub head_sha: String,
}

impl From<RawPr> for Pr {
    fn from(raw: RawPr) -> Self {
        Self {
            number: raw.number,
            title: raw.title,
            body: raw.body,
            html_url: raw.html_url,
            created_at: raw.created_at,
            author_login: raw.user.login,
            head_ref: raw.head.ref_,
            head_sha: raw.head.sha,
        }
    }
}

impl<'de> Deserialize<'de> for Pr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        RawPr::deserialize(deserializer).map(Pr::from)
    }
}

/// Detailed, single-PR view from `GET /repos/{o}/{r}/pulls/{n}`. Carries the
/// mergeability and diff-size signals the triage evaluator needs, which the
/// pulls *list* endpoint omits.
#[derive(Debug, Clone, Deserialize)]
pub struct PrDetail {
    /// PR number within the repository.
    pub number: u64,
    /// GitHub's boolean mergeability, when computed. `None` while GitHub is
    /// still computing the merge state in the background.
    #[serde(default)]
    pub mergeable: Option<bool>,
    /// GitHub's `mergeable_state` string (`clean`, `dirty`, `blocked`,
    /// `behind`, `unstable`, `unknown`, …).
    #[serde(default)]
    pub mergeable_state: String,
    /// Total lines added across the PR.
    #[serde(default)]
    pub additions: u64,
    /// Total lines deleted across the PR.
    #[serde(default)]
    pub deletions: u64,
    /// Number of files changed by the PR.
    #[serde(default)]
    pub changed_files: u64,
    /// Head commit SHA (`head.sha`), used to query check runs.
    #[serde(default, rename = "head")]
    head: RawHead,
}

impl PrDetail {
    /// Head commit SHA (`head.sha`) for this PR.
    #[must_use]
    pub fn head_sha(&self) -> &str {
        &self.head.sha
    }
}

/// A single check run's relevant fields.
#[derive(Debug, Clone, Deserialize)]
struct RawCheckRun {
    /// `queued`, `in_progress`, or `completed`.
    #[serde(default)]
    status: String,
    /// `success`, `failure`, `neutral`, `cancelled`, `timed_out`,
    /// `action_required`, `skipped`, `stale`, … Present once `completed`.
    #[serde(default)]
    conclusion: Option<String>,
}

/// Envelope for `GET /commits/{sha}/check-runs`.
#[derive(Debug, Clone, Deserialize)]
struct RawCheckRunsResponse {
    #[serde(default)]
    check_runs: Vec<RawCheckRun>,
}

/// Summary of the check runs for a commit: total count, a per-conclusion
/// tally, and how many are still running. Built by [`CheckSummary::from_runs`]
/// so the counting logic is pure and unit-testable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckSummary {
    /// Total number of check runs reported.
    pub total: usize,
    /// Check runs with `conclusion == "success"`.
    pub success: usize,
    /// Check runs with `conclusion == "failure"`, `"timed_out"`,
    /// `"cancelled"`, `"action_required"`, or `"startup_failure"`.
    pub failure: usize,
    /// Check runs with `conclusion == "neutral"` or `"skipped"`.
    pub neutral: usize,
    /// Check runs not yet `completed` (`queued` / `in_progress`).
    pub running: usize,
}

impl CheckSummary {
    /// Whether any check run reached a failing conclusion.
    #[must_use]
    pub fn has_failure(&self) -> bool {
        self.failure > 0
    }

    /// Whether every check run completed successfully (and there is at least
    /// one).
    #[must_use]
    pub fn all_success(&self) -> bool {
        self.total > 0 && self.success == self.total
    }

    /// Whether any check run is still queued or in progress.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running > 0
    }

    /// Build a summary from raw check-run rows. `status != "completed"` counts
    /// as still-running regardless of any (absent) conclusion; otherwise the
    /// `conclusion` string is bucketed into success / failure / neutral.
    fn from_runs(runs: &[RawCheckRun]) -> CheckSummary {
        let mut summary = CheckSummary {
            total: runs.len(),
            ..CheckSummary::default()
        };
        for run in runs {
            if run.status != "completed" {
                summary.running += 1;
                continue;
            }
            match run.conclusion.as_deref() {
                Some("success") => summary.success += 1,
                Some(
                    "failure" | "timed_out" | "cancelled" | "action_required" | "startup_failure",
                ) => summary.failure += 1,
                // `neutral`/`skipped`/`stale`, or an unknown/missing conclusion
                // on a completed run, are counted as neutral rather than failing.
                _ => summary.neutral += 1,
            }
        }
        summary
    }
}

impl GithubClient {
    /// Build a client from the environment. Returns `None` when `GITHUB_TOKEN`
    /// is unset or empty. `GITHUB_API_URL` overrides the base URL (default
    /// `https://api.github.com`).
    #[must_use]
    pub fn from_env() -> Option<GithubClient> {
        let token = std::env::var("GITHUB_TOKEN").ok()?.trim().to_string();
        if token.is_empty() {
            return None;
        }
        let base_url = std::env::var("GITHUB_API_URL")
            .ok()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());
        let http = Client::builder().build().ok()?;
        Some(GithubClient {
            http,
            token,
            base_url,
        })
    }

    /// Apply the standard auth + content-negotiation headers to a request.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
    }

    /// Build a [`GithubError::Api`] from a non-success response, reading the body
    /// (best-effort) for diagnostics.
    async fn api_error(response: reqwest::Response) -> GithubError {
        let status = response.status().as_u16();
        let message = response.text().await.unwrap_or_default();
        GithubError::Api { status, message }
    }

    /// List every open pull request for `owner/repo`, paginating until a short
    /// page is returned. This endpoint (unlike the search API) includes each
    /// PR's `body`, `user.login`, and `head.ref`, which the Jules matcher needs.
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] on transport failure, a non-success HTTP status,
    /// or a response body that cannot be decoded.
    pub async fn list_open_pulls(&self, owner: &str, repo: &str) -> Result<Vec<Pr>, GithubError> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let url = format!(
                "{}/repos/{owner}/{repo}/pulls?state=open&per_page={PER_PAGE}&page={page}",
                self.base_url
            );
            let response = self.authorized(self.http.get(&url)).send().await?;
            if !response.status().is_success() {
                return Err(Self::api_error(response).await);
            }
            let prs: Vec<Pr> = response.json().await?;
            let received = prs.len();
            all.extend(prs);
            if received < PER_PAGE {
                break;
            }
            page += 1;
        }
        Ok(all)
    }

    /// Close a single pull request (`PATCH .../pulls/{number}` with
    /// `{"state":"closed"}`).
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] on transport failure or a non-success status.
    pub async fn close_pull(&self, owner: &str, repo: &str, number: u64) -> Result<(), GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/pulls/{number}", self.base_url);
        let response = self
            .authorized(self.http.patch(&url))
            .json(&serde_json::json!({ "state": "closed" }))
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::api_error(response).await)
        }
    }

    /// Delete a head branch (`DELETE .../git/refs/heads/{branch}`). A `404` or
    /// `422` is treated as success — the branch is already gone.
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] on transport failure or a non-success status
    /// other than `404`/`422`.
    pub async fn delete_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<(), GithubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/git/refs/heads/{branch}",
            self.base_url
        );
        let response = self.authorized(self.http.delete(&url)).send().await?;
        if response.status().is_success() || matches!(response.status().as_u16(), 404 | 422) {
            Ok(())
        } else {
            Err(Self::api_error(response).await)
        }
    }

    /// Fetch the detailed view of a single pull request
    /// (`GET .../pulls/{number}`). Unlike the pulls-list endpoint, this returns
    /// `mergeable`, `mergeable_state`, and the diff-size counters.
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] on transport failure, a non-success status, or a
    /// response body that cannot be decoded.
    pub async fn get_pull(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PrDetail, GithubError> {
        let url = format!("{}/repos/{owner}/{repo}/pulls/{number}", self.base_url);
        let response = self.authorized(self.http.get(&url)).send().await?;
        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }
        Ok(response.json().await?)
    }

    /// List the check runs for a commit (`GET .../commits/{sha}/check-runs`)
    /// and summarize them by conclusion and status.
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] on transport failure, a non-success status, or a
    /// response body that cannot be decoded.
    pub async fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<CheckSummary, GithubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/commits/{sha}/check-runs?per_page={PER_PAGE}",
            self.base_url
        );
        let response = self.authorized(self.http.get(&url)).send().await?;
        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }
        let body: RawCheckRunsResponse = response.json().await?;
        Ok(CheckSummary::from_runs(&body.check_runs))
    }
}

/// Classifies a pull request as Jules-authored or not. Pure and configurable so
/// it can be exercised in unit tests without any network access.
///
/// A PR matches when **either** its author login is in `author_logins`
/// **or** its body (case-insensitively) contains the configured `marker` or the
/// Jules task permalink `jules.google.com/task/`.
#[derive(Debug, Clone)]
pub struct JulesMatcher {
    author_logins: HashSet<String>,
    /// Lowercased body marker, compared case-insensitively.
    marker: String,
}

impl JulesMatcher {
    /// Build a matcher from the environment.
    ///
    /// * `JULES_PR_AUTHORS` — comma-separated author logins (default
    ///   `google-labs-jules[bot]`).
    /// * `JULES_PR_MARKER` — body substring marking a Jules PR (default
    ///   `created automatically by Jules for task`).
    #[must_use]
    pub fn from_env() -> JulesMatcher {
        let author_logins = std::env::var("JULES_PR_AUTHORS")
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .map_or_else(
                || HashSet::from([DEFAULT_JULES_AUTHOR.to_string()]),
                |raw| {
                    raw.split(',')
                        .map(|login| login.trim().to_string())
                        .filter(|login| !login.is_empty())
                        .collect()
                },
            );
        let marker = std::env::var("JULES_PR_MARKER")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .unwrap_or_else(|| DEFAULT_JULES_MARKER.to_string());
        JulesMatcher {
            author_logins,
            marker: marker.to_lowercase(),
        }
    }

    /// Whether `pr` looks Jules-authored: author-login match OR body-marker
    /// match OR a Jules task permalink in the body.
    #[must_use]
    pub fn is_jules(&self, pr: &Pr) -> bool {
        if self.author_logins.contains(&pr.author_login) {
            return true;
        }
        if let Some(body) = &pr.body {
            let lower = body.to_lowercase();
            if lower.contains(&self.marker) || lower.contains(JULES_TASK_LINK) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal Jules matcher with fixed config (no env dependency).
    fn matcher() -> JulesMatcher {
        JulesMatcher {
            author_logins: HashSet::from(["google-labs-jules[bot]".to_string()]),
            marker: DEFAULT_JULES_MARKER.to_lowercase(),
        }
    }

    fn pr(author: &str, body: Option<&str>) -> Pr {
        Pr {
            number: 1,
            title: "t".to_string(),
            body: body.map(ToString::to_string),
            html_url: "https://example.com/1".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            author_login: author.to_string(),
            head_ref: "nova-x".to_string(),
            head_sha: "cafef00d".to_string(),
        }
    }

    #[test]
    fn matches_bot_author() {
        let m = matcher();
        assert!(m.is_jules(&pr("google-labs-jules[bot]", None)));
    }

    #[test]
    fn matches_body_marker_from_human_author() {
        let m = matcher();
        let body = "Some summary.\n\ncreated automatically by Jules for task 12345";
        assert!(m.is_jules(&pr("madmax983", Some(body))));
    }

    #[test]
    fn matches_task_permalink_regardless_of_marker_casing() {
        let m = matcher();
        let body = "See https://JULES.GOOGLE.COM/task/abc for details.";
        assert!(m.is_jules(&pr("madmax983", Some(body))));
    }

    #[test]
    fn non_jules_pr_by_same_human_author_is_false() {
        let m = matcher();
        let body = "Hand-written fix for the login bug.";
        assert!(!m.is_jules(&pr("madmax983", Some(body))));
    }

    #[test]
    fn empty_or_missing_body_is_false_for_human_author() {
        let m = matcher();
        assert!(!m.is_jules(&pr("madmax983", None)));
        assert!(!m.is_jules(&pr("madmax983", Some(""))));
    }

    #[test]
    fn deserializes_github_pr_json_field_mapping() {
        // A representative GitHub pulls-list element (subset of fields).
        let json = r#"{
            "number": 4321,
            "title": "nova: tidy up imports",
            "body": "created automatically by Jules for task 99\n\nhttps://jules.google.com/task/99",
            "html_url": "https://github.com/madmax983/jules-rs/pull/4321",
            "created_at": "2026-07-10T08:15:30Z",
            "state": "open",
            "user": { "login": "madmax983", "id": 7 },
            "head": { "ref": "nova/tidy-imports", "sha": "deadbeef" }
        }"#;
        let pr: Pr = serde_json::from_str(json).expect("valid PR JSON");
        assert_eq!(pr.number, 4321);
        assert_eq!(pr.title, "nova: tidy up imports");
        assert_eq!(pr.author_login, "madmax983");
        assert_eq!(pr.head_ref, "nova/tidy-imports");
        assert_eq!(pr.head_sha, "deadbeef");
        assert_eq!(
            pr.html_url,
            "https://github.com/madmax983/jules-rs/pull/4321"
        );
        assert_eq!(pr.created_at, "2026-07-10T08:15:30Z");
        assert!(pr.body.as_deref().unwrap().contains("created automatically"));

        // The matcher flags it via the body marker even though the author is a
        // human account.
        assert!(matcher().is_jules(&pr));
    }

    #[test]
    fn get_pull_maps_mergeability_and_diffstat_fields() {
        let json = r#"{
            "number": 512,
            "state": "open",
            "mergeable": true,
            "mergeable_state": "clean",
            "additions": 42,
            "deletions": 7,
            "changed_files": 3,
            "head": { "ref": "nova/fix", "sha": "abc123" }
        }"#;
        let detail: PrDetail = serde_json::from_str(json).expect("valid PR detail JSON");
        assert_eq!(detail.number, 512);
        assert_eq!(detail.mergeable, Some(true));
        assert_eq!(detail.mergeable_state, "clean");
        assert_eq!(detail.additions, 42);
        assert_eq!(detail.deletions, 7);
        assert_eq!(detail.changed_files, 3);
        assert_eq!(detail.head_sha(), "abc123");
    }

    #[test]
    fn get_pull_tolerates_null_mergeable_and_missing_fields() {
        // GitHub returns mergeable: null while it computes the merge state.
        let json = r#"{
            "number": 7,
            "mergeable": null,
            "mergeable_state": "unknown"
        }"#;
        let detail: PrDetail = serde_json::from_str(json).expect("valid PR detail JSON");
        assert_eq!(detail.number, 7);
        assert_eq!(detail.mergeable, None);
        assert_eq!(detail.mergeable_state, "unknown");
        assert_eq!(detail.additions, 0);
        assert_eq!(detail.deletions, 0);
        assert_eq!(detail.changed_files, 0);
        assert_eq!(detail.head_sha(), "");
    }

    #[test]
    fn check_runs_summary_buckets_conclusions_and_status() {
        let json = r#"{
            "total_count": 5,
            "check_runs": [
                { "name": "build",  "status": "completed",  "conclusion": "success" },
                { "name": "test",   "status": "completed",  "conclusion": "failure" },
                { "name": "lint",   "status": "completed",  "conclusion": "neutral" },
                { "name": "deploy", "status": "in_progress", "conclusion": null },
                { "name": "docs",   "status": "queued",      "conclusion": null }
            ]
        }"#;
        let body: RawCheckRunsResponse = serde_json::from_str(json).expect("valid check-runs JSON");
        let summary = CheckSummary::from_runs(&body.check_runs);
        assert_eq!(summary.total, 5);
        assert_eq!(summary.success, 1);
        assert_eq!(summary.failure, 1);
        assert_eq!(summary.neutral, 1);
        assert_eq!(summary.running, 2);
        assert!(summary.has_failure());
        assert!(summary.is_running());
        assert!(!summary.all_success());
    }

    #[test]
    fn check_runs_summary_all_success() {
        let json = r#"{
            "check_runs": [
                { "status": "completed", "conclusion": "success" },
                { "status": "completed", "conclusion": "success" }
            ]
        }"#;
        let body: RawCheckRunsResponse = serde_json::from_str(json).expect("valid check-runs JSON");
        let summary = CheckSummary::from_runs(&body.check_runs);
        assert!(summary.all_success());
        assert!(!summary.has_failure());
        assert!(!summary.is_running());
    }

    #[test]
    fn check_runs_summary_empty_is_not_all_success() {
        let summary = CheckSummary::from_runs(&[]);
        assert_eq!(summary.total, 0);
        assert!(!summary.all_success());
        assert!(!summary.has_failure());
        assert!(!summary.is_running());
    }

    #[test]
    fn deserializes_array_of_prs() {
        let json = r#"[
            {"number":1,"title":"a","html_url":"","created_at":"","user":{"login":"x"},"head":{"ref":"b"}},
            {"number":2,"title":"c","body":null,"html_url":"","created_at":"","user":{"login":"y"},"head":{"ref":"d"}}
        ]"#;
        let prs: Vec<Pr> = serde_json::from_str(json).expect("valid PR array");
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].author_login, "x");
        assert_eq!(prs[1].head_ref, "d");
        assert!(prs[1].body.is_none());
    }
}
