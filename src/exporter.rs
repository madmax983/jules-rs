use std::fmt::Write;

use crate::types::{ListActivitiesParams, SessionState};
use crate::{JulesClient, JulesError};

/// Exports session data to various formats.
pub struct SessionExporter {
    client: JulesClient,
    session_id: String,
}

impl SessionExporter {
    /// Creates a new exporter for the given session.
    pub fn new(client: JulesClient, session_id: impl Into<String>) -> Self {
        Self {
            client,
            session_id: session_id.into(),
        }
    }

    /// Fetches the session history and generates a Markdown report.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if API calls fail.
    pub async fn export_to_markdown(&self) -> Result<String, JulesError> {
        let session = self.client.get_session(&self.session_id).await?;

        let mut out = String::new();

        // Header
        let id = session.id.as_deref().unwrap_or(&session.name);
        writeln!(out, "# Session Report: {id}").unwrap();

        if let Some(title) = &session.title {
            writeln!(out, "## Title: {title}").unwrap();
        }

        let state = session
            .state
            .unwrap_or(SessionState::SessionStateUnspecified);
        writeln!(out, "**Status:** {state:?}").unwrap();

        if let Some(prompt) = &session.prompt {
            writeln!(out, "\n**Original Prompt:**\n> {prompt}").unwrap();
        }

        writeln!(out, "\n---").unwrap();

        // Activities
        let mut page_token = None;
        let mut activity_count = 0;

        loop {
            let params = ListActivitiesParams {
                page_size: Some(100),
                page_token: page_token.clone(),
                ..Default::default()
            };

            let response = self
                .client
                .list_activities(&self.session_id, params)
                .await?;

            for activity in response.activities {
                activity_count += 1;
                writeln!(out, "\n## Activity {activity_count}").unwrap();

                if let Some(status) = activity.status {
                    writeln!(out, "**Status:** {status:?}").unwrap();
                }

                if let Some(ts) = activity.timestamp {
                    writeln!(out, "**Time:** {ts}").unwrap();
                }

                if let Some(overview) = activity.overview {
                    if let Some(thoughts) = overview.thoughts {
                        writeln!(out, "\n**Thoughts:**\n{thoughts}").unwrap();
                    }
                    if let Some(summary) = overview.summary {
                        writeln!(out, "\n**Summary:** {summary}").unwrap();
                    }
                }

                if let Some(req) = activity.user_input_request {
                    writeln!(out, "\n**Question:** {}", req.question).unwrap();
                    if !req.suggested_responses.is_empty() {
                        writeln!(out, "Suggested Responses:").unwrap();
                        for resp in req.suggested_responses {
                            writeln!(out, "- {}", resp.response).unwrap();
                        }
                    }
                }

                if let Some(tool) = activity.tool_use {
                    writeln!(out, "\n**Tool:** `{}`", tool.tool_name).unwrap();
                    writeln!(out, "Input:\n```json\n{}\n```", tool.input).unwrap();
                }

                if let Some(diff) = activity.view_diff {
                    if let Some(title) = diff.title {
                        writeln!(out, "\n**Diff:** {title}").unwrap();
                    }
                    writeln!(out, "```diff\n{}\n```", diff.diff).unwrap();
                }

                if let Some(commit) = activity.commit {
                    writeln!(out, "\n**Commit:** {}", commit.commit_message).unwrap();
                }

                if let Some(pr) = activity.create_pull_request {
                    writeln!(out, "\n**Pull Request:** {}", pr.title).unwrap();
                    writeln!(out, "{}", pr.body).unwrap();
                }

                if let Some(output) = activity.output {
                    if !output.changed_files.is_empty() {
                        writeln!(out, "\n## Changed Files").unwrap();
                        for file in output.changed_files {
                            writeln!(out, "### {}", file.path).unwrap();
                            writeln!(out, "```diff\n{}\n```", file.diff).unwrap();
                        }
                    }
                    if let Some(pr) = output.pull_request {
                        writeln!(
                            out,
                            "\n**PR Created:** {} (#{})",
                            pr.url,
                            pr.number.unwrap_or(0)
                        )
                        .unwrap();
                    }
                }
            }

            match response.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        Ok(out)
    }
}
