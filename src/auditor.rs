use crate::analytics::{SessionAnalyzer, SessionReport};
use crate::doctor::{Diagnosis, SessionDoctor};
use crate::{Activity, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct SessionAuditor;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub report: SessionReport,
    pub diagnoses: Vec<Diagnosis>,
}

impl SessionAuditor {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn audit(&self, session: &Session, activities: &[Activity]) -> AuditReport {
        let analyzer = SessionAnalyzer::new();
        let report = analyzer.analyze(session, activities);

        let doctor = SessionDoctor::new();
        let diagnoses = doctor.examine(session, activities);

        AuditReport { report, diagnoses }
    }
}
