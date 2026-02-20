use std::collections::HashSet;
use std::fmt::{self, Write};

use crate::{Activity, JulesClient, JulesError, ListActivitiesParams, Session};

const MAX_PAGE_SIZE: u32 = 100;

/// Exporter for session reports.
pub struct SessionExporter<'a> {
    client: &'a JulesClient,
}

impl<'a> SessionExporter<'a> {
    /// Creates a new `SessionExporter` with the given client.
    #[must_use]
    pub fn new(client: &'a JulesClient) -> Self {
        Self { client }
    }

    /// Exports a session report to Markdown.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if session or activity retrieval fails.
    pub async fn export_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;
        let activities = self.list_all_activities(session_id).await?;

        let mut output = String::new();
        Self::format_header(&mut output, &session)
            .map_err(|e| JulesError::InvalidArgument(format!("formatting error: {e}")))?;
        Self::format_activities(&mut output, &activities)
            .map_err(|e| JulesError::InvalidArgument(format!("formatting error: {e}")))?;

        Ok(output)
    }

    async fn list_all_activities(&self, session_id: &str) -> Result<Vec<Activity>, JulesError> {
        let mut activities = Vec::new();
        let mut page_token = None;
        let mut seen_page_tokens = HashSet::new();

        loop {
            let response = self
                .client
                .list_activities(
                    session_id,
                    ListActivitiesParams {
                        page_size: Some(MAX_PAGE_SIZE),
                        page_token: page_token.clone(),
                        filter: None,
                        create_time: None,
                    },
                )
                .await?;

            activities.extend(response.activities);

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

        Ok(activities)
    }

    fn format_header(output: &mut String, session: &Session) -> fmt::Result {
        writeln!(
            output,
            "# Session Report: {}",
            session.title.as_deref().unwrap_or("Untitled")
        )?;
        writeln!(output, "**ID:** `{}`", session.name)?;
        if let Some(state) = session.state {
            writeln!(output, "**State:** `{state:?}`")?;
        }
        if let Some(prompt) = &session.prompt {
            writeln!(output, "\n## Prompt\n> {prompt}")?;
        }
        Ok(())
    }

    fn format_activities(output: &mut String, activities: &[Activity]) -> fmt::Result {
        if activities.is_empty() {
            writeln!(output, "\n## Activities\n*(No activities found)*")?;
            return Ok(());
        }

        writeln!(output, "\n## Activities")?;
        for activity in activities {
            Self::format_single_activity(output, activity)?;
        }
        Ok(())
    }

    fn format_single_activity(output: &mut String, activity: &Activity) -> fmt::Result {
        writeln!(
            output,
            "\n### {} - {}",
            activity.timestamp.as_deref().unwrap_or("Unknown Time"),
            activity.activity_type.as_deref().unwrap_or("Unknown Type")
        )?;

        if let Some(detail) = &activity.detail {
            writeln!(output, "\n{detail}")?;
        }

        if let Some(tool_use) = &activity.tool_use {
            writeln!(output, "\n**Tool:** `{}`", tool_use.tool_name)?;
            writeln!(output, "```json\n{}\n```", tool_use.input)?;
        }

        if let Some(user_input) = &activity.user_input_request {
            writeln!(output, "\n**Question:** {}", user_input.question)?;
            if !user_input.suggested_responses.is_empty() {
                writeln!(output, "\n*Suggested Responses:*")?;
                for resp in &user_input.suggested_responses {
                    writeln!(output, "- {}", resp.response)?;
                }
            }
        }

        if let Some(view_diff) = &activity.view_diff {
            if let Some(title) = &view_diff.title {
                writeln!(output, "\n**Diff:** {title}")?;
            }
            writeln!(output, "```diff\n{}\n```", view_diff.diff)?;
        }

        if let Some(overview) = &activity.overview {
            if let Some(summary) = &overview.summary {
                writeln!(output, "\n**Summary:** {summary}")?;
            }
            if let Some(thoughts) = &overview.thoughts {
                writeln!(output, "\n**Thoughts:**\n> {}", thoughts.replace('\n', "\n> "))?;
            }
        }

        if let Some(plan) = &activity.plan {
            writeln!(output, "\n**Plan:**")?;
            for (i, step) in plan.steps.iter().enumerate() {
                writeln!(output, "{}. {} ({})", i + 1, step.description, step.status)?;
            }
        }

        if let Some(commit) = &activity.commit {
            writeln!(output, "\n**Commit:**")?;
            writeln!(output, "```\n{}\n```", commit.commit_message)?;
        }

        if let Some(pr) = &activity.create_pull_request {
            writeln!(output, "\n**Pull Request:** {}", pr.title)?;
            writeln!(output, "{}", pr.body)?;
        }

        Ok(())
    }
}
