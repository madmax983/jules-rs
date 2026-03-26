use crate::{Activity, ReviewFinding, SessionReviewer};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A finding mapped to a specific activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsFinding {
    /// The ID or name of the activity that introduced the finding.
    pub activity_id: String,
    /// The name of the tool used in the activity, if any.
    pub tool_name: Option<String>,
    /// The underlying review finding.
    pub finding: ReviewFinding,
}

/// The complete forensics report for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// Total number of findings across all activities.
    pub total_findings: usize,
    /// List of findings mapped to their introducing activity.
    pub findings: Vec<ForensicsFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;
        writeln!(f, "Total Findings: {}", self.total_findings)?;
        writeln!(f)?;

        if self.findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        writeln!(f, "Findings:")?;
        for finding in &self.findings {
            let line_info = match finding.finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            let tool_info = match &finding.tool_name {
                Some(tool) => format!(" [{tool}]"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {}{} (Activity: {}{}) - {}",
                finding.finding.severity,
                finding.finding.file_path,
                line_info,
                finding.activity_id,
                tool_info,
                finding.finding.message
            )?;
        }

        Ok(())
    }
}

/// Analyzes activities to trace review findings back to their source.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Analyzes the provided activities and generates a `ForensicsReport`.
    #[must_use]
    pub fn analyze(&self, activities: &[Activity]) -> ForensicsReport {
        let mut findings = Vec::new();
        let reviewer = SessionReviewer::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    findings.push(ForensicsFinding {
                        activity_id: activity.name.clone(),
                        tool_name: activity.tool_use.as_ref().map(|t| t.tool_name.clone()),
                        finding,
                    });
                }
            }
        }

        ForensicsReport {
            total_findings: findings.len(),
            findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, ToolUse};

    fn create_mock_activity(
        name: &str,
        tool: Option<(&str, &str)>,
        output: Option<SessionOutput>,
    ) -> Activity {
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
    fn test_analyze_finds_issues() {
        let activities = vec![
            create_mock_activity("act-1", Some(("read_file", "{}")), None),
            create_mock_activity(
                "act-2",
                Some(("write_file", "{}")),
                Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "src/clean_test.rs".to_string(),
                        diff: "@@ -0,0 +1,2 @@\n+fn main() {\n+    Some(1).unwrap();\n+}"
                            .to_string(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            ),
        ];

        let forensics = SessionForensics::new();
        let report = forensics.analyze(&activities);

        assert_eq!(report.total_findings, 1);
        assert_eq!(report.findings.len(), 1);

        let finding = &report.findings[0];
        assert_eq!(finding.activity_id, "act-2");
        assert_eq!(finding.tool_name.as_deref(), Some("write_file"));
        assert_eq!(finding.finding.file_path, "src/clean_test.rs");
        assert!(finding.finding.message.contains("unwrap()"));
    }
}
