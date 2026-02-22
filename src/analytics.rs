use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{Activity, Session, SessionState};

/// Analyzes a session and its activities to produce a statistical summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    /// Total number of activities.
    pub total_activities: usize,
    /// Breakdown of tool usage frequencies.
    pub tool_usage: HashMap<String, usize>,
    /// Number of files changed (unique paths).
    pub files_changed: usize,
    /// Number of plan steps completed vs total.
    pub plan_progress: (usize, usize),
    /// Interaction count (User Input Requests).
    pub user_interactions: usize,
    /// Final session state.
    pub final_state: Option<SessionState>,
}

/// A statistical analyzer for Jules sessions.
#[derive(Debug, Default, Clone, Copy)]
pub struct SessionAnalyzer;

impl SessionAnalyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyzes a session and its activities to generate a stats report.
    #[must_use]
    pub fn analyze(session: &Session, activities: &[Activity]) -> SessionStats {
        let mut tool_usage = HashMap::new();
        let mut unique_files = HashSet::new();
        let mut user_interactions = 0;

        for activity in activities {
            if let Some(tool) = &activity.tool_use {
                *tool_usage.entry(tool.tool_name.clone()).or_insert(0) += 1;
            }
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    unique_files.insert(file.path.clone());
                }
            }
            if activity.user_input_request.is_some() {
                user_interactions += 1;
            }
        }

        // Check plan progress
        let mut completed_steps = 0;
        let mut total_steps = 0;
        if let Some(plan) = &session.plan {
            total_steps = plan.steps.len();
            completed_steps = plan
                .steps
                .iter()
                .filter(|s| {
                    let status = s.status.to_lowercase();
                    status == "completed" || status == "success" || status == "done"
                })
                .count();
        }

        SessionStats {
            total_activities: activities.len(),
            tool_usage,
            files_changed: unique_files.len(),
            plan_progress: (completed_steps, total_steps),
            user_interactions,
            final_state: session.state,
        }
    }
}

impl Display for SessionStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Session Analysis Report")?;
        writeln!(f, "-----------------------")?;
        writeln!(
            f,
            "State: {:?}",
            self.final_state
                .unwrap_or(SessionState::SessionStateUnspecified)
        )?;
        writeln!(
            f,
            "Plan Progress: {}/{} steps completed",
            self.plan_progress.0, self.plan_progress.1
        )?;
        writeln!(f, "Total Activities: {}", self.total_activities)?;
        writeln!(f, "User Interactions: {}", self.user_interactions)?;
        writeln!(f, "Files Changed: {}", self.files_changed)?;

        if self.tool_usage.is_empty() {
            writeln!(f, "\nNo tools used.")?;
        } else {
            writeln!(f, "\nTool Usage:")?;
            // Sort by usage count descending for nicer output
            let mut tools: Vec<_> = self.tool_usage.iter().collect();
            // Sort by count desc, then name asc
            tools.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

            for (name, count) in tools {
                writeln!(f, "  - {name}: {count}")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangedFile, Plan, PlanStep, SessionOutput, ToolUse, UserInputRequest};

    fn empty_session() -> Session {
        Session {
            name: "sessions/1".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: None,
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: None,
            output: None,
            outputs: vec![],
        }
    }

    #[test]
    fn test_analyze_empty_session() {
        let session = empty_session();
        let activities = vec![];

        let stats = SessionAnalyzer::analyze(&session, &activities);

        assert_eq!(stats.total_activities, 0);
        assert_eq!(stats.files_changed, 0);
        assert_eq!(stats.plan_progress, (0, 0));
        assert!(stats.tool_usage.is_empty());
    }

    #[test]
    fn test_analyze_complex_session() {
        let mut session = empty_session();
        session.state = Some(SessionState::Completed);
        session.plan = Some(Plan {
            steps: vec![
                PlanStep {
                    description: "Step 1".to_string(),
                    status: "completed".to_string(),
                },
                PlanStep {
                    description: "Step 2".to_string(),
                    status: "pending".to_string(),
                },
            ],
        });

        let activities = vec![
            Activity {
                name: "act1".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "read_file".to_string(),
                    input: "{}".to_string(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "act2".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "read_file".to_string(),
                    input: "{}".to_string(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "act3".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: Some(UserInputRequest {
                    question: "?".to_string(),
                    suggested_responses: vec![],
                }),
                tool_use: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "foo.rs".to_string(),
                        diff: String::new(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            },
        ];

        let stats = SessionAnalyzer::analyze(&session, &activities);

        assert_eq!(stats.total_activities, 3);
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.plan_progress, (1, 2));
        assert_eq!(stats.user_interactions, 1);
        assert_eq!(stats.tool_usage.get("read_file"), Some(&2));
        assert_eq!(stats.final_state, Some(SessionState::Completed));
    }
}
