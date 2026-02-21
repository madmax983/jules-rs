#[cfg(feature = "visualizer")]
fn main() {
    use jules_rs::{Activity, Plan, PlanStep, Session, SessionState, SessionVisualizer, ToolUse};

    let session = Session {
        name: "sessions/demo-session".to_string(),
        id: Some("demo-session".to_string()),
        prompt: Some("Create a new snake game".to_string()),
        title: Some("Snake Game Demo".to_string()),
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
                    description: "Initialize project structure".to_string(),
                    status: "completed".to_string(),
                },
                PlanStep {
                    description: "Implement game logic".to_string(),
                    status: "completed".to_string(),
                },
            ],
        }),
        output: None,
        outputs: vec![],
    };

    let activities = vec![
        Activity {
            name: "act/1".to_string(),
            status: None,
            stage_name: Some("Planning".to_string()),
            activity_type: Some("Thinking".to_string()),
            detail: Some("Analyzing user request...".to_string()),
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
            name: "act/2".to_string(),
            status: None,
            stage_name: None,
            activity_type: Some("Tool Use".to_string()),
            detail: Some("Running `cargo new snake_game`".to_string()),
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "bash".to_string(),
                input: "cargo new snake_game".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
    ];

    let viz = SessionVisualizer::new();
    println!("{}", viz.to_mermaid(&session, &activities));
}

#[cfg(not(feature = "visualizer"))]
fn main() {
    println!("This example requires the `visualizer` feature.");
}
