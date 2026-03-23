use crate::{Activity, ReviewFinding, SessionReviewer};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A review finding mapped to the specific activity that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsFinding {
    /// The name of the activity that introduced the finding.
    pub activity_name: String,
    /// The specific finding identified.
    pub finding: ReviewFinding,
}

/// The complete forensics report mapping review findings to activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// List of findings mapped to their source activities.
    pub findings: Vec<ForensicsFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;

        if self.findings.is_empty() {
            writeln!(f, "No issues found during forensics analysis.")?;
            return Ok(());
        }

        writeln!(f, "Findings Mapping:")?;
        for entry in &self.findings {
            let finding = &entry.finding;
            let line_info = match finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {}{} (Activity: {}) - {}",
                finding.severity,
                finding.file_path,
                line_info,
                entry.activity_name,
                finding.message
            )?;
        }

        Ok(())
    }
}

/// Analyzes a session's activities to map review findings back to specific actions.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Analyzes the provided activities to generate a `ForensicsReport`.
    #[must_use]
    pub fn analyze_activities(&self, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    if finding.file_path != "Global" {
                        findings.push(ForensicsFinding {
                            activity_name: activity.name.clone(),
                            finding,
                        });
                    }
                }
            }
        }

        ForensicsReport { findings }
    }
}
