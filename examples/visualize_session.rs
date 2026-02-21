#[cfg(not(feature = "visualizer"))]
fn main() {
    println!("This example requires the 'visualizer' feature.");
}

#[cfg(feature = "visualizer")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use jules_rs::{
        Activity, ActivityStatus, Plan, PlanStep, Session, SessionState, SessionVisualizer, ToolUse,
    };

    let session = Session {
        name: "sessions/demo-123".to_string(),
        id: Some("demo-123".to_string()),
        prompt: Some("Refactor the login module to use OAuth2".to_string()),
        title: Some("OAuth2 Migration".to_string()),
        description: None,
        state: Some(SessionState::Completed),
        source_context: None,
        require_plan_approval: None,
        automation_mode: None,
        create_time: None,
        update_time: None,
        url: Some("https://jules.ai/sessions/demo-123".to_string()),
        source: None,
        plan: Some(Plan {
            steps: vec![
                PlanStep {
                    description: "Analyze existing auth logic".to_string(),
                    status: "completed".to_string(),
                },
                PlanStep {
                    description: "Add OAuth2 dependency".to_string(),
                    status: "completed".to_string(),
                },
                PlanStep {
                    description: "Implement callback handler".to_string(),
                    status: "failed".to_string(),
                },
            ],
        }),
        output: None,
        outputs: vec![],
    };

    let activities = vec![
        Activity {
            name: "sessions/demo-123/activities/a-1".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "{\"path\": \"src/auth.rs\"}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
        Activity {
            name: "sessions/demo-123/activities/a-2".to_string(),
            status: Some(ActivityStatus::Failed),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "run_tests".to_string(),
                input: "{\"filter\": \"test_auth\"}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
    ];

    let visualizer = SessionVisualizer::new();
    let mermaid_graph = visualizer.to_mermaid(&session, &activities)?;

    println!("Copy the following output into a Mermaid viewer (e.g. https://mermaid.live):\n");
    println!("{mermaid_graph}");

    Ok(())
}
