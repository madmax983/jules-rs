use crate::reviewer::{ReviewFinding, SessionReviewer};
use crate::{Activity, Session};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Represents a review finding mapped back to the activity that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsFinding {
    /// The specific finding identified.
    pub finding: ReviewFinding,
    /// The name of the activity that introduced the finding.
    pub activity_name: String,
}

/// The complete forensics report mapping findings to their source activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// List of findings traced to their source activities.
    pub findings: Vec<ForensicsFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;

        if self.findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        writeln!(f, "Findings mapped to activities:")?;
        for entry in &self.findings {
            let line_info = match entry.finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {}{} - {} (Activity: {})",
                entry.finding.severity,
                entry.finding.file_path,
                line_info,
                entry.finding.message,
                entry.activity_name
            )?;
        }

        Ok(())
    }
}

/// Analyzes session activities to trace review findings back to their source.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Traces and maps `ReviewFinding`s to the specific `Activity` that introduced them.
    #[must_use]
    pub fn analyze(&self, _session: &Session, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut mapped_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    mapped_findings.push(ForensicsFinding {
                        finding,
                        activity_name: activity.name.clone(),
                    });
                }
            }
        }

        ForensicsReport {
            findings: mapped_findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviewer::FindingSeverity;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState, ToolUse};

    fn create_mock_session() -> Session {
        Session {
            name: "sessions/test".to_string(),
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
            plan: None,
            output: None,
            outputs: vec![],
        }
    }

    fn create_mock_activity(name: &str, output: Option<SessionOutput>) -> Activity {
        Activity {
            name: name.to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "write_file".to_string(),
                input: "content".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output,
        }
    }

    #[test]
    fn test_forensics_analyze_maps_findings_to_activity() {
        let session = create_mock_session();

        let output1 = SessionOutput {
            changed_files: vec![ChangedFile {
                path: "src/clean_test.rs".to_string(),
                diff: "@@ -0,0 +1,2 @@\n+fn test() {\n+    let x = Some(1).unwrap();\n+}"
                    .to_string(),
            }],
            commit_hash: None,
            pull_request: None,
        };

        let output2 = SessionOutput {
            changed_files: vec![ChangedFile {
                path: "src/clean_test2.rs".to_string(),
                diff: "@@ -0,0 +1,2 @@\n+fn test2() {\n+    // TODO: implement\n+}".to_string(),
            }],
            commit_hash: None,
            pull_request: None,
        };

        let activities = vec![
            create_mock_activity("activities/1", Some(output1)),
            create_mock_activity("activities/2", Some(output2)),
        ];

        let forensics = SessionForensics::new();
        let report = forensics.analyze(&session, &activities);

        assert_eq!(report.findings.len(), 2);

        let finding1 = &report.findings[0];
        assert_eq!(finding1.activity_name, "activities/1");
        assert_eq!(finding1.finding.severity, FindingSeverity::Warning);
        assert_eq!(finding1.finding.file_path, "src/clean_test.rs");

        let finding2 = &report.findings[1];
        assert_eq!(finding2.activity_name, "activities/2");
        assert_eq!(finding2.finding.severity, FindingSeverity::Info);
        assert_eq!(finding2.finding.file_path, "src/clean_test2.rs");
    }
}
