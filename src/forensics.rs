use crate::{Activity, ReviewFinding, SessionReviewer};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// The complete forensics report for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// Findings mapped to the name of the activity that introduced them.
    pub findings_by_activity: BTreeMap<String, Vec<ReviewFinding>>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;
        writeln!(f)?;

        if self.findings_by_activity.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        for (activity_name, findings) in &self.findings_by_activity {
            writeln!(f, "Activity: {activity_name}")?;
            for finding in findings {
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

/// Analyzes session activities to trace code review findings to specific activities.
#[derive(Default)]
pub struct SessionForensics {
    reviewer: SessionReviewer,
}

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reviewer: SessionReviewer::new(),
        }
    }

    /// Analyzes the provided activities and generates a `ForensicsReport`.
    #[must_use]
    pub fn analyze(&self, activities: &[Activity]) -> ForensicsReport {
        let mut findings_by_activity = BTreeMap::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = self.reviewer.review_session(std::slice::from_ref(output));
                if !report.findings.is_empty() {
                    findings_by_activity.insert(activity.name.clone(), report.findings);
                }
            }
        }

        ForensicsReport {
            findings_by_activity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, FindingSeverity, SessionOutput};

    #[test]
    fn test_forensics_analysis() {
        let forensics = SessionForensics::new();

        let activity = Activity {
            name: "activity-1".to_string(),
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
                    diff: "@@ -0,0 +1,2 @@\n+fn test() {\n+    unwrap();\n+}".to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        };

        let report = forensics.analyze(&[activity]);

        assert_eq!(report.findings_by_activity.len(), 1);
        let findings = report.findings_by_activity.get("activity-1").unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
        assert!(findings[0].message.contains("unwrap()"));
    }
}
