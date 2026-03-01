use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use crate::{Activity, Session};

/// Analyzes session data to produce summary metrics and insights.
#[derive(Debug, Default)]
pub struct SessionAnalyzer;

use serde::{Deserialize, Serialize};

/// Report containing aggregated metrics and insights from a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReport {
    /// Total number of activities.
    pub total_activities: usize,
    /// Count of activities by status.
    pub activity_status_counts: HashMap<String, usize>,
    /// Count of tool usage by tool name.
    pub tool_usage_counts: HashMap<String, usize>,
    /// Number of plan steps completed.
    pub completed_steps: usize,
    /// Total number of plan steps.
    pub total_steps: usize,
    /// List of files modified (unique paths).
    pub modified_files: Vec<String>,
}

impl SessionAnalyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyzes the provided session and activities to generate a report.
    #[must_use]
    pub fn analyze(&self, session: &Session, activities: &[Activity]) -> SessionReport {
        let mut activity_status_counts = HashMap::new();
        let mut tool_usage_counts = HashMap::new();
        let mut modified_files_set = HashSet::new();

        for activity in activities {
            // Count status
            if let Some(status) = activity.status {
                let status_key = format!("{status:?}");
                *activity_status_counts.entry(status_key).or_insert(0) += 1;
            }

            // Count tool usage
            if let Some(tool) = &activity.tool_use {
                *tool_usage_counts.entry(tool.tool_name.clone()).or_insert(0) += 1;
            }

            // Collect modified files
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    modified_files_set.insert(file.path.clone());
                }
            }
        }

        let mut completed_steps = 0;
        let mut total_steps = 0;

        if let Some(plan) = &session.plan {
            total_steps = plan.steps.len();
            for step in &plan.steps {
                if matches!(
                    step.status.to_lowercase().as_str(),
                    "completed" | "done" | "success"
                ) {
                    completed_steps += 1;
                }
            }
        }

        let mut modified_files: Vec<String> = modified_files_set.into_iter().collect();
        modified_files.sort();

        SessionReport {
            total_activities: activities.len(),
            activity_status_counts,
            tool_usage_counts,
            completed_steps,
            total_steps,
            modified_files,
        }
    }
}

impl Display for SessionReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Session Analytics Report")?;
        writeln!(f, "========================")?;

        // Plan Progress
        #[allow(clippy::cast_precision_loss)]
        let progress_pct = if self.total_steps > 0 {
            (self.completed_steps as f64 / self.total_steps as f64) * 100.0
        } else {
            0.0
        };
        writeln!(
            f,
            "Plan Progress:   {}/{} Steps ({:.1}%)",
            self.completed_steps, self.total_steps, progress_pct
        )?;

        // Activity Stats
        writeln!(f, "\nActivity Summary")?;
        writeln!(f, "----------------")?;
        writeln!(f, "Total Activities: {}", self.total_activities)?;
        for (status, count) in &self.activity_status_counts {
            writeln!(f, "  {status:<15}: {count}")?;
        }

        // Tool Usage
        writeln!(f, "\nTool Usage")?;
        writeln!(f, "----------")?;
        if self.tool_usage_counts.is_empty() {
            writeln!(f, "  (No tools used)")?;
        } else {
            // Sort by count desc
            let mut tools: Vec<_> = self.tool_usage_counts.iter().collect();
            tools.sort_by(|a, b| b.1.cmp(a.1));

            for (tool, count) in tools {
                // ASCII Bar
                let bar_len = (*count).min(20); // Cap at 20 chars
                let bar = "|".repeat(bar_len);
                writeln!(f, "  {tool:<20}: {bar} ({count})")?;
            }
        }

        // Modified Files
        writeln!(f, "\nModified Files")?;
        writeln!(f, "--------------")?;
        if self.modified_files.is_empty() {
            writeln!(f, "  (No files modified)")?;
        } else {
            for file in &self.modified_files {
                writeln!(f, "  - {file}")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActivityStatus, ChangedFile, Plan, PlanStep, SessionOutput, SessionState, ToolUse,
    };

    fn create_mock_activity(
        name: &str,
        status: ActivityStatus,
        tool: Option<(&str, &str)>,
        output: Option<SessionOutput>,
    ) -> Activity {
        Activity {
            name: name.to_string(),
            status: Some(status),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: tool.map(|(name, input)| ToolUse {
                tool_name: name.to_string(),
                input: input.to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output,
        }
    }

    #[test]
    fn test_analyze_session() {
        let session = Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("Bug fix session".to_string()),
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
            create_mock_activity(
                "act-1",
                ActivityStatus::Success,
                Some(("read_file", "{}")),
                None,
            ),
            create_mock_activity(
                "act-2",
                ActivityStatus::Failed,
                Some(("run_test", "{}")),
                None,
            ),
            create_mock_activity(
                "act-3",
                ActivityStatus::Success,
                Some(("read_file", "{}")),
                Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "src/main.rs".to_string(),
                        diff: String::new(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            ),
        ];

        let analyzer = SessionAnalyzer::new();
        let report = analyzer.analyze(&session, &activities);

        assert_eq!(report.total_activities, 3);
        assert_eq!(report.total_steps, 2);
        assert_eq!(report.completed_steps, 1);

        assert_eq!(report.tool_usage_counts.get("read_file"), Some(&2));
        assert_eq!(report.tool_usage_counts.get("run_test"), Some(&1));

        assert_eq!(report.activity_status_counts.get("Success"), Some(&2));
        assert_eq!(report.activity_status_counts.get("Failed"), Some(&1));

        assert!(report.modified_files.contains(&"src/main.rs".to_string()));
        assert_eq!(report.modified_files.len(), 1);
    }
}
