use thiserror::Error;

/// API error payload returned by Jules on non-2xx responses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Jules API error {status_code}: {message}")]
pub struct ApiError {
    /// HTTP status code from the response.
    pub status_code: u16,
    /// Optional numeric API error code from Google-style payloads.
    pub code: Option<i64>,
    /// Optional symbolic status such as `NOT_FOUND`.
    pub status: Option<String>,
    /// Human-readable message returned by the API.
    pub message: String,
}

/// Errors that can occur while calling the Jules API.
#[derive(Debug, Error)]
pub enum JulesError {
    /// Caller provided invalid input before a request was sent.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Network transport error while sending or receiving a request.
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// Response body could not be decoded into the expected model.
    #[error("failed to decode response body: {source}; body: {body}")]
    Decode {
        #[source]
        source: serde_json::Error,
        body: String,
    },
    /// Jules API returned a non-success response.
    #[error(transparent)]
    Api(#[from] ApiError),
}
