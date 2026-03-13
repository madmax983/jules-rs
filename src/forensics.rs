use crate::{Activity, ReviewFinding, ReviewReport};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedFinding {
    pub finding: ReviewFinding,
    pub activity_name: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicReport {
    pub mapped_findings: Vec<MappedFinding>,
}

impl Display for ForensicReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensic Report")?;
        writeln!(f, "=======================")?;
        for mapped in &self.mapped_findings {
            let activity_info = mapped
                .activity_name
                .as_deref()
                .unwrap_or("Unknown Activity");
            let tool_info = mapped.tool_name.as_deref().unwrap_or("Unknown Tool");
            writeln!(
                f,
                "[{}] {}: {} (Introduced by {} using {})",
                mapped.finding.severity,
                mapped.finding.file_path,
                mapped.finding.message,
                activity_info,
                tool_info
            )?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    #[must_use]
    pub fn investigate(
        &self,
        activities: &[Activity],
        review_report: &ReviewReport,
    ) -> ForensicReport {
        let mut mapped_findings = Vec::new();

        for finding in &review_report.findings {
            let mut found_activity_name = None;
            let mut found_tool_name = None;

            // Iterate backwards to find the most recent activity that modified the file
            for activity in activities.iter().rev() {
                if let Some(output) = &activity.output {
                    if output
                        .changed_files
                        .iter()
                        .any(|f| f.path == finding.file_path)
                    {
                        found_activity_name = Some(activity.name.clone());
                        if let Some(tool) = &activity.tool_use {
                            found_tool_name = Some(tool.tool_name.clone());
                        }
                        break;
                    }
                }
            }

            mapped_findings.push(MappedFinding {
                finding: finding.clone(),
                activity_name: found_activity_name,
                tool_name: found_tool_name,
            });
        }

        ForensicReport { mapped_findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, FindingSeverity, SessionOutput, ToolUse};

    fn create_mock_activity(
        name: &str,
        tool: Option<(&str, &str)>,
        output_files: Vec<&str>,
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
            output: if output_files.is_empty() {
                None
            } else {
                Some(SessionOutput {
                    changed_files: output_files
                        .into_iter()
                        .map(|p| ChangedFile {
                            path: p.to_string(),
                            diff: String::new(),
                        })
                        .collect(),
                    commit_hash: None,
                    pull_request: None,
                })
            },
        }
    }

    #[test]
    fn test_investigate_finding_mapped() {
        let activities = vec![
            create_mock_activity("act-1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("act-2", Some(("write_file", "bar")), vec!["src/main.rs"]),
            create_mock_activity("act-3", Some(("run_tests", "foo")), vec![]),
        ];

        let review_report = ReviewReport {
            lines_added: 10,
            lines_removed: 0,
            findings: vec![ReviewFinding {
                file_path: "src/main.rs".to_string(),
                line_number: Some(5),
                severity: FindingSeverity::Error,
                message: "A bad error".to_string(),
            }],
        };

        let forensics = SessionForensics::new();
        let report = forensics.investigate(&activities, &review_report);

        assert_eq!(report.mapped_findings.len(), 1);
        assert_eq!(report.mapped_findings[0].finding.file_path, "src/main.rs");
        assert_eq!(
            report.mapped_findings[0].activity_name,
            Some("act-2".to_string())
        );
        assert_eq!(
            report.mapped_findings[0].tool_name,
            Some("write_file".to_string())
        );
    }

    #[test]
    fn test_investigate_unmapped_finding() {
        let activities = vec![
            create_mock_activity("act-1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("act-2", Some(("write_file", "bar")), vec!["src/lib.rs"]),
        ];

        let review_report = ReviewReport {
            lines_added: 10,
            lines_removed: 0,
            findings: vec![ReviewFinding {
                file_path: "src/main.rs".to_string(),
                line_number: Some(5),
                severity: FindingSeverity::Error,
                message: "A bad error".to_string(),
            }],
        };

        let forensics = SessionForensics::new();
        let report = forensics.investigate(&activities, &review_report);

        assert_eq!(report.mapped_findings.len(), 1);
        assert_eq!(report.mapped_findings[0].finding.file_path, "src/main.rs");
        assert_eq!(report.mapped_findings[0].activity_name, None);
        assert_eq!(report.mapped_findings[0].tool_name, None);
    }
}
