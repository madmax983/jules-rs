//! Minimal Anthropic (Claude) Messages API client for the PR-triage agent.
//!
//! This is a deliberately tiny client — a [`reqwest::Client`], an API key, and
//! a single [`AnthropicClient::judge`] method that POSTs one user turn to the
//! Messages API and returns the assistant's text. It mirrors the conventions of
//! [`crate::github`] rather than pulling in the Anthropic SDK.
//!
//! The triage evaluator asks the model to answer with STRICT JSON
//! (`{ "recommendation": ..., "confidence": ..., "rationale": ... }`). Parsing
//! that answer is handled by the pure, unit-tested [`parse_judgment`], which
//! never panics: any malformed answer degrades to a `needs_human` judgment with
//! the raw text as its rationale.

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

/// Default Anthropic Messages API base URL.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Anthropic API version pin sent on every request.
const API_VERSION: &str = "2023-06-01";

/// Default model — a current Sonnet-tier model, chosen to keep triage cost low.
const DEFAULT_MODEL: &str = "claude-sonnet-5";

/// Upper bound on the assistant response length; the JSON verdict is small.
const MAX_TOKENS: u32 = 1024;

/// Errors that can occur while talking to the Anthropic Messages API.
#[derive(Debug, Error)]
pub enum AnthropicError {
    /// Network transport error while sending or receiving a request.
    #[error("anthropic request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The API returned a non-success HTTP status.
    #[error("anthropic API error {status}: {message}")]
    Api {
        /// HTTP status code from the response.
        status: u16,
        /// Response body (best-effort), useful for diagnostics.
        message: String,
    },
    /// The API responded 2xx but with no text content block.
    #[error("anthropic response contained no text content")]
    EmptyResponse,
}

/// Async client for the subset of the Anthropic Messages API triage needs.
pub struct AnthropicClient {
    http: Client,
    api_key: String,
    base_url: String,
    model: String,
}

/// One content block in a Messages API response.
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

/// Top-level Messages API response envelope.
#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

impl AnthropicClient {
    /// Build a client from the environment. Returns `None` when
    /// `ANTHROPIC_API_KEY` is unset or empty — in that case the LLM evaluator is
    /// simply unavailable. `ANTHROPIC_BASE_URL` overrides the base URL and
    /// `ANTHROPIC_MODEL` overrides the default model.
    #[must_use]
    pub fn from_env() -> Option<AnthropicClient> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?.trim().to_string();
        if api_key.is_empty() {
            return None;
        }
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .ok()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = std::env::var("ANTHROPIC_MODEL")
            .ok()
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let http = Client::builder().build().ok()?;
        Some(AnthropicClient {
            http,
            api_key,
            base_url,
            model,
        })
    }

    /// The model id this client sends requests to.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send `prompt` as a single user turn and return the assistant's text.
    ///
    /// # Errors
    ///
    /// Returns [`AnthropicError`] on transport failure, a non-success HTTP
    /// status, a body that cannot be decoded, or a 2xx response with no text
    /// content block.
    pub async fn judge(&self, prompt: &str) -> Result<String, AnthropicError> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(AnthropicError::Api { status, message });
        }

        let parsed: MessagesResponse = response.json().await?;
        let text = parsed
            .content
            .into_iter()
            .filter(|block| block.block_type == "text")
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            return Err(AnthropicError::EmptyResponse);
        }
        Ok(text)
    }
}

/// A structured judgment parsed from the model's answer. The `recommendation`
/// is kept as the raw string (`merge` / `close` / `needs_human`) so the mapping
/// to a strongly-typed recommendation lives in the triage layer.
#[derive(Debug, Clone, PartialEq)]
pub struct RawJudgment {
    /// One of `merge`, `close`, or `needs_human` (lowercased).
    pub recommendation: String,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// One-line rationale.
    pub rationale: String,
}

/// The exact JSON shape the model is asked to emit.
#[derive(Debug, Deserialize)]
struct JudgmentJson {
    recommendation: String,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    rationale: String,
}

/// Extract the first balanced `{ ... }` JSON object from `text`, tolerating
/// prose or code fences around it. Returns `None` when no balanced object is
/// present. Brace counting ignores braces inside JSON string literals (with
/// escape handling) so prose braces or `\"` inside strings don't confuse it.
#[must_use]
pub fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse the model's answer into a [`RawJudgment`]. This never panics: if no
/// JSON object can be extracted, or it fails to deserialize, or the
/// recommendation is unrecognized, the result downgrades to `needs_human` with
/// the raw text (trimmed) as the rationale.
#[must_use]
pub fn parse_judgment(text: &str) -> RawJudgment {
    let fallback = || RawJudgment {
        recommendation: "needs_human".to_string(),
        confidence: 0.0,
        rationale: text.trim().to_string(),
    };

    let Some(json) = extract_json_object(text) else {
        return fallback();
    };
    let Ok(parsed) = serde_json::from_str::<JudgmentJson>(json) else {
        return fallback();
    };

    let recommendation = parsed.recommendation.trim().to_lowercase();
    let recommendation = match recommendation.as_str() {
        "merge" | "close" | "needs_human" => recommendation,
        // Unknown label — be conservative.
        _ => return fallback(),
    };
    let rationale = if parsed.rationale.trim().is_empty() {
        text.trim().to_string()
    } else {
        parsed.rationale.trim().to_string()
    };
    RawJudgment {
        recommendation,
        confidence: parsed.confidence.clamp(0.0, 1.0),
        rationale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json_object() {
        let text = r#"{"recommendation": "merge", "confidence": 0.9, "rationale": "CI green, small diff"}"#;
        let judgment = parse_judgment(text);
        assert_eq!(judgment.recommendation, "merge");
        assert!((judgment.confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(judgment.rationale, "CI green, small diff");
    }

    #[test]
    fn extracts_json_from_surrounding_prose() {
        let text = "Sure! Here is my assessment:\n\n```json\n{\n  \"recommendation\": \"close\",\n  \"confidence\": 0.75,\n  \"rationale\": \"CI is failing on {main}\"\n}\n```\nLet me know if you need more.";
        let judgment = parse_judgment(text);
        assert_eq!(judgment.recommendation, "close");
        assert!((judgment.confidence - 0.75).abs() < f64::EPSILON);
        // The brace inside the string literal must not truncate extraction.
        assert_eq!(judgment.rationale, "CI is failing on {main}");
    }

    #[test]
    fn normalizes_recommendation_casing() {
        let text = r#"prose {"recommendation": "NEEDS_HUMAN", "confidence": 0.4, "rationale": "conflicting signals"} more"#;
        let judgment = parse_judgment(text);
        assert_eq!(judgment.recommendation, "needs_human");
    }

    #[test]
    fn clamps_out_of_range_confidence() {
        let text = r#"{"recommendation": "merge", "confidence": 1.7, "rationale": "x"}"#;
        assert!((parse_judgment(text).confidence - 1.0).abs() < f64::EPSILON);
        let text = r#"{"recommendation": "merge", "confidence": -0.5, "rationale": "x"}"#;
        assert!((parse_judgment(text).confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn downgrades_on_unparseable_text() {
        let text = "I cannot produce JSON but I think this should be closed.";
        let judgment = parse_judgment(text);
        assert_eq!(judgment.recommendation, "needs_human");
        assert!((judgment.confidence - 0.0).abs() < f64::EPSILON);
        assert_eq!(judgment.rationale, text);
    }

    #[test]
    fn downgrades_on_unknown_recommendation_label() {
        let text = r#"{"recommendation": "delete_everything", "confidence": 0.99, "rationale": "nope"}"#;
        let judgment = parse_judgment(text);
        assert_eq!(judgment.recommendation, "needs_human");
        // Raw text is retained as rationale so the report keeps the context.
        assert_eq!(judgment.rationale, text);
    }

    #[test]
    fn empty_rationale_falls_back_to_raw_text() {
        let text = r#"{"recommendation": "merge", "confidence": 0.8, "rationale": ""}"#;
        let judgment = parse_judgment(text);
        assert_eq!(judgment.rationale, text);
    }

    #[test]
    fn extract_json_object_ignores_braces_in_strings() {
        let text = r#"{"a": "has } brace", "b": 1}"#;
        assert_eq!(extract_json_object(text), Some(text));
    }

    #[test]
    fn extract_json_object_returns_none_without_object() {
        assert_eq!(extract_json_object("no object here"), None);
        assert_eq!(extract_json_object("{ unbalanced"), None);
    }
}
