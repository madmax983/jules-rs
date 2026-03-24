use crate::{Activity, ReviewFinding, SessionReviewer};
use serde::{Deserialize, Serialize};

/// Trace of findings back to the specific activity that introduced them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFindings {
    /// The ID/name of the activity that introduced the findings.
    pub activity_id: String,
    /// The list of findings introduced by this activity.
    pub findings: Vec<ReviewFinding>,
}

/// The complete forensics report tracing findings to activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicsReport {
    /// The name of the session analyzed.
    pub session_name: String,
    /// Findings mapped to the activities that introduced them.
    pub activity_findings: Vec<ActivityFindings>,
}

/// Traces and maps `ReviewFinding`s back to the specific `Activity` that introduced them.
#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    /// Creates a new `SessionForensics` instance.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Analyzes the provided session activities and traces findings to specific activities.
    #[must_use]
    pub fn analyze(&self, session_name: &str, activities: &[Activity]) -> ForensicsReport {
        let reviewer = SessionReviewer::new();
        let mut activity_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                if !report.findings.is_empty() {
                    activity_findings.push(ActivityFindings {
                        activity_id: activity.name.clone(),
                        findings: report.findings,
                    });
                }
            }
        }

        ForensicsReport {
            session_name: session_name.to_string(),
            activity_findings,
        }
    }
}
