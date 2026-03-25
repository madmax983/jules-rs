use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::{Activity, Session};

/// Represents the estimated effort and cost of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortReport {
    /// The calculated total effort score.
    pub total_score: u32,
    /// The number of times tools were used.
    pub tool_call_count: u32,
    /// The number of files modified across the session outputs.
    pub files_modified: u32,
}

impl Display for EffortReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Effort Estimate")?;
        writeln!(f, "=======================")?;
        writeln!(f, "Total Score:    {}", self.total_score)?;
        writeln!(f, "Tool Calls:     {}", self.tool_call_count)?;
        writeln!(f, "Files Modified: {}", self.files_modified)?;
        Ok(())
    }
}

/// Analyzes a session and its activities to estimate effort/cost.
#[derive(Default)]
pub struct SessionEstimator {}

impl SessionEstimator {
    /// Creates a new `SessionEstimator`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Calculates the effort report for a given session and its activities.
    #[must_use]
    pub fn estimate(&self, session: &Session, activities: &[Activity]) -> EffortReport {
        let mut tool_call_count = 0;
        let mut files_modified = 0;

        // Base score for simply starting a session
        let mut total_score = 10;

        for activity in activities {
            if activity.tool_use.is_some() {
                tool_call_count += 1;
                total_score += 5;
            }

            if let Some(output) = &activity.output {
                #[allow(clippy::cast_possible_truncation)]
                let count = output.changed_files.len() as u32;
                files_modified += count;
                total_score += count * 2;
            }
        }

        // Also check session outputs
        if let Some(session_output) = &session.output {
            #[allow(clippy::cast_possible_truncation)]
            let count = session_output.changed_files.len() as u32;
            files_modified += count;
            total_score += count * 2;
        }

        for session_out in &session.outputs {
            #[allow(clippy::cast_possible_truncation)]
            let count = session_out.changed_files.len() as u32;
            files_modified += count;
            total_score += count * 2;
        }

        EffortReport {
            total_score,
            tool_call_count,
            files_modified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangedFile, SessionOutput, ToolUse};

    #[test]
    fn test_estimate_calculates_correct_score() {
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
            output: Some(SessionOutput {
                changed_files: vec![ChangedFile {
                    path: "src/main.rs".to_string(),
                    diff: String::new(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
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
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "list_files".to_string(),
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
                    changed_files: vec![
                        ChangedFile {
                            path: "src/lib.rs".to_string(),
                            diff: String::new(),
                        },
                        ChangedFile {
                            path: "Cargo.toml".to_string(),
                            diff: String::new(),
                        },
                    ],
                    commit_hash: None,
                    pull_request: None,
                }),
            },
        ];

        let estimator = SessionEstimator::new();
        let report = estimator.estimate(&session, &activities);

        // Score logic:
        // Base = 10
        // Tool calls = 2 * 5 = 10
        // Files modified = 3 * 2 = 6 (1 in session, 2 in activity 2)
        // Total = 26

        assert_eq!(report.total_score, 26);
        assert_eq!(report.tool_call_count, 2);
        assert_eq!(report.files_modified, 3);
    }
}
