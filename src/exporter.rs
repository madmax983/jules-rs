use std::collections::HashSet;
use std::fmt::Write;

use crate::types::{
    Activity, Commit, CreatePullRequest, ListActivitiesParams, Overview, Session, ToolUse,
    UserInputRequest, ViewDiff,
};
use crate::{JulesClient, JulesError};

const MAX_PAGE_SIZE: u32 = 100;

/// Exports session data to various formats.
#[derive(Debug)]
pub struct SessionExporter<'a> {
    client: &'a JulesClient,
}

impl<'a> SessionExporter<'a> {
    /// Creates a new exporter using the provided client.
    #[must_use]
    pub fn new(client: &'a JulesClient) -> Self {
        Self { client }
    }

    /// Generates a Markdown report for a given session.
    ///
    /// This method fetches the full session details and all associated activities,
    /// then formats them into a readable Markdown document.
    ///
    /// # Errors
    ///
    /// Returns [`JulesError`] if fetching data from the API fails.
    ///
    /// # Panics
    ///
    /// Panics if memory allocation fails during string formatting.
    pub async fn export_markdown(&self, session_id: &str) -> Result<String, JulesError> {
        let session = self.client.get_session(session_id).await?;
        let activities = self.fetch_all_activities(&session.name).await?;

        let mut out = String::new();

        Self::write_header(&mut out, &session).expect("formatting string");
        Self::write_activities(&mut out, &activities).expect("formatting string");

        Ok(out)
    }

    async fn fetch_all_activities(&self, session_id: &str) -> Result<Vec<Activity>, JulesError> {
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

    fn write_header(out: &mut String, session: &Session) -> Result<(), std::fmt::Error> {
        let title = session.title.as_deref().unwrap_or("Untitled Session");
        writeln!(out, "# Session Report: {title}")?;
        writeln!(out, "**ID:** `{}`", session.name)?;

        if let Some(state) = session.state {
            writeln!(out, "**State:** `{state:?}`")?;
        }

        if let Some(create_time) = &session.create_time {
            writeln!(out, "**Created:** {create_time}")?;
        }

        if let Some(prompt) = &session.prompt {
            writeln!(out, "\n## Prompt\n")?;
            writeln!(out, "{prompt}")?;
        }

        Ok(())
    }

    fn write_activities(out: &mut String, activities: &[Activity]) -> Result<(), std::fmt::Error> {
        if activities.is_empty() {
            writeln!(out, "\nNo activities recorded.")?;
            return Ok(());
        }

        writeln!(out, "\n## Activities")?;

        for activity in activities {
            writeln!(out, "\n### Activity: {}", activity.name)?;

            if let Some(status) = activity.status {
                writeln!(out, "**Status:** `{status:?}`")?;
            }

            if let Some(timestamp) = &activity.timestamp {
                writeln!(out, "**Time:** {timestamp}")?;
            }

            if let Some(stage) = &activity.stage_name {
                writeln!(out, "**Stage:** {stage}")?;
            }

            if let Some(detail) = &activity.detail {
                writeln!(out, "\n{detail}")?;
            }

            if let Some(overview) = &activity.overview {
                Self::write_overview(out, overview)?;
            }

            if let Some(tool_use) = &activity.tool_use {
                Self::write_tool_use(out, tool_use)?;
            }

            if let Some(user_input) = &activity.user_input_request {
                Self::write_user_input(out, user_input)?;
            }

            if let Some(view_diff) = &activity.view_diff {
                Self::write_diff(out, view_diff)?;
            }

            if let Some(commit) = &activity.commit {
                Self::write_commit(out, commit)?;
            }

            if let Some(pr) = &activity.create_pull_request {
                Self::write_pr(out, pr)?;
            }

            writeln!(out, "\n---")?;
        }

        Ok(())
    }

    fn write_overview(out: &mut String, overview: &Overview) -> Result<(), std::fmt::Error> {
        if let Some(summary) = &overview.summary {
            writeln!(out, "\n**Summary:** {summary}")?;
        }
        if let Some(thoughts) = &overview.thoughts {
            writeln!(out, "\n**Thoughts:**\n> {}", thoughts.replace('\n', "\n> "))?;
        }
        Ok(())
    }

    fn write_tool_use(out: &mut String, tool_use: &ToolUse) -> Result<(), std::fmt::Error> {
        writeln!(out, "\n**Tool Call:** `{}`", tool_use.tool_name)?;
        writeln!(out, "```json\n{}\n```", tool_use.input)?;
        Ok(())
    }

    fn write_user_input(out: &mut String, req: &UserInputRequest) -> Result<(), std::fmt::Error> {
        writeln!(out, "\n**Question:** {}", req.question)?;
        if !req.suggested_responses.is_empty() {
            writeln!(out, "\n*Suggested Responses:*")?;
            for resp in &req.suggested_responses {
                writeln!(out, "- {}", resp.response)?;
            }
        }
        Ok(())
    }

    fn write_diff(out: &mut String, diff: &ViewDiff) -> Result<(), std::fmt::Error> {
        if let Some(title) = &diff.title {
            writeln!(out, "\n**Diff:** {title}")?;
        } else {
            writeln!(out, "\n**Diff:**")?;
        }
        writeln!(out, "```diff\n{}\n```", diff.diff)?;
        Ok(())
    }

    fn write_commit(out: &mut String, commit: &Commit) -> Result<(), std::fmt::Error> {
        writeln!(out, "\n**Commit Message:**")?;
        writeln!(out, "```\n{}\n```", commit.commit_message)?;
        Ok(())
    }

    fn write_pr(out: &mut String, pr: &CreatePullRequest) -> Result<(), std::fmt::Error> {
        writeln!(out, "\n**Create Pull Request:**")?;
        writeln!(out, "Title: {}", pr.title)?;
        writeln!(out, "\n{}", pr.body)?;
        Ok(())
    }
}
