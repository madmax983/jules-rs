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
            let status = response.status();
            if !status.is_success() {
                let message = response.text().await.unwrap_or_default();
                return Err(GithubError::Api {
                    status: status.as_u16(),
                    message,
                });
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
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let message = response.text().await.unwrap_or_default();
            Err(GithubError::Api {
                status: status.as_u16(),
                message,
            })
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
        let status = response.status();
        if status.is_success() || matches!(status.as_u16(), 404 | 422) {
            Ok(())
        } else {
            let message = response.text().await.unwrap_or_default();
            Err(GithubError::Api {
                status: status.as_u16(),
                message,
            })
        }
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
                || {
                    let mut set = HashSet::new();
                    set.insert(DEFAULT_JULES_AUTHOR.to_string());
                    set
                },
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
        let mut author_logins = HashSet::new();
        author_logins.insert("google-labs-jules[bot]".to_string());
        JulesMatcher {
            author_logins,
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
