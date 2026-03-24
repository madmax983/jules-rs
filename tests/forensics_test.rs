#![cfg(feature = "forensics")]

use jules_rs::{Activity, ActivityStatus, ChangedFile, Session, SessionForensics, SessionOutput, SessionState};

fn create_mock_session(name: &str) -> Session {
    Session {
        name: name.to_string(),
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

fn create_mock_activity(name: &str, output: Option<SessionOutput>) -> Activity {
    Activity {
        name: name.to_string(),
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
        output,
    }
}

#[test]
fn test_forensics_traces_findings() {
    let session = create_mock_session("sessions/123");

    let clean_output = SessionOutput {
        changed_files: vec![ChangedFile {
            // Need a test file to avoid the "Global" test coverage warning
            path: "src/clean_test.rs".to_string(),
            diff: "@@ -1,1 +1,2 @@\n+println!(\"clean\");\n".to_string(),
        }],
        commit_hash: None,
        pull_request: None,
    };

    let dirty_output = SessionOutput {
        changed_files: vec![ChangedFile {
            // Need a test file to avoid the "Global" test coverage warning
            path: "src/dirty_test.rs".to_string(),
            diff: "@@ -1,1 +1,2 @@\n+let x = option.unwrap();\n".to_string(),
        }],
        commit_hash: None,
        pull_request: None,
    };

    let activities = vec![
        create_mock_activity("activities/1", Some(clean_output)),
        create_mock_activity("activities/2", Some(dirty_output)),
    ];

    let forensics = SessionForensics::new();
    let report = forensics.analyze(&session.name, &activities);

    assert_eq!(report.session_name, "sessions/123");
    assert_eq!(report.activity_findings.len(), 1);

    let finding = &report.activity_findings[0];
    assert_eq!(finding.activity_id, "activities/2");
    assert_eq!(finding.findings.len(), 1);
    assert!(finding.findings[0].message.contains("unwrap()"));
}