use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{Activity, Session};

/// Statistical analysis of a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    /// Total number of steps in the plan.
    pub total_steps: usize,
    /// Number of completed steps.
    pub completed_steps: usize,
    /// Number of pending steps.
    pub pending_steps: usize,
    /// Frequency of tool usage.
    pub tool_usage: HashMap<String, usize>,
    /// Total number of files changed across all activities.
    pub files_changed: usize,
    /// Total length of thoughts (characters).
    pub thought_volume: usize,
}

impl Display for SessionStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Session Stats:")?;
        writeln!(
            f,
            "  Plan Progress: {}/{} steps ({} pending)",
            self.completed_steps, self.total_steps, self.pending_steps
        )?;
        writeln!(f, "  Files Changed: {}", self.files_changed)?;
        writeln!(f, "  Thought Volume: {} chars", self.thought_volume)?;
        writeln!(f, "  Tool Usage:")?;
        if self.tool_usage.is_empty() {
            writeln!(f, "    (None)")?;
        } else {
            // Sort tools for consistent output
            let mut tools: Vec<_> = self.tool_usage.iter().collect();
            tools.sort_by_key(|(name, _)| *name);
            for (tool, count) in tools {
                writeln!(f, "    {}: {}", tool, count)?;
            }
        }
        Ok(())
    }
}

/// Analyzer for session data.
#[derive(Debug, Default, Clone, Copy)]
pub struct SessionAnalyzer;

impl SessionAnalyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyzes a session and its activities to produce statistics.
    #[must_use]
    pub fn analyze(&self, session: &Session, activities: &[Activity]) -> SessionStats {
        let (total_steps, completed_steps, pending_steps) = if let Some(plan) = &session.plan {
            let total = plan.steps.len();
            let completed = plan
                .steps
                .iter()
                .filter(|s| is_completed(&s.status))
                .count();
            let pending = total - completed;
            (total, completed, pending)
        } else {
            (0, 0, 0)
        };

        let mut tool_usage = HashMap::new();
        let mut files_changed = 0;
        let mut thought_volume = 0;

        for activity in activities {
            if let Some(tool) = &activity.tool_use {
                *tool_usage.entry(tool.tool_name.clone()).or_insert(0) += 1;
            }

            if let Some(output) = &activity.output {
                files_changed += output.changed_files.len();
            }

            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    thought_volume += thoughts.len();
                }
            }
        }

        SessionStats {
            total_steps,
            completed_steps,
            pending_steps,
            tool_usage,
            files_changed,
            thought_volume,
        }
    }
}

fn is_completed(status: &str) -> bool {
    matches!(
        status.to_lowercase().as_str(),
        "completed" | "done" | "success"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangedFile, Overview, Plan, PlanStep, SessionOutput, ToolUse};

    #[test]
    fn test_analyze_empty() {
        let session = Session {
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
        };
        let activities = vec![];

        let analyzer = SessionAnalyzer::new();
        let stats = analyzer.analyze(&session, &activities);

        assert_eq!(stats.total_steps, 0);
        assert_eq!(stats.files_changed, 0);
        assert_eq!(stats.thought_volume, 0);
        assert!(stats.tool_usage.is_empty());
    }

    #[test]
    fn test_analyze_populated() {
        let session = Session {
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
            plan: Some(Plan {
                steps: vec![
                    PlanStep {
                        description: "1".into(),
                        status: "completed".into(),
                    },
                    PlanStep {
                        description: "2".into(),
                        status: "pending".into(),
                    },
                ],
            }),
            output: None,
            outputs: vec![],
        };

        let activities = vec![
            Activity {
                name: "a1".into(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: Some(Overview {
                    state: None,
                    thoughts: Some("thinking...".into()),
                    summary: None,
                }),
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "read_file".into(),
                    input: "{}".into(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "foo.rs".into(),
                        diff: "".into(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            },
            Activity {
                name: "a2".into(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "read_file".into(),
                    input: "{}".into(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let analyzer = SessionAnalyzer::new();
        let stats = analyzer.analyze(&session, &activities);

        assert_eq!(stats.total_steps, 2);
        assert_eq!(stats.completed_steps, 1);
        assert_eq!(stats.pending_steps, 1);
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.thought_volume, 11); // "thinking..." length
        assert_eq!(stats.tool_usage.get("read_file"), Some(&2));
    }
}
