use std::fmt::Write;

use crate::{
    Activity, ActivityStatus, JulesClient, JulesError, ListActivitiesParams, SessionState,
};

/// Export format for sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Markdown format.
    Markdown,
}

/// Exports session data into human-readable formats.
pub struct SessionExporter<'a> {
    client: &'a JulesClient,
}

impl<'a> SessionExporter<'a> {
    /// Creates a new exporter using the provided client.
    #[must_use]
    pub fn new(client: &'a JulesClient) -> Self {
        Self { client }
    }

    /// Exports a session's history to a Markdown string.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if fetching the session or its activities fails.
    pub async fn export_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;
        let activities = self.fetch_all_activities(session_id).await?;

        let mut report = String::new();

        // Header
        writeln!(
            report,
            "# Session Report: {}\n",
            session.title.as_deref().unwrap_or(session_id)
        )
        .expect("write to string");

        // Metadata
        writeln!(report, "**ID:** `{}`", session.name).expect("write to string");
        if let Some(state) = session.state {
            writeln!(report, "**Status:** `{state:?}`").expect("write to string");
        }
        if let Some(create_time) = &session.create_time {
            writeln!(report, "**Created:** {create_time}").expect("write to string");
        }
        if let Some(update_time) = &session.update_time {
            writeln!(report, "**Updated:** {update_time}").expect("write to string");
        }
        report.push('\n');

        // Initial Prompt
        report.push_str("## Initial Prompt\n\n");
        if let Some(prompt) = &session.prompt {
            // Blockquote the prompt
            writeln!(report, "> {}\n", prompt.replace('\n', "\n> ")).expect("write to string");
        } else {
            report.push_str("_No prompt recorded._\n\n");
        }

        // Activities
        report.push_str("## Activities\n\n");
        if activities.is_empty() {
            report.push_str("_No activities recorded._\n\n");
        } else {
            for activity in activities {
                Self::format_activity(&mut report, &activity);
            }
        }

        // Final Output (if completed)
        if session.state == Some(SessionState::Completed) {
            report.push_str("## Final Output\n\n");
            if let Some(output) = &session.output {
                if let Some(pr) = &output.pull_request {
                    writeln!(
                        report,
                        "**Pull Request:** [{}]({})",
                        pr.title.as_deref().unwrap_or("PR"),
                        pr.url
                    )
                    .expect("write to string");
                }
                if !output.changed_files.is_empty() {
                    report.push_str("**Changed Files:**\n");
                    for file in &output.changed_files {
                        writeln!(report, "- `{}`", file.path).expect("write to string");
                    }
                }
            }
        }

        Ok(report)
    }

    async fn fetch_all_activities(&self, session_id: &str) -> Result<Vec<Activity>, JulesError> {
        let mut activities = Vec::new();
        let mut page_token = None;

        loop {
            let response = self
                .client
                .list_activities(
                    session_id,
                    ListActivitiesParams {
                        page_size: Some(100),
                        page_token: page_token.clone(),
                        ..Default::default()
                    },
                )
                .await?;

            activities.extend(response.activities);

            match response.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        Ok(activities)
    }

    fn format_activity(report: &mut String, activity: &Activity) {
        // Activity Header
        let title = activity.activity_type.as_deref().unwrap_or("Activity");
        writeln!(report, "### {title}").expect("write to string");

        writeln!(report, "**ID:** `{}`", activity.name).expect("write to string");
        if let Some(timestamp) = &activity.timestamp {
            writeln!(report, "**Time:** {timestamp}").expect("write to string");
        }
        if let Some(status) = activity.status {
            let status_icon = match status {
                ActivityStatus::Success => "✅",
                ActivityStatus::Failed => "❌",
                ActivityStatus::InProgress => "🔄",
                _ => "❓",
            };
            writeln!(report, "**Status:** {status_icon} `{status:?}`").expect("write to string");
        }
        report.push('\n');

        // Detail
        if let Some(detail) = &activity.detail {
            writeln!(report, "{detail}\n").expect("write to string");
        }

        // Thoughts
        if let Some(overview) = &activity.overview {
            if let Some(thoughts) = &overview.thoughts {
                report.push_str("**Thoughts:**\n\n");
                writeln!(report, "{thoughts}\n").expect("write to string");
            }
        }

        // Tool Use
        if let Some(tool_use) = &activity.tool_use {
            writeln!(report, "**Tool Use:** `{}`", tool_use.tool_name).expect("write to string");
            report.push_str("```json\n");
            report.push_str(&tool_use.input);
            report.push_str("\n```\n\n");
        }

        // User Input
        if let Some(req) = &activity.user_input_request {
            report.push_str("**User Input Requested:**\n");
            writeln!(report, "> {}\n", req.question).expect("write to string");
        }

        // Diff
        if let Some(diff_view) = &activity.view_diff {
            if let Some(title) = &diff_view.title {
                writeln!(report, "**Diff:** {title}").expect("write to string");
            } else {
                report.push_str("**Diff:**\n");
            }
            report.push_str("```diff\n");
            report.push_str(&diff_view.diff);
            report.push_str("\n```\n\n");
        }

        // Create PR
        if let Some(pr) = &activity.create_pull_request {
            report.push_str("**Created Pull Request:**\n");
            writeln!(report, "Title: {}", pr.title).expect("write to string");
            writeln!(report, "Body:\n{}\n", pr.body).expect("write to string");
        }

        report.push_str("---\n\n");
    }
}
