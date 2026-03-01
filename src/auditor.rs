use std::fmt::{self, Display, Formatter};

use crate::{Activity, Session};
use crate::analytics::{SessionAnalyzer, SessionReport};
use crate::doctor::{Diagnosis, SessionDoctor};

/// A combined report containing both session analytics and diagnostic information.
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// The analytics report.
    pub analytics: SessionReport,
    /// The diagnoses from the session doctor.
    pub diagnoses: Vec<Diagnosis>,
}

impl Display for AuditReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.analytics)?;

        writeln!(f, "\nSession Diagnoses")?;
        writeln!(f, "=================")?;
        if self.diagnoses.is_empty() {
            writeln!(f, "  No issues detected. Session looks healthy!")?;
        } else {
            for diagnosis in &self.diagnoses {
                writeln!(
                    f,
                    "  [{}] {}",
                    diagnosis.level, diagnosis.message
                )?;
                writeln!(f, "      Recommendation: {}", diagnosis.recommendation)?;
            }
        }

        Ok(())
    }
}

/// Combines analytics and diagnostics into a unified audit report.
#[derive(Debug, Default)]
pub struct SessionAuditor {
    analyzer: SessionAnalyzer,
    doctor: SessionDoctor,
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

    /// Audits the given session and activities to produce a unified report.
    #[must_use]
    pub fn audit(&self, session: &Session, activities: &[Activity]) -> AuditReport {
        let analytics = self.analyzer.analyze(session, activities);
        let diagnoses = self.doctor.examine(session, activities);

        AuditReport {
            analytics,
            diagnoses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActivityStatus, ChangedFile, Plan, PlanStep, SessionOutput, SessionState, ToolUse,
    };

    fn create_mock_activity(
        name: &str,
        status: ActivityStatus,
        tool: Option<(&str, &str)>,
        output: Option<SessionOutput>,
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
            output,
        }
    }

    #[test]
    fn test_audit_session() {
        let session = Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("Bug fix session".to_string()),
            description: None,
            state: Some(SessionState::Completed),
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: Some(Plan {
                steps: vec![
                    PlanStep {
                        description: "Step 1".to_string(),
                        status: "completed".to_string(),
                    },
                ],
            }),
            output: None,
            outputs: vec![],
        };

        // Create a sequence that triggers both analytics and doctor warnings (e.g. a loop)
        let activities = vec![
            create_mock_activity(
                "act-1",
                ActivityStatus::Success,
                Some(("read_file", "{}")),
                None,
            ),
            create_mock_activity(
                "act-2",
                ActivityStatus::Success,
                Some(("read_file", "{}")),
                None,
            ),
            create_mock_activity(
                "act-3",
                ActivityStatus::Success,
                Some(("read_file", "{}")),
                Some(SessionOutput {
                    changed_files: vec![ChangedFile {
                        path: "src/main.rs".to_string(),
                        diff: String::new(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            ),
        ];

        let auditor = SessionAuditor::new();
        let report = auditor.audit(&session, &activities);

        // Check Analytics portion
        assert_eq!(report.analytics.total_activities, 3);
        assert_eq!(report.analytics.total_steps, 1);
        assert_eq!(report.analytics.completed_steps, 1);
        assert_eq!(report.analytics.tool_usage_counts.get("read_file"), Some(&3));
        assert!(report.analytics.modified_files.contains(&"src/main.rs".to_string()));

        // Check Doctor portion
        assert_eq!(report.diagnoses.len(), 1);
        assert_eq!(report.diagnoses[0].level, crate::doctor::DiagnosisLevel::Warning);
        assert!(report.diagnoses[0].message.contains("Potential loop detected"));
    }
}
