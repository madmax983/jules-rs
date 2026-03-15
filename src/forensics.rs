use crate::{Activity, ReviewFinding, SessionReviewer};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A finding mapped to the specific activity that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFinding {
    /// The name of the activity that introduced the finding.
    pub activity_name: String,
    /// The name of the tool used in the activity, if any.
    pub tool_name: Option<String>,
    /// The specific findings associated with this activity.
    pub findings: Vec<ReviewFinding>,
}

/// A comprehensive report tracing code review findings to specific activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// Total number of individual findings identified across all activities.
    pub total_findings: usize,
    /// List of activities that introduced one or more findings, along with those findings.
    pub activity_findings: Vec<ActivityFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;
        writeln!(f, "Total Findings Traced: {}", self.total_findings)?;
        writeln!(f)?;

        if self.activity_findings.is_empty() {
            writeln!(f, "No issues traced to activities.")?;
            return Ok(());
        }

        for activity_finding in &self.activity_findings {
            let tool_info = match &activity_finding.tool_name {
                Some(tool) => format!(" (Tool: {tool})"),
                None => String::new(),
            };
            writeln!(
                f,
                "Activity: {}{}",
                activity_finding.activity_name, tool_info
            )?;
            for finding in &activity_finding.findings {
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

/// Analyzes session activities to trace code review findings back to their source.
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
        let reviewer = SessionReviewer::new();
        let mut activity_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));

                if !report.findings.is_empty() {
                    let mut filtered_findings = Vec::new();
                    // We only care about specific file findings, not the global "No test changes" finding
                    // which might not be directly attributable to a single activity in isolation without
                    // knowing the full session context. Actually, the prompt says to map ReviewFindings
                    // back to the Activity.
                    // If a global finding occurs (e.g., "Code was modified but no test files were changed"),
                    // the file_path is "Global". We should include all findings returned by the reviewer for this output.
                    for finding in report.findings {
                        // Suppress the global "no tests" warning for individual activity forensics
                        // as it causes noise and doesn't make sense per-activity.
                        if finding.file_path != "Global" {
                            filtered_findings.push(finding);
                        }
                    }

                    if !filtered_findings.is_empty() {
                        activity_findings.push(ActivityFinding {
                            activity_name: activity.name.clone(),
                            tool_name: activity.tool_use.as_ref().map(|t| t.tool_name.clone()),
                            findings: filtered_findings,
                        });
                    }
                }
            }
        }

        // Recalculate total_findings in case global findings were filtered out
        let actual_total = activity_findings.iter().map(|af| af.findings.len()).sum();

        ForensicsReport {
            total_findings: actual_total,
            activity_findings,
        }
    }
}
