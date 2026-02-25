use crate::{Activity, ActivityStatus, Session};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosisLevel {
    Info,
    Warning,
    Error,
}

impl fmt::Display for DiagnosisLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub level: DiagnosisLevel,
    pub message: String,
    pub recommendation: String,
}

#[derive(Debug, Default)]
pub struct SessionDoctor;

impl SessionDoctor {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn examine(&self, _session: &Session, activities: &[Activity]) -> Vec<Diagnosis> {
        let mut diagnoses = Vec::new();
        diagnoses.extend(self.check_loops(activities));
        diagnoses.extend(self.check_stagnation(activities));
        diagnoses.extend(self.check_error_rate(activities));
        diagnoses
    }

    #[allow(clippy::unused_self)]
    fn check_loops(&self, activities: &[Activity]) -> Vec<Diagnosis> {
        let mut diagnoses = Vec::new();
        // Simple loop detection: same tool and input 3+ times in a row
        let mut consecutive_count = 0;
        let mut last_tool: Option<(&str, &str)> = None;

        for activity in activities {
            if let Some(tool) = &activity.tool_use {
                let current_tool = (tool.tool_name.as_str(), tool.input.as_str());
                if let Some(last) = last_tool {
                    if last == current_tool {
                        consecutive_count += 1;
                    } else {
                        consecutive_count = 1;
                        last_tool = Some(current_tool);
                    }
                } else {
                    consecutive_count = 1;
                    last_tool = Some(current_tool);
                }

                if consecutive_count >= 3 {
                    // Only report once per loop sequence to avoid spam, but here we simplify
                    if consecutive_count == 3 {
                        diagnoses.push(Diagnosis {
                            level: DiagnosisLevel::Warning,
                            message: format!(
                                "Potential loop detected: Tool '{}' used 3 times in a row with identical input.",
                                tool.tool_name
                            ),
                            recommendation: "Check if the agent is stuck. Try providing a hint or changing the request."
                                .to_string(),
                        });
                    }
                }
            } else {
                consecutive_count = 0;
                last_tool = None;
            }
        }
        diagnoses
    }

    #[allow(clippy::unused_self)]
    fn check_stagnation(&self, activities: &[Activity]) -> Vec<Diagnosis> {
        let mut diagnoses = Vec::new();
        let mut no_change_count = 0;

        for activity in activities {
            if let Some(status) = activity.status {
                if status == ActivityStatus::Success {
                    let has_changes = activity
                        .output
                        .as_ref()
                        .is_some_and(|o| !o.changed_files.is_empty());
                    if has_changes {
                        no_change_count = 0;
                    } else {
                        no_change_count += 1;
                    }
                }
            }
        }

        if no_change_count >= 10 {
            diagnoses.push(Diagnosis {
                level: DiagnosisLevel::Info,
                message: format!(
                    "Session has gone {no_change_count} successful steps without modifying any files."
                ),
                recommendation: "Ensure the agent is actually making progress and not just reading/thinking."
                    .to_string(),
            });
        }

        diagnoses
    }

    #[allow(clippy::unused_self)]
    fn check_error_rate(&self, activities: &[Activity]) -> Vec<Diagnosis> {
        let mut diagnoses = Vec::new();
        if activities.len() < 5 {
            return diagnoses;
        }

        // Check last 10 activities
        let window_size = 10;
        let start = activities.len().saturating_sub(window_size);
        let window = &activities[start..];

        let failed_count = window
            .iter()
            .filter(|a| matches!(a.status, Some(ActivityStatus::Failed)))
            .count();

        if failed_count >= 5 {
            diagnoses.push(Diagnosis {
                level: DiagnosisLevel::Error,
                message: format!(
                    "High error rate detected: {failed_count} failures in the last {} activities.",
                    window.len()
                ),
                recommendation:
                    "The agent seems to be struggling. Check the error logs and environment setup."
                        .to_string(),
            });
        }

        diagnoses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activity, ActivityStatus, ChangedFile, SessionOutput, ToolUse};

    fn create_mock_activity(
        name: &str,
        status: ActivityStatus,
        tool: Option<(&str, &str)>,
        changed_files: bool,
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
            output: if changed_files {
                Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "file.txt".to_string(),
                        diff: "diff".to_string(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                })
            } else {
                None
            },
        }
    }

    #[test]
    fn test_check_loops() {
        let doctor = SessionDoctor::new();
        let activities = vec![
            create_mock_activity(
                "1",
                ActivityStatus::Success,
                Some(("read_file", "foo")),
                false,
            ),
            create_mock_activity(
                "2",
                ActivityStatus::Success,
                Some(("read_file", "foo")),
                false,
            ),
            create_mock_activity(
                "3",
                ActivityStatus::Success,
                Some(("read_file", "foo")),
                false,
            ),
        ];

        let diagnoses = doctor.check_loops(&activities);
        assert_eq!(diagnoses.len(), 1);
        assert_eq!(diagnoses[0].level, DiagnosisLevel::Warning);
        assert!(diagnoses[0].message.contains("used 3 times"));
    }

    #[test]
    fn test_check_stagnation() {
        let doctor = SessionDoctor::new();
        let mut activities = Vec::new();
        for i in 0..10 {
            activities.push(create_mock_activity(
                &format!("{i}"),
                ActivityStatus::Success,
                None,
                false,
            ));
        }

        let diagnoses = doctor.check_stagnation(&activities);
        assert_eq!(diagnoses.len(), 1);
        assert_eq!(diagnoses[0].level, DiagnosisLevel::Info);
        assert!(diagnoses[0].message.contains("10 successful steps"));
    }

    #[test]
    fn test_check_error_rate() {
        let doctor = SessionDoctor::new();
        let mut activities = Vec::new();
        for i in 0..10 {
            let status = if i < 5 {
                ActivityStatus::Failed
            } else {
                ActivityStatus::Success
            };
            activities.push(create_mock_activity(&format!("{i}"), status, None, false));
        }

        let diagnoses = doctor.check_error_rate(&activities);
        assert_eq!(diagnoses.len(), 1);
        assert_eq!(diagnoses[0].level, DiagnosisLevel::Error);
        assert!(diagnoses[0].message.contains("5 failures"));
    }
}
