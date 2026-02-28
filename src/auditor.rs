use std::fmt::{self, Display, Formatter};

use crate::{Activity, Diagnosis, Session, SessionAnalyzer, SessionDoctor, SessionReport};

/// Auditor that combines analytics and diagnostics into a unified report.
#[derive(Debug, Default)]
pub struct SessionAuditor {
    analyzer: SessionAnalyzer,
    doctor: SessionDoctor,
}

/// A combined audit report of session health and metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAuditReport {
    pub metrics: SessionReport,
    pub diagnoses: Vec<Diagnosis>,
}

impl SessionAuditor {
    /// Creates a new `SessionAuditor`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            analyzer: SessionAnalyzer::new(),
            doctor: SessionDoctor::new(),
        }
    }

    /// Generates a comprehensive audit report for a session.
    #[must_use]
    pub fn audit(&self, session: &Session, activities: &[Activity]) -> SessionAuditReport {
        let metrics = self.analyzer.analyze(session, activities);
        let diagnoses = self.doctor.examine(session, activities);

        SessionAuditReport { metrics, diagnoses }
    }
}

impl Display for SessionAuditReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SESSION AUDIT REPORT ===")?;
        writeln!(f)?;

        writeln!(f, "--- HEALTH DIAGNOSES ---")?;
        if self.diagnoses.is_empty() {
            writeln!(f, "  No issues detected. The session is healthy.")?;
        } else {
            for diagnosis in &self.diagnoses {
                writeln!(f, "[{}] {}", diagnosis.level, diagnosis.message)?;
                writeln!(f, "  Recommendation: {}", diagnosis.recommendation)?;
            }
        }
        writeln!(f)?;

        writeln!(f, "--- METRICS ---")?;
        write!(f, "{}", self.metrics)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState, ToolUse};

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
    fn test_auditor_generates_combined_report() {
        let session = Session {
            name: "sessions/test-audit".to_string(),
            id: Some("test-audit".to_string()),
            prompt: Some("Fix it".to_string()),
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
        };

        // Create 3 identical tool uses to trigger a loop warning in Doctor,
        // and overall 3 activities for Analytics.
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

        let auditor = SessionAuditor::new();
        let report = auditor.audit(&session, &activities);

        // Metrics from Analytics
        assert_eq!(
            report.metrics.total_activities, 3,
            "Expected 3 total activities in metrics"
        );
        assert_eq!(
            report.metrics.tool_usage_counts.get("read_file"),
            Some(&3),
            "Expected 3 'read_file' tool usages"
        );

        // Diagnoses from Doctor
        assert_eq!(report.diagnoses.len(), 1, "Expected 1 loop diagnosis");
        assert!(
            report.diagnoses[0].message.contains("used 3 times"),
            "Expected loop warning message"
        );
    }
}
