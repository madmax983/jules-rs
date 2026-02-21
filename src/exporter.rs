use std::fmt::Write;

use crate::{Activity, JulesClient, JulesError, ListActivitiesParams, Session};

/// Exports session data to various formats.
pub struct SessionExporter<'a> {
    client: &'a JulesClient,
}

impl<'a> SessionExporter<'a> {
    /// Creates a new exporter using the provided client.
    #[must_use]
    pub fn new(client: &'a JulesClient) -> Self {
        Self { client }
    }

    /// Fetches a session and its activities, then formats them as a Markdown report.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if the session or activities cannot be fetched.
    pub async fn export_to_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;
        let activities = self.get_all_activities(session_id).await?;

        let mut out = String::new();

        Self::format_header(&mut out, &session);
        Self::format_plan(&mut out, &session);
        Self::format_activities(&mut out, &activities);

        Ok(out)
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

            match response.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        Ok(activities)
    }

    fn format_header(out: &mut String, session: &Session) {
        let title = session.title.as_deref().unwrap_or("Untitled Session");
        let _ = writeln!(out, "# {title}");
        let _ = writeln!(out, "**ID:** `{}`", session.name);
        if let Some(state) = session.state {
            let _ = writeln!(out, "**Status:** {state:?}");
        }
        if let Some(url) = &session.url {
            let _ = writeln!(out, "**URL:** <{url}>");
        }
        if let Some(prompt) = &session.prompt {
            let _ = writeln!(out, "\n## Prompt\n\n{prompt}");
        }
    }

    fn format_plan(out: &mut String, session: &Session) {
        if let Some(plan) = &session.plan {
            let _ = writeln!(out, "\n## Plan");
            if plan.steps.is_empty() {
                let _ = writeln!(out, "_No plan steps._");
            } else {
                for step in &plan.steps {
                    // Simple heuristic for checkbox state based on status text
                    let checked = matches!(
                        step.status.to_lowercase().as_str(),
                        "completed" | "done" | "success"
                    );
                    let check_mark = if checked { "x" } else { " " };
                    let _ = writeln!(
                        out,
                        "- [{check_mark}] {} ({})",
                        step.description, step.status
                    );
                }
            }
        }
    }

    fn format_activities(out: &mut String, activities: &[Activity]) {
        let _ = writeln!(out, "\n## Activity Log");
        if activities.is_empty() {
            let _ = writeln!(out, "_No activities recorded._");
            return;
        }

        for activity in activities {
            let _ = writeln!(out, "\n### {}", activity.name);

            if let Some(ts) = &activity.timestamp {
                let _ = writeln!(out, "**Time:** {ts}");
            }
            if let Some(status) = activity.status {
                let _ = writeln!(out, "**Status:** {status:?}");
            }
            if let Some(detail) = &activity.detail {
                let _ = writeln!(out, "\n_{detail}_");
            }

            // Overview / Thoughts
            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    let _ = writeln!(out, "\n**Thoughts:**\n> {}", thoughts.replace('\n', "\n> "));
                }
                if let Some(summary) = &overview.summary {
                    let _ = writeln!(out, "\n**Summary:** {summary}");
                }
            }

            // Tool Usage
            if let Some(tool) = &activity.tool_use {
                let _ = writeln!(out, "\n**Tool Use:** `{}`", tool.tool_name);
                let _ = writeln!(out, "```json\n{}\n```", tool.input);
            }

            // Diffs
            if let Some(diff) = &activity.view_diff {
                let _ = writeln!(out, "\n**Diff View:**");
                let _ = writeln!(out, "```diff\n{}\n```", diff.diff);
            }

            // Commits
            if let Some(commit) = &activity.commit {
                let _ = writeln!(out, "\n**Commit:**\n```\n{}\n```", commit.commit_message);
            }

            // PRs
            if let Some(pr) = &activity.create_pull_request {
                let _ = writeln!(out, "\n**Create PR:** {}", pr.title);
                let _ = writeln!(out, "_{}_", pr.body);
            }

            // User Input Requests
            if let Some(req) = &activity.user_input_request {
                let _ = writeln!(out, "\n**Question:** {}", req.question);
            }

            // Output / Changed Files
            if let Some(output) = &activity.output {
                if !output.changed_files.is_empty() {
                    let _ = writeln!(out, "\n**Changed Files:**");
                    for file in &output.changed_files {
                        let _ = writeln!(out, "- `{}`", file.path);
                    }
                }
            }
        }
    }
}
