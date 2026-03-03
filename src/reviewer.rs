use crate::{Activity, Session};

#[derive(Debug, Default)]
pub struct SessionReviewer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewIssue {
    pub file_path: String,
    pub issue_type: String,
    pub description: String,
}

impl SessionReviewer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn review(&self, _session: &Session, activities: &[Activity]) -> Vec<ReviewIssue> {
        let mut issues = Vec::new();

        for activity in activities {
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    let mut in_replace_block = false;

                    for line in file.diff.lines() {
                        if line.starts_with("=======") {
                            in_replace_block = true;
                            continue;
                        } else if line.starts_with(">>>>>>> REPLACE") {
                            in_replace_block = false;
                            continue;
                        }

                        if in_replace_block {
                            if line.contains(".unwrap()") {
                                issues.push(ReviewIssue {
                                    file_path: file.path.clone(),
                                    issue_type: "unwrap".to_string(),
                                    description: "Found usage of `.unwrap()`. Consider handling the error properly.".to_string(),
                                });
                            }
                            if line.contains("TODO") || line.contains("todo!") {
                                issues.push(ReviewIssue {
                                    file_path: file.path.clone(),
                                    issue_type: "TODO".to_string(),
                                    description: "Found incomplete task (`TODO` or `todo!`)."
                                        .to_string(),
                                });
                            }
                            if line.contains("println!") {
                                issues.push(ReviewIssue {
                                    file_path: file.path.clone(),
                                    issue_type: "println!".to_string(),
                                    description:
                                        "Found usage of `println!`. Use proper logging instead."
                                            .to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState};

    fn create_mock_activity_with_diff(diff: &str) -> Activity {
        Activity {
            name: "test-activity".to_string(),
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
                    path: "src/main.rs".to_string(),
                    diff: diff.to_string(),
                }],
                commit_hash: None,
                pull_request: None,
            }),
        }
    }

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

    #[test]
    fn test_review_flags_unwrap() {
        let diff = "
<<<<<<< SEARCH
=======
    let val = option.unwrap();
>>>>>>> REPLACE
        ";
        let session = create_mock_session();
        let activities = vec![create_mock_activity_with_diff(diff)];

        let reviewer = SessionReviewer::new();
        let issues = reviewer.review(&session, &activities);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "unwrap");
        assert_eq!(issues[0].file_path, "src/main.rs");
    }

    #[test]
    fn test_review_flags_todo() {
        let diff = "
<<<<<<< SEARCH
=======
    // TODO: implement this
>>>>>>> REPLACE
        ";
        let session = create_mock_session();
        let activities = vec![create_mock_activity_with_diff(diff)];

        let reviewer = SessionReviewer::new();
        let issues = reviewer.review(&session, &activities);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "TODO");
    }

    #[test]
    fn test_review_flags_println() {
        let diff = "
<<<<<<< SEARCH
=======
    println!(\"debug\");
>>>>>>> REPLACE
        ";
        let session = create_mock_session();
        let activities = vec![create_mock_activity_with_diff(diff)];

        let reviewer = SessionReviewer::new();
        let issues = reviewer.review(&session, &activities);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "println!");
    }

    #[test]
    fn test_review_ignores_search_block() {
        let diff = "
<<<<<<< SEARCH
    let val = option.unwrap();
=======
    let val = option.expect(\"error\");
>>>>>>> REPLACE
        ";
        let session = create_mock_session();
        let activities = vec![create_mock_activity_with_diff(diff)];

        let reviewer = SessionReviewer::new();
        let issues = reviewer.review(&session, &activities);

        assert!(issues.is_empty());
    }
}
