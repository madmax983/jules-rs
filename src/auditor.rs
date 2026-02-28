use std::fmt::{self, Display, Formatter};

use crate::{Activity, Diagnosis, Session, SessionAnalyzer, SessionDoctor, SessionReport};

/// An audit report for a session, combining analytics and diagnoses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReport {
    pub analytics: SessionReport,
    pub diagnoses: Vec<Diagnosis>,
}

impl Display for AuditReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.analytics)?;

        writeln!(f, "\nSession Diagnoses")?;
        writeln!(f, "-----------------")?;
        if self.diagnoses.is_empty() {
            writeln!(f, "  No issues detected.")?;
        } else {
            for diagnosis in &self.diagnoses {
                writeln!(f, "[{}] {}", diagnosis.level, diagnosis.message)?;
                writeln!(f, "  Recommendation: {}", diagnosis.recommendation)?;
            }
        }

        Ok(())
    }
}

/// Auditor that combines analytics and diagnostics.
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

    /// Audits the session and returns a combined report.
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

    #[test]
    fn test_audit_report_contains_metrics_and_diagnoses() {
        let auditor = SessionAuditor::new();
        let session = Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("Bug fix session".to_string()),
            description: None,
            state: None,
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

        let report = auditor.audit(&session, &[]);
        let display = format!("{report}");

        // This should fail initially because the minimal struct doesn't include these
        assert!(display.contains("Session Analytics Report"));
        assert!(display.contains("Session Diagnoses"));
    }
}
