use crate::Activity;
use crate::reviewer::{ReviewFinding, SessionReviewer};
use serde::{Deserialize, Serialize};

/// Represents findings discovered in a specific activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsFinding {
    /// The name of the activity that introduced these findings.
    pub activity_name: String,
    /// The findings introduced by this activity.
    pub findings: Vec<ReviewFinding>,
}

/// The report containing all forensics mappings for a session's activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// The mapped findings per activity.
    pub activity_findings: Vec<ForensicsFinding>,
    /// The total number of findings across all activities.
    pub total_findings: usize,
}

impl std::fmt::Display for ForensicsReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;
        writeln!(f, "Total Findings: {}", self.total_findings)?;
        writeln!(f)?;

        if self.activity_findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        for forensics_finding in &self.activity_findings {
            writeln!(f, "Activity: {}", forensics_finding.activity_name)?;
            for finding in &forensics_finding.findings {
                let line_info = match finding.line_number {
                    Some(line) => format!(":{line}"),
                    None => String::new(),
                };
                writeln!(
                    f,
                    "  [{}] {}{} - {}",
                    finding.severity, finding.file_path, line_info, finding.message
                )?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

/// Traces and maps `ReviewFinding`s back to the specific `Activity` that introduced them.
#[derive(Default)]
pub struct SessionForensics;

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Analyzes a list of activities and maps findings to the activity that introduced them.
    #[must_use]
    pub fn analyze(&self, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut activity_findings = Vec::new();
        let mut total_findings = 0;

        for activity in activities {
            if let Some(output) = &activity.output {
                let review_report = reviewer.review_session(std::slice::from_ref(output));
                if !review_report.findings.is_empty() {
                    total_findings += review_report.findings.len();
                    activity_findings.push(ForensicsFinding {
                        activity_name: activity.name.clone(),
                        findings: review_report.findings,
                    });
                }
            }
        }

        ForensicsReport {
            activity_findings,
            total_findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviewer::FindingSeverity;
    use crate::{ActivityStatus, ChangedFile, SessionOutput};

    #[test]
    fn test_forensics_mapping() {
        let activity1 = Activity {
            name: "activity/1".to_string(),
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
                    diff: "@@ -0,0 +1,2 @@\n+fn test() {\n+  unwrap();\n+}".to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        };

        let activity2 = Activity {
            name: "activity/2".to_string(),
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
                    diff: "@@ -0,0 +1,2 @@\n+// TODO: Fix this\n+fn another() {}".to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        };

        let activity_clean = Activity {
            name: "activity/3".to_string(),
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
                    diff: "@@ -0,0 +1,2 @@\n+fn nice() {}\n+fn cool() {}".to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        };

        let forensics = SessionForensics::new();
        let report = forensics.analyze(&[activity1, activity2, activity_clean]);

        assert_eq!(report.total_findings, 2);
        assert_eq!(report.activity_findings.len(), 2);

        let find1 = &report.activity_findings[0];
        assert_eq!(find1.activity_name, "activity/1");
        assert_eq!(find1.findings.len(), 1);
        assert_eq!(find1.findings[0].severity, FindingSeverity::Warning);
        assert!(find1.findings[0].message.contains("unwrap()"));

        let find2 = &report.activity_findings[1];
        assert_eq!(find2.activity_name, "activity/2");
        assert_eq!(find2.findings.len(), 1);
        assert_eq!(find2.findings[0].severity, FindingSeverity::Info);
        assert!(find2.findings[0].message.contains("TODO"));
    }
}
