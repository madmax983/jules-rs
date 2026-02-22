use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use crate::{Activity, Session, SessionState};

/// Summarized report of a session's execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// Session ID (resource name).
    pub session_id: String,
    /// Session title or "Untitled".
    pub title: String,
    /// Final session state.
    pub state: Option<SessionState>,
    /// Total number of steps in the plan.
    pub total_plan_steps: usize,
    /// Number of completed plan steps.
    pub completed_plan_steps: usize,
    /// Count of tool usage by tool name.
    pub tool_usage: HashMap<String, usize>,
    /// Count of activities by status (e.g., "Success", "Failed").
    pub activity_status_counts: HashMap<String, usize>,
    /// Count of modifications per file path.
    pub changed_files: HashMap<String, usize>,
}

/// Analyzer for generating insights from session data.
#[derive(Debug, Default, Clone, Copy)]
pub struct SessionAnalyzer;

impl SessionAnalyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyzes a session and its activities to produce a report.
    #[must_use]
    pub fn analyze(&self, session: &Session, activities: &[Activity]) -> SessionReport {
        let mut report = SessionReport {
            session_id: session.name.clone(),
            title: session.title.clone().unwrap_or_else(|| "Untitled".to_string()),
            state: session.state,
            total_plan_steps: 0,
            completed_plan_steps: 0,
            tool_usage: HashMap::new(),
            activity_status_counts: HashMap::new(),
            changed_files: HashMap::new(),
        };

        // Analyze Plan
        if let Some(plan) = &session.plan {
            report.total_plan_steps = plan.steps.len();
            for step in &plan.steps {
                if matches!(
                    step.status.to_lowercase().as_str(),
                    "completed" | "done" | "success"
                ) {
                    report.completed_plan_steps += 1;
                }
            }
        }

        // Analyze Activities
        for activity in activities {
            // Status counts
            if let Some(status) = activity.status {
                let status_str = format!("{status:?}");
                *report.activity_status_counts.entry(status_str).or_insert(0) += 1;
            }

            // Tool usage
            if let Some(tool) = &activity.tool_use {
                *report.tool_usage.entry(tool.tool_name.clone()).or_insert(0) += 1;
            }

            // Changed files
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    *report.changed_files.entry(file.path.clone()).or_insert(0) += 1;
                }
            }
        }

        report
    }
}

impl Display for SessionReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Session Report: {} ({})",
            self.title, self.session_id
        )?;
        if let Some(state) = self.state {
            writeln!(f, "State: {state:?}")?;
        }
        writeln!(f, "{:-<50}", "")?;

        // Plan Progress
        let pct = if self.total_plan_steps > 0 {
            (self.completed_plan_steps as f64 / self.total_plan_steps as f64) * 100.0
        } else {
            0.0
        };
        let bars = (pct / 10.0).round() as usize;
        let progress_bar = format!("[{:<10}]", "#".repeat(bars));
        writeln!(
            f,
            "Plan Progress: {} {:.0}% ({}/{} Steps)",
            progress_bar, pct, self.completed_plan_steps, self.total_plan_steps
        )?;
        writeln!(f, "{:-<50}", "")?;

        // Activity Summary
        let total_activities: usize = self.activity_status_counts.values().sum();
        let failed_activities = *self.activity_status_counts.get("Failed").unwrap_or(&0);
        writeln!(f, "Activity Summary:")?;
        writeln!(f, "  Total:  {}", total_activities)?;
        writeln!(f, "  Failed: {}", failed_activities)?;
        writeln!(f, "{:-<50}", "")?;

        // Tool Usage
        writeln!(f, "Tool Usage:")?;
        if self.tool_usage.is_empty() {
            writeln!(f, "  (No tools used)")?;
        } else {
            // Sort by count descending
            let mut tools: Vec<_> = self.tool_usage.iter().collect();
            tools.sort_by(|a, b| b.1.cmp(a.1));

            for (name, count) in tools {
                let bar = "|".repeat(*count);
                writeln!(f, "  {:<15}: {} ({})", name, bar, count)?;
            }
        }
        writeln!(f, "{:-<50}", "")?;

        // Hotspot Files
        writeln!(f, "Hotspot Files:")?;
        if self.changed_files.is_empty() {
            writeln!(f, "  (No files changed)")?;
        } else {
            // Sort by count descending
            let mut files: Vec<_> = self.changed_files.iter().collect();
            files.sort_by(|a, b| b.1.cmp(a.1));

            for (path, count) in files {
                writeln!(f, "  {:<20}: {} edits", path, count)?;
            }
        }
        writeln!(f, "{:-<50}", "")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, Plan, PlanStep, ToolUse, SessionOutput, ChangedFile};

    // Helper to create a minimal session
    fn make_session() -> Session {
        Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("Bug Fix Session".to_string()),
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
        }
    }

    // Helper to create a minimal activity
    fn make_activity(name: &str, tool: Option<&str>, status: Option<ActivityStatus>) -> Activity {
        Activity {
            name: name.to_string(),
            status,
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: tool.map(|t| ToolUse {
                tool_name: t.to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    fn test_analyze_session() {
        let session = make_session();
        let mut activities = vec![
            make_activity("act-1", Some("read_file"), Some(ActivityStatus::Success)),
            make_activity("act-2", Some("read_file"), Some(ActivityStatus::Success)),
            make_activity("act-3", Some("write_file"), Some(ActivityStatus::Success)),
            make_activity("act-4", None, Some(ActivityStatus::Failed)), // Non-tool activity
        ];

        // Add file changes to act-3
        activities[2].output = Some(SessionOutput {
            changed_files: vec![
                ChangedFile { path: "src/main.rs".to_string(), diff: "".to_string() }
            ],
            commit_hash: None,
            pull_request: None,
        });

        let analyzer = SessionAnalyzer::new();
        let report = analyzer.analyze(&session, &activities);

        assert_eq!(report.session_id, "sessions/test-1");
        assert_eq!(report.title, "Bug Fix Session");
        assert_eq!(report.state, Some(SessionState::Completed));

        // Plan stats
        assert_eq!(report.total_plan_steps, 2);
        assert_eq!(report.completed_plan_steps, 1); // "completed"

        // Tool stats
        assert_eq!(report.tool_usage.get("read_file"), Some(&2));
        assert_eq!(report.tool_usage.get("write_file"), Some(&1));
        assert_eq!(report.tool_usage.get("unknown_tool"), None);

        // Status stats
        assert_eq!(report.activity_status_counts.get("Success"), Some(&3));
        assert_eq!(report.activity_status_counts.get("Failed"), Some(&1));

        // File stats
        assert_eq!(report.changed_files.get("src/main.rs"), Some(&1));

        // Test Display output
        let output = format!("{}", report);
        println!("{}", output);
        assert!(output.contains("Plan Progress: [#####     ] 50%"));
        assert!(output.contains("read_file      : || (2)"));
        assert!(output.contains("src/main.rs         : 1 edits"));
    }
}
