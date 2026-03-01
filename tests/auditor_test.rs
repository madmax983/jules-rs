#![cfg(feature = "auditor")]

use jules_rs::{Activity, ActivityStatus, AuditReport, Session, SessionAuditor, ToolUse};

#[test]
fn test_auditor_empty_session() {
    let session = Session {
        name: "sessions/test-1".to_string(),
        id: None,
        prompt: None,
        title: None,
        description: None,
        state: None,
        source_context: None,
        require_plan_approval: None,
        automation_mode: None,
        create_time: None,
        update_time: None,
        outputs: Vec::new(),
        plan: None,
        source: None,
        url: None,
        output: None,
    };
    let activities: Vec<Activity> = Vec::new();

    let auditor = SessionAuditor::new();
    let report: AuditReport = auditor.audit(&session, &activities);

    assert_eq!(report.report.total_activities, 0);
    assert_eq!(report.diagnoses.len(), 0);
}

#[test]
fn test_auditor_populated_session() {
    let session = Session {
        name: "sessions/test-2".to_string(),
        id: None,
        prompt: None,
        title: None,
        description: None,
        state: None,
        source_context: None,
        require_plan_approval: None,
        automation_mode: None,
        create_time: None,
        update_time: None,
        outputs: Vec::new(),
        plan: None,
        source: None,
        url: None,
        output: None,
    };

    let activities = vec![
        Activity {
            name: "sessions/test-2/activities/1".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: Some("MessageUser".to_string()),
            detail: Some("Hello".to_string()),
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
            name: "sessions/test-2/activities/2".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: Some("ToolCall".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
        Activity {
            name: "sessions/test-2/activities/3".to_string(),
            status: Some(ActivityStatus::Failed),
            stage_name: None,
            activity_type: Some("ToolCall".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
    ];

    let auditor = SessionAuditor::new();
    let report: AuditReport = auditor.audit(&session, &activities);

    assert_eq!(report.report.total_activities, 3);
    assert_eq!(
        *report
            .report
            .tool_usage_counts
            .get("read_file")
            .unwrap_or(&0),
        2
    );
}
