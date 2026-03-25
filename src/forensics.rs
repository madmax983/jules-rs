use crate::Activity;
use crate::reviewer::{FindingSeverity, SessionReviewer};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Represents a review finding mapped back to the specific activity that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFinding {
    pub activity_name: String,
    pub file_path: String,
    pub line_number: Option<usize>,
    pub severity: FindingSeverity,
    pub message: String,
}

/// The complete forensics report mapping findings to activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    pub activity_findings: Vec<ActivityFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;

        if self.activity_findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        writeln!(f, "Activity Findings:")?;
        for finding in &self.activity_findings {
            let line_info = match finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {} - {}{} - {}",
                finding.severity,
                finding.activity_name,
                finding.file_path,
                line_info,
                finding.message
            )?;
        }

        Ok(())
    }
}

/// Analyzes activities to map code review findings to the specific activity that caused them.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    #[must_use]
    pub fn analyze(&self, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut activity_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    activity_findings.push(ActivityFinding {
                        activity_name: activity.name.clone(),
                        file_path: finding.file_path,
                        line_number: finding.line_number,
                        severity: finding.severity,
                        message: finding.message,
                    });
                }
            }
        }

        ForensicsReport { activity_findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput};

    #[test]
    fn test_analyze_maps_finding_to_activity() {
        let activity = Activity {
            name: "sessions/1/activities/1".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: None,
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: Some(SessionOutput {
                changed_files: vec![ChangedFile {
                    path: "src/clean_test.rs".to_string(),
                    diff: "@@ -0,0 +1,2 @@\n+fn main() {\n+    unwrap();\n+}".to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        };

        let forensics = SessionForensics::new();
        let report = forensics.analyze(&[activity]);

        assert_eq!(report.activity_findings.len(), 1);

        let finding = &report.activity_findings[0];
        assert_eq!(finding.activity_name, "sessions/1/activities/1");
        assert_eq!(finding.file_path, "src/clean_test.rs");
        assert_eq!(finding.line_number, Some(2));
        assert_eq!(finding.severity, FindingSeverity::Warning);
        assert!(finding.message.contains("unwrap()"));
    }
}
