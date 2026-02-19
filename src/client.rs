use std::collections::HashSet;
use std::time::Duration;
use std::{fmt, fmt::Formatter};

use reqwest::{Client, Request, RequestBuilder, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::time::sleep;

use crate::error::{ApiError, JulesError};
use crate::types::{
    CreateSessionRequest, ListActivitiesParams, ListActivitiesResponse, ListSessionsParams,
    ListSessionsResponse, ListSourcesParams, ListSourcesResponse, SendMessageRequest, Session,
    SessionState, Source,
};

const DEFAULT_BASE_URL: &str = "https://jules.googleapis.com/v1alpha/";
const MAX_PAGE_SIZE: u32 = 100;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);

/// Retry/backoff settings used for transient failures.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Number of retries after the initial attempt.
    pub max_retries: u32,
    /// Delay before the first retry attempt.
    pub initial_backoff: Duration,
    /// Upper bound for exponential backoff growth.
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }
}

/// Per-request timeout settings.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutPolicy {
    /// Total request timeout for each API call.
    pub request_timeout: Duration,
}

impl Default for TimeoutPolicy {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

/// Async HTTP client for the Jules API.
#[derive(Clone)]
pub struct JulesClient {
    http_client: Client,
    base_url: Url,
    api_key: String,
    retry_policy: RetryPolicy,
    timeout_policy: TimeoutPolicy,
}

/// Builder for [`JulesClient`].
#[must_use]
#[derive(Clone)]
pub struct JulesClientBuilder {
    api_key: String,
    base_url: String,
    http_client: Option<Client>,
    retry_policy: RetryPolicy,
    timeout_policy: TimeoutPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryBehavior {
    Idempotent,
    NonIdempotent,
}

impl JulesClient {
    /// Starts building a Jules client with an API key.
    pub fn builder(api_key: impl Into<String>) -> JulesClientBuilder {
        JulesClientBuilder {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            http_client: None,
            retry_policy: RetryPolicy::default(),
            timeout_policy: TimeoutPolicy::default(),
        }
    }

    /// Creates a session from an initial prompt and optional source context.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if the request fails, the API returns non-success,
    /// or the response payload cannot be decoded.
    pub async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> Result<Session, JulesError> {
        let url = self.endpoint("sessions")?;
        let request = self.authorized(self.http_client.post(url)).json(&request);
        self.send_json(request, RetryBehavior::NonIdempotent).await
    }

    /// Lists existing sessions.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if page params are invalid, transport fails, the API
    /// returns non-success, or decoding fails.
    pub async fn list_sessions(
        &self,
        params: ListSessionsParams,
    ) -> Result<ListSessionsResponse, JulesError> {
        validate_page_size(params.page_size)?;
        let url = self.endpoint("sessions")?;
        let request = self.authorized(self.http_client.get(url)).query(&params);
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    /// Gets a session by id or resource name (for example `s-1` or `sessions/s-1`).
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if `session_id` is invalid, transport fails,
    /// the API returns non-success, or decoding fails.
    pub async fn get_session(&self, session_id: &str) -> Result<Session, JulesError> {
        let session_id = normalize_simple_id(session_id, "sessions")?;
        let url = self.endpoint(&format!("sessions/{session_id}"))?;
        let request = self.authorized(self.http_client.get(url));
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    /// Deletes a session by id or resource name.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if `session_id` is invalid, transport fails,
    /// or the API returns non-success.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), JulesError> {
        let session_id = normalize_simple_id(session_id, "sessions")?;
        let url = self.endpoint(&format!("sessions/{session_id}"))?;
        let request = self.authorized(self.http_client.delete(url));
        self.send_empty(request, RetryBehavior::Idempotent).await
    }

    /// Deletes all sessions currently in the `COMPLETED` state.
    ///
    /// Returns the resource names for sessions that were deleted.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if listing pages fails, a deletion fails, or
    /// transport/API errors occur.
    pub async fn delete_completed_sessions(&self) -> Result<Vec<String>, JulesError> {
        let mut deleted_sessions = Vec::new();
        let mut page_token = None;
        let mut seen_page_tokens = HashSet::new();

        loop {
            let response = self
                .list_sessions(ListSessionsParams {
                    page_size: Some(MAX_PAGE_SIZE),
                    page_token,
                    filter: None,
                })
                .await?;

            for session in response.sessions {
                if session.state == Some(SessionState::Completed) {
                    let session_name = session.name;
                    self.delete_session(&session_name).await?;
                    deleted_sessions.push(session_name);
                }
            }

            match response.next_page_token.filter(|token| !token.is_empty()) {
                Some(next_page_token) => {
                    if !seen_page_tokens.insert(next_page_token.clone()) {
                        return Err(JulesError::InvalidArgument(format!(
                            "duplicate next_page_token returned by API: `{next_page_token}`"
                        )));
                    }
                    page_token = Some(next_page_token);
                }
                None => break,
            }
        }

        Ok(deleted_sessions)
    }

    /// Sends a follow-up message prompt to a session.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if `session_id` or `prompt` is invalid,
    /// transport fails, or the API returns non-success.
    pub async fn send_message(&self, session_id: &str, prompt: &str) -> Result<(), JulesError> {
        let session_id = normalize_simple_id(session_id, "sessions")?;
        if prompt.trim().is_empty() {
            return Err(JulesError::InvalidArgument(
                "prompt cannot be empty".to_string(),
            ));
        }

        let url = self.endpoint(&format!("sessions/{session_id}:sendMessage"))?;
        let body = SendMessageRequest {
            prompt: prompt.to_string(),
        };
        let request = self.authorized(self.http_client.post(url)).json(&body);
        self.send_empty(request, RetryBehavior::NonIdempotent).await
    }

    /// Approves the current session plan.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if `session_id` is invalid, transport fails,
    /// or the API returns non-success.
    pub async fn approve_plan(&self, session_id: &str) -> Result<(), JulesError> {
        let session_id = normalize_simple_id(session_id, "sessions")?;
        let url = self.endpoint(&format!("sessions/{session_id}:approvePlan"))?;
        let request = self
            .authorized(self.http_client.post(url))
            .json(&serde_json::json!({}));
        self.send_empty(request, RetryBehavior::NonIdempotent).await
    }

    /// Lists activities for a session.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if params are invalid, transport fails,
    /// the API returns non-success, or decoding fails.
    pub async fn list_activities(
        &self,
        session_id: &str,
        params: ListActivitiesParams,
    ) -> Result<ListActivitiesResponse, JulesError> {
        validate_page_size(params.page_size)?;
        let session_id = normalize_simple_id(session_id, "sessions")?;
        let url = self.endpoint(&format!("sessions/{session_id}/activities"))?;
        let request = self.authorized(self.http_client.get(url)).query(&params);
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    /// Gets a specific activity by session id and activity id.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if ids are invalid, transport fails,
    /// the API returns non-success, or decoding fails.
    pub async fn get_activity(
        &self,
        session_id: &str,
        activity_id: &str,
    ) -> Result<crate::types::Activity, JulesError> {
        let session_id = normalize_simple_id(session_id, "sessions")?;
        let activity_id = normalize_simple_id(activity_id, "activities")?;
        let url = self.endpoint(&format!("sessions/{session_id}/activities/{activity_id}"))?;
        let request = self.authorized(self.http_client.get(url));
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    /// Lists accessible sources.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if page params are invalid, transport fails,
    /// the API returns non-success, or decoding fails.
    pub async fn list_sources(
        &self,
        params: ListSourcesParams,
    ) -> Result<ListSourcesResponse, JulesError> {
        validate_page_size(params.page_size)?;
        let url = self.endpoint("sources")?;
        let request = self.authorized(self.http_client.get(url)).query(&params);
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    /// Gets a source by id or resource name (for example `demo` or `sources/demo`).
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if `source_id` is invalid, transport fails,
    /// the API returns non-success, or decoding fails.
    pub async fn get_source(&self, source_id: &str) -> Result<Source, JulesError> {
        let source_id = normalize_resource_id(source_id, "sources", true)?;
        let url = self.endpoint(&format!("sources/{source_id}"))?;
        let request = self.authorized(self.http_client.get(url));
        self.send_json(request, RetryBehavior::Idempotent).await
    }

    fn endpoint(&self, path: &str) -> Result<Url, JulesError> {
        self.base_url.join(path).map_err(|error| {
            JulesError::InvalidArgument(format!("invalid endpoint path `{path}`: {error}"))
        })
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        request
            .header("x-goog-api-key", &self.api_key)
            .timeout(self.timeout_policy.request_timeout)
    }

    async fn send_json<T>(
        &self,
        request: RequestBuilder,
        retry_behavior: RetryBehavior,
    ) -> Result<T, JulesError>
    where
        T: DeserializeOwned,
    {
        let response = self.execute_with_retry(request, retry_behavior).await?;
        let status_code = response.status().as_u16();
        let body = response.bytes().await?;

        if (200..300).contains(&status_code) {
            serde_json::from_slice(&body).map_err(|source| JulesError::Decode {
                source,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        } else {
            Err(parse_api_error(status_code, &body))
        }
    }

    async fn send_empty(
        &self,
        request: RequestBuilder,
        retry_behavior: RetryBehavior,
    ) -> Result<(), JulesError> {
        let response = self.execute_with_retry(request, retry_behavior).await?;
        let status_code = response.status().as_u16();
        if (200..300).contains(&status_code) {
            Ok(())
        } else {
            let body = response.bytes().await?;
            Err(parse_api_error(status_code, &body))
        }
    }

    async fn execute_with_retry(
        &self,
        request_builder: RequestBuilder,
        retry_behavior: RetryBehavior,
    ) -> Result<reqwest::Response, JulesError> {
        let max_retries = if retry_behavior == RetryBehavior::Idempotent {
            self.retry_policy.max_retries
        } else {
            0
        };
        let request = request_builder.build()?;
        let request_template = request.try_clone();
        let mut current_request = request;
        let mut retries_used = 0_u32;
        let mut backoff = self.retry_policy.initial_backoff;

        loop {
            match self.http_client.execute(current_request).await {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let should_retry =
                        Self::is_retryable_status(status_code) && retries_used < max_retries;
                    if !should_retry {
                        return Ok(response);
                    }

                    let Some(next_request) = request_template.as_ref().and_then(Request::try_clone)
                    else {
                        return Ok(response);
                    };

                    sleep(backoff).await;
                    backoff = next_backoff(backoff, self.retry_policy.max_backoff);
                    retries_used = retries_used.saturating_add(1);
                    current_request = next_request;
                }
                Err(error) => {
                    let should_retry =
                        Self::is_retryable_error(&error) && retries_used < max_retries;
                    if !should_retry {
                        return Err(JulesError::Http(error));
                    }

                    let Some(next_request) = request_template.as_ref().and_then(Request::try_clone)
                    else {
                        return Err(JulesError::Http(error));
                    };

                    sleep(backoff).await;
                    backoff = next_backoff(backoff, self.retry_policy.max_backoff);
                    retries_used = retries_used.saturating_add(1);
                    current_request = next_request;
                }
            }
        }
    }

    fn is_retryable_status(status_code: u16) -> bool {
        matches!(status_code, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504)
    }

    fn is_retryable_error(error: &reqwest::Error) -> bool {
        error.is_connect() || error.is_timeout()
    }
}

impl JulesClientBuilder {
    /// Overrides the API base URL (useful for tests).
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Injects a preconfigured reqwest client.
    pub fn http_client(mut self, http_client: Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Overrides retry and backoff behavior.
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Overrides per-request timeout behavior.
    pub fn timeout_policy(mut self, timeout_policy: TimeoutPolicy) -> Self {
        self.timeout_policy = timeout_policy;
        self
    }

    /// Builds a [`JulesClient`].
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] when the API key is empty, `base_url` is invalid,
    /// or client policies are invalid.
    pub fn build(self) -> Result<JulesClient, JulesError> {
        let api_key = self.api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(JulesError::InvalidArgument(
                "api_key cannot be empty".to_string(),
            ));
        }

        validate_retry_policy(&self.retry_policy)?;
        validate_timeout_policy(&self.timeout_policy)?;

        let normalized_base_url = normalize_base_url(&self.base_url)?;
        let base_url = Url::parse(&normalized_base_url).map_err(|error| {
            JulesError::InvalidArgument(format!(
                "invalid base_url `{normalized_base_url}`: {error}"
            ))
        })?;
        let http_client = match self.http_client {
            Some(client) => client,
            None => Client::builder().build()?,
        };

        Ok(JulesClient {
            http_client,
            base_url,
            api_key,
            retry_policy: self.retry_policy,
            timeout_policy: self.timeout_policy,
        })
    }
}

impl fmt::Debug for JulesClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JulesClient")
            .field("http_client", &self.http_client)
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("retry_policy", &self.retry_policy)
            .field("timeout_policy", &self.timeout_policy)
            .finish()
    }
}

impl fmt::Debug for JulesClientBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JulesClientBuilder")
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("http_client", &self.http_client)
            .field("retry_policy", &self.retry_policy)
            .field("timeout_policy", &self.timeout_policy)
            .finish()
    }
}

fn normalize_base_url(base_url: &str) -> Result<String, JulesError> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(JulesError::InvalidArgument(
            "base_url cannot be empty".to_string(),
        ));
    }

    if trimmed.ends_with('/') {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/"))
    }
}

fn normalize_simple_id<'a>(value: &'a str, resource_type: &str) -> Result<&'a str, JulesError> {
    normalize_resource_id(value, resource_type, false)
}

fn normalize_resource_id<'a>(
    value: &'a str,
    resource_type: &str,
    allow_nested: bool,
) -> Result<&'a str, JulesError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(JulesError::InvalidArgument(format!(
            "{resource_type} id cannot be empty"
        )));
    }

    let prefix = format!("{resource_type}/");
    let normalized = trimmed.strip_prefix(&prefix).unwrap_or(trimmed);
    if !allow_nested && normalized.contains('/') {
        return Err(JulesError::InvalidArgument(format!(
            "{resource_type} id must not include `/`"
        )));
    }
    if normalized.is_empty() {
        return Err(JulesError::InvalidArgument(format!(
            "{resource_type} id cannot be empty"
        )));
    }

    Ok(normalized)
}

fn validate_page_size(page_size: Option<u32>) -> Result<(), JulesError> {
    if let Some(page_size) = page_size {
        if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
            return Err(JulesError::InvalidArgument(format!(
                "page_size must be in 1..={MAX_PAGE_SIZE}"
            )));
        }
    }
    Ok(())
}

fn validate_retry_policy(policy: &RetryPolicy) -> Result<(), JulesError> {
    if policy.max_retries > 0 && policy.max_backoff < policy.initial_backoff {
        return Err(JulesError::InvalidArgument(
            "retry_policy.max_backoff must be >= retry_policy.initial_backoff".to_string(),
        ));
    }
    Ok(())
}

fn validate_timeout_policy(policy: &TimeoutPolicy) -> Result<(), JulesError> {
    if policy.request_timeout.is_zero() {
        return Err(JulesError::InvalidArgument(
            "timeout_policy.request_timeout must be > 0".to_string(),
        ));
    }
    Ok(())
}

fn next_backoff(current: Duration, max_backoff: Duration) -> Duration {
    current.saturating_mul(2).min(max_backoff)
}

#[derive(Debug, Deserialize)]
struct GoogleErrorEnvelope {
    error: GoogleErrorBody,
}

#[derive(Debug, Deserialize)]
struct GoogleErrorBody {
    code: Option<i64>,
    message: Option<String>,
    status: Option<String>,
}

fn parse_api_error(status_code: u16, body: &[u8]) -> JulesError {
    if let Ok(envelope) = serde_json::from_slice::<GoogleErrorEnvelope>(body) {
        return JulesError::Api(ApiError {
            status_code,
            code: envelope.error.code,
            status: envelope.error.status,
            message: envelope
                .error
                .message
                .unwrap_or_else(|| format!("request failed with status {status_code}")),
        });
    }

    let body_text = String::from_utf8_lossy(body).trim().to_string();
    let message = if body_text.is_empty() {
        format!("request failed with status {status_code}")
    } else {
        body_text
    };

    JulesError::Api(ApiError {
        status_code,
        code: None,
        status: None,
        message,
    })
}
