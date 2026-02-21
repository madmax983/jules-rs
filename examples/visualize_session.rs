#[cfg(feature = "visualizer")]
fn main() {
    use jules_rs::{
        Activity, ActivityStatus, Overview, Plan, PlanStep, Session, SessionState, SessionVisualizer,
        ToolUse,
    };

    let session = Session {
        name: "sessions/demo-123".into(),
        id: Some("demo-123".into()),
        prompt: Some("Create a snake game in Python".into()),
        title: Some("Snake Game Demo".into()),
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
                    description: "Initialize project structure".into(),
                    status: "completed".into(),
                },
                PlanStep {
                    description: "Implement game loop".into(),
                    status: "completed".into(),
                },
                PlanStep {
                    description: "Add food and collision logic".into(),
                    status: "completed".into(),
                },
            ],
        }),
        output: None,
        outputs: vec![],
    };

    let activities = vec![
        Activity {
            name: "act-1".into(),
            status: Some(ActivityStatus::Success),
            stage_name: Some("Planning".into()),
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: Some(Overview {
                state: None,
                thoughts: Some("I will start by creating a main.py file.".into()),
                summary: None,
            }),
            plan: None,
            user_input_request: None,
            tool_use: None,
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        },
        Activity {
            name: "act-2".into(),
            status: Some(ActivityStatus::Success),
            stage_name: Some("Execution".into()),
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "write_file".into(),
                input: "{\"filepath\": \"main.py\", ...}".into(),
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
    println!("This example requires the 'visualizer' feature.");
    println!("Run with: cargo run --example visualize_session --features visualizer");
}
