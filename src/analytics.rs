use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::types::{Activity, Session, SessionState};

/// Statistical summary of a session's execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    /// Total number of activities recorded.
    pub total_activities: usize,
    /// Count of tool invocations by tool name.
    pub tool_usage: HashMap<String, usize>,
    /// Set of unique file paths modified.
    pub files_changed: HashSet<String>,
    /// Number of plan steps completed vs total steps.
    pub plan_completion: (usize, usize),
    /// Total characters in "thoughts" (rough proxy for reasoning depth).
    pub thoughts_volume: usize,
    /// Final session state.
    pub final_state: Option<SessionState>,
}

impl Display for SessionStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Session Stats:")?;
        writeln!(f, "  Total Activities: {}", self.total_activities)?;
        writeln!(
            f,
            "  Final State: {:?}",
            self.final_state
                .unwrap_or(SessionState::SessionStateUnspecified)
        )?;
        writeln!(
            f,
            "  Plan Progress: {}/{} steps",
            self.plan_completion.0, self.plan_completion.1
        )?;
        writeln!(f, "  Files Changed: {}", self.files_changed.len())?;
        if !self.files_changed.is_empty() {
            let mut sorted_files: Vec<_> = self.files_changed.iter().collect();
            sorted_files.sort();
            for file in sorted_files {
                writeln!(f, "    - {file}")?;
            }
        }
        writeln!(f, "  Tool Usage:")?;
        if self.tool_usage.is_empty() {
            writeln!(f, "    (None)")?;
        } else {
            let mut sorted_tools: Vec<_> = self.tool_usage.iter().collect();
            sorted_tools.sort_by_key(|(name, _)| *name);
            for (tool, count) in sorted_tools {
                writeln!(f, "    - {tool}: {count}")?;
            }
        }
        writeln!(f, "  Thoughts Volume: {} chars", self.thoughts_volume)?;
        Ok(())
    }
}

/// Analyzer for extracting insights from session data.
#[derive(Debug, Default)]
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
        let total_activities = activities.len();
        let mut tool_usage = HashMap::new();
        let mut files_changed = HashSet::new();
        let mut thoughts_volume = 0;

        for activity in activities {
            // Count tool usage
            if let Some(tool) = &activity.tool_use {
                *tool_usage.entry(tool.tool_name.clone()).or_insert(0) += 1;
            }

            // Track changed files
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    files_changed.insert(file.path.clone());
                }
            }

            // Sum thoughts volume
            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    thoughts_volume += thoughts.len();
                }
            }
        }

        // Calculate plan completion
        let (completed_steps, total_steps) = if let Some(plan) = &session.plan {
            let total = plan.steps.len();
            let completed = plan
                .steps
                .iter()
                .filter(|step| {
                    matches!(
                        step.status.to_lowercase().as_str(),
                        "completed" | "done" | "success"
                    )
                })
                .count();
            (completed, total)
        } else {
            (0, 0)
        };

        SessionStats {
            total_activities,
            tool_usage,
            files_changed,
            plan_completion: (completed_steps, total_steps),
            thoughts_volume,
            final_state: session.state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChangedFile, Overview, Plan, PlanStep, SessionOutput, ToolUse};

    #[test]
    fn test_empty_session_stats() {
        let session = Session {
            name: "sessions/1".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: Some(SessionState::Queued),
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

        assert_eq!(stats.total_activities, 0);
        assert!(stats.tool_usage.is_empty());
        assert!(stats.files_changed.is_empty());
        assert_eq!(stats.plan_completion, (0, 0));
        assert_eq!(stats.thoughts_volume, 0);
        assert_eq!(stats.final_state, Some(SessionState::Queued));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_complex_session_stats() {
        let session = Session {
            name: "sessions/2".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: Some(SessionState::Completed),
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
                        description: "Step 1".to_string(),
                        status: "completed".to_string(),
                    },
                    PlanStep {
                        description: "Step 2".to_string(),
                        status: "pending".to_string(),
                    },
                ],
            }),
            output: None,
            outputs: vec![],
        };

        let activities = vec![
            Activity {
                name: "act/1".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: Some(Overview {
                    state: None,
                    thoughts: Some("Thinking about code".to_string()), // 19 chars
                    summary: None,
                }),
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
                name: "act/2".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "write_file".to_string(),
                    input: "{}".to_string(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "src/main.rs".to_string(),
                        diff: String::new(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            },
            Activity {
                name: "act/3".to_string(),
                status: None,
                stage_name: None,
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: Some(Overview {
                    state: None,
                    thoughts: Some("Done".to_string()), // 4 chars
                    summary: None,
                }),
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "read_file".to_string(), // 2nd use
                    input: "{}".to_string(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let analyzer = SessionAnalyzer::new();
        let stats = analyzer.analyze(&session, &activities);

        assert_eq!(stats.total_activities, 3);
        assert_eq!(stats.tool_usage.get("read_file"), Some(&2));
        assert_eq!(stats.tool_usage.get("write_file"), Some(&1));
        assert!(stats.files_changed.contains("src/main.rs"));
        assert_eq!(stats.files_changed.len(), 1);
        assert_eq!(stats.plan_completion, (1, 2));
        assert_eq!(stats.thoughts_volume, 23); // 19 + 4
        assert_eq!(stats.final_state, Some(SessionState::Completed));

        let display = format!("{stats}");
        assert!(display.contains("Total Activities: 3"));
        assert!(display.contains("read_file: 2"));
        assert!(display.contains("write_file: 1"));
        assert!(display.contains("Thoughts Volume: 23 chars"));
    }
}
