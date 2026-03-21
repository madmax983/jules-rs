use crate::Activity;
use serde::{Deserialize, Serialize};

use crate::reviewer::ReviewFinding;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicsFinding {
    pub activity_name: String,
    pub finding: ReviewFinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicsReport {
    pub findings: Vec<ForensicsFinding>,
}

#[derive(Default)]
pub struct SessionForensics {}

impl SessionForensics {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    #[must_use]
    pub fn analyze(&self, activities: &[Activity]) -> ForensicsReport {
        let reviewer = crate::reviewer::SessionReviewer::new();
        let mut forensics_findings = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                let report = reviewer.review_session(std::slice::from_ref(output));
                for finding in report.findings {
                    if finding.file_path != "Global" {
                        forensics_findings.push(ForensicsFinding {
                            activity_name: activity.name.clone(),
                            finding,
                        });
                    }
                }
            }
        }

        ForensicsReport {
            findings: forensics_findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChangedFile;
    use crate::SessionOutput;

    #[test]
    fn test_forensics_analysis() {
        let activities = vec![
            Activity {
                name: "sessions/1/activities/empty".to_string(),
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
                output: None,
            },
            Activity {
                name: "sessions/1/activities/bad_code".to_string(),
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
                        path: "src/bad.rs".to_string(),
                        diff:
                            "@@ -0,0 +1,2 @@\n+ fn test() {\n+     let x = Some(1).unwrap();\n+ }\n"
                                .to_string(),
                    }],
                    commit_hash: None,
                    pull_request: None,
                }),
            },
        ];

        let forensics = SessionForensics::new();
        let report = forensics.analyze(&activities);

        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.activity_name, "sessions/1/activities/bad_code");
        assert_eq!(finding.finding.file_path, "src/bad.rs");
        assert!(finding.finding.message.contains("unwrap()"));
    }
}
