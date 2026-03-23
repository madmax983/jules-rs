use crate::{Activity, ReviewFinding, Session, SessionReviewer};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// A review finding traced back to the specific activity that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TracedFinding {
    /// The name of the activity that introduced the finding.
    pub activity_name: String,
    /// The specific finding identified.
    pub finding: ReviewFinding,
}

/// A forensics report tracing all review findings to their originating activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// The session analyzed.
    pub session_id: String,
    /// List of traced findings.
    pub findings: Vec<TracedFinding>,
}

impl Display for ForensicsReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Forensics Report")?;
        writeln!(f, "========================")?;
        writeln!(f, "Session ID: {}", self.session_id)?;
        writeln!(f)?;

        if self.findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        writeln!(f, "Traced Findings:")?;
        for traced in &self.findings {
            let line_info = match traced.finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {}{} (Activity: {}) - {}",
                traced.finding.severity,
                traced.finding.file_path,
                line_info,
                traced.activity_name,
                traced.finding.message
            )?;
        }

        Ok(())
    }
}

/// Analyzes a session's activities to map `ReviewFinding`s to the activity that created them.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    /// Creates a new `SessionForensics`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Traces review findings through the session's activities.
    #[must_use]
    pub fn trace(&self, session: &Session, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut traced_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    traced_findings.push(TracedFinding {
                        activity_name: activity.name.clone(),
                        finding,
                    });
                }
            }
        }

        ForensicsReport {
            session_id: session.name.clone(),
            findings: traced_findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangedFile, FindingSeverity, SessionOutput, SessionState};

    fn create_mock_session() -> Session {
        Session {
            name: "sessions/test-1".to_string(),
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

    fn create_mock_activity(name: &str, diff: &str) -> Activity {
        Activity {
            name: name.to_string(),
            status: None,
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
                    path: "src/main.rs".to_string(),
                    diff: diff.to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        }
    }

    #[test]
    fn test_trace_findings() {
        let session = create_mock_session();
        let activities = vec![create_mock_activity(
            "mock-act",
            "@@ -0,0 +1 @@\n+fn main() { let x = Option::Some(1).unwrap(); }",
        )];

        let forensics = SessionForensics::new();
        let report = forensics.trace(&session, &activities);

        assert_eq!(report.session_id, "sessions/test-1");

        // One finding from unwrap(), one global finding because no test files changed
        assert_eq!(report.findings.len(), 2);

        // Find the unwrap finding
        let traced = report
            .findings
            .iter()
            .find(|t| t.finding.message.contains("unwrap()"))
            .unwrap();
        assert_eq!(traced.activity_name, "mock-act");
        assert_eq!(traced.finding.file_path, "src/main.rs");
        assert_eq!(traced.finding.severity, FindingSeverity::Warning);
        assert!(traced.finding.message.contains("unwrap()"));
    }
}
