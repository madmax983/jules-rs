use crate::{Activity, JulesClient, JulesError, ListActivitiesParams, Session, SessionState};
use std::fmt::Write;

/// Fetches session data and formats it into a Markdown report.
pub struct SessionExporter {
    client: JulesClient,
}

impl SessionExporter {
    /// Creates a new exporter using the provided client.
    #[must_use]
    pub fn new(client: JulesClient) -> Self {
        Self { client }
    }

    /// Generates a Markdown report for the given session ID.
    ///
    /// The report includes:
    /// - Session metadata (ID, title, state, prompt)
    /// - Chronological list of activities
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if session or activity data cannot be fetched.
    pub async fn export_to_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;

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

        // API returns reverse chronological (newest first).
        // We want chronological (oldest first) for a transcript.
        activities.reverse();

        Ok(Self::format_report(&session, &activities))
    }

    fn format_report(session: &Session, activities: &[Activity]) -> String {
        let mut out = String::new();

        writeln!(
            out,
            "# Session Report: {}",
            session.title.as_deref().unwrap_or("Untitled")
        )
        .unwrap();
        writeln!(out, "**Session ID:** `{}`", session.name).unwrap();
        writeln!(
            out,
            "**State:** {:?}",
            session
                .state
                .unwrap_or(SessionState::SessionStateUnspecified)
        )
        .unwrap();
        writeln!(
            out,
            "**Created:** {}",
            session.create_time.as_deref().unwrap_or("Unknown")
        )
        .unwrap();
        writeln!(out).unwrap();

        writeln!(out, "## Prompt").unwrap();
        writeln!(
            out,
            "{}",
            session.prompt.as_deref().unwrap_or("*No prompt provided*")
        )
        .unwrap();
        writeln!(out).unwrap();

        writeln!(out, "## Activity Log").unwrap();

        if activities.is_empty() {
            writeln!(out, "*No activities found.*").unwrap();
        }

        for activity in activities {
            Self::format_activity(&mut out, activity);
            writeln!(out, "---").unwrap();
        }

        out
    }

    fn format_activity(out: &mut String, activity: &Activity) {
        writeln!(
            out,
            "### {}",
            activity.activity_type.as_deref().unwrap_or("Activity")
        )
        .unwrap();
        if let Some(ts) = &activity.timestamp {
            writeln!(out, "**Time:** {ts}").unwrap();
        }
        if let Some(status) = activity.status {
            writeln!(out, "**Status:** {status:?}").unwrap();
        }
        writeln!(out).unwrap();

        if let Some(detail) = &activity.detail {
            writeln!(out, "{detail}").unwrap();
            writeln!(out).unwrap();
        }

        if let Some(overview) = &activity.overview {
            if let Some(thoughts) = &overview.thoughts {
                writeln!(out, "**Thoughts:**").unwrap();
                writeln!(out, "```text").unwrap();
                writeln!(out, "{thoughts}").unwrap();
                writeln!(out, "```").unwrap();
            }
        }

        if let Some(tool_use) = &activity.tool_use {
            writeln!(out, "**Tool Call:** `{}`", tool_use.tool_name).unwrap();
            writeln!(out, "```json").unwrap();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&tool_use.input) {
                if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                    writeln!(out, "{pretty}").unwrap();
                } else {
                    writeln!(out, "{}", tool_use.input).unwrap();
                }
            } else {
                writeln!(out, "{}", tool_use.input).unwrap();
            }
            writeln!(out, "```").unwrap();
        }

        if let Some(output) = &activity.output {
            if !output.changed_files.is_empty() {
                writeln!(out, "**Changed Files:**").unwrap();
                for file in &output.changed_files {
                    writeln!(out, "- `{}`", file.path).unwrap();
                }
            }
        }
    }
}
