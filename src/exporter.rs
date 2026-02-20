use crate::client::JulesClient;
use crate::error::JulesError;
use crate::types::{Activity, ListActivitiesParams, Session};
use std::fmt::Write;

/// Exports session data to various formats.
pub struct SessionExporter<'a> {
    client: &'a JulesClient,
}

impl<'a> SessionExporter<'a> {
    /// Creates a new session exporter.
    #[must_use]
    pub fn new(client: &'a JulesClient) -> Self {
        Self { client }
    }

    /// Exports the session and its activities to a Markdown string.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if fetching session details or activities fails.
    pub async fn export_to_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;
        let activities = self.get_all_activities(session_id).await?;

        let mut output = String::new();

        write_header(&mut output, &session);
        write_plan(&mut output, &session);
        write_activities(&mut output, &activities);

        Ok(output)
    }

    async fn get_all_activities(&self, session_id: &str) -> Result<Vec<Activity>, JulesError> {
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

            match response.next_page_token.filter(|t| !t.is_empty()) {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        Ok(activities)
    }
}

fn write_header(w: &mut String, session: &Session) {
    let title = session.title.as_deref().unwrap_or("Untitled Session");
    // Extract simple ID from name if ID field is missing
    let id = session
        .id
        .as_deref()
        .unwrap_or_else(|| session.name.split('/').next_back().unwrap_or(&session.name));

    let state = session
        .state
        .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));

    let _ = writeln!(w, "# Session: {title}");
    let _ = writeln!(w);
    let _ = writeln!(w, "**ID**: {id}");
    let _ = writeln!(w, "**Status**: {state}");
    if let Some(prompt) = &session.prompt {
        let _ = writeln!(w, "**Prompt**: {prompt}");
    }
    if let Some(url) = &session.url {
        let _ = writeln!(w, "**URL**: {url}");
    }
    let _ = writeln!(w);
}

fn write_plan(w: &mut String, session: &Session) {
    if let Some(plan) = &session.plan {
        let _ = writeln!(w, "## Plan");
        for step in &plan.steps {
            let status_mark = if step.status == "COMPLETED" { "x" } else { " " };
            let _ = writeln!(w, "- [{status_mark}] {}", step.description);
        }
        let _ = writeln!(w);
    }
}

fn write_activities(w: &mut String, activities: &[Activity]) {
    if activities.is_empty() {
        return;
    }

    let _ = writeln!(w, "## Timeline");
    let _ = writeln!(w);

    for activity in activities {
        let id = activity
            .name
            .split('/')
            .next_back()
            .unwrap_or(&activity.name);
        let _ = writeln!(w, "### Activity: {id}");

        if let Some(ts) = &activity.timestamp {
            let _ = writeln!(w, "_{ts}_");
        }

        if let Some(status) = &activity.status {
            let _ = writeln!(w, "**Status**: {status:?}");
        }

        if let Some(overview) = &activity.overview {
            if let Some(thoughts) = &overview.thoughts {
                let _ = writeln!(w, "**Thoughts**: {thoughts}");
            }
            if let Some(summary) = &overview.summary {
                let _ = writeln!(w, "**Summary**: {summary}");
            }
        }

        if let Some(tool) = &activity.tool_use {
            let _ = writeln!(w, "**Tool Use**: `{}`", tool.tool_name);
            let _ = writeln!(w, "```json\n{}\n```", tool.input);
        }

        if let Some(diff) = &activity.view_diff {
            let _ = writeln!(w, "**Diff**:");
            let _ = writeln!(w, "```diff\n{}\n```", diff.diff);
        }

        if let Some(commit) = &activity.commit {
            let _ = writeln!(w, "**Commit**: {}", commit.commit_message);
        }

        let _ = writeln!(w);
    }
}
