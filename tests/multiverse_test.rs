#![cfg(feature = "multiverse")]

use jules_rs::{Activity, MultiverseVisualizer, Session, ToolUse};

#[test]
#[allow(clippy::too_many_lines)]
fn generate_flowchart_contains_shared_and_divergent_nodes() {
    let session_a = Session {
        name: "sessions/s-1".to_string(),
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
        url: None,
        source: None,
        plan: None,
        output: None,
        outputs: Vec::new(),
    };

    let session_b = Session {
        name: "sessions/s-2".to_string(),
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
        url: None,
        source: None,
        plan: None,
        output: None,
        outputs: Vec::new(),
    };

    let mut activities_a = Vec::new();
    let mut activities_b = Vec::new();

    // Shared Activity
    let act1 = Activity {
        name: "activities/a-1".to_string(),
        status: None,
        stage_name: None,
        activity_type: Some("ToolUse".to_string()),
        detail: None,
        timestamp: None,
        overview: None,
        plan: None,
        user_input_request: None,
        tool_use: Some(ToolUse {
            tool_name: "list_files".to_string(),
            input: "{}".to_string(),
        }),
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: None,
    };
    activities_a.push(act1.clone());
    activities_b.push(act1);

    // Divergent Activity A
    let act2_a = Activity {
        name: "activities/a-2".to_string(),
        status: None,
        stage_name: None,
        activity_type: Some("ToolUse".to_string()),
        detail: None,
        timestamp: None,
        overview: None,
        plan: None,
        user_input_request: None,
        tool_use: Some(ToolUse {
            tool_name: "read_file".to_string(),
            input: "{\"path\": \"src/main.rs\"}".to_string(),
        }),
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: None,
    };
    activities_a.push(act2_a);

    // Divergent Activity B
    let act2_b = Activity {
        name: "activities/a-3".to_string(),
        status: None,
        stage_name: None,
        activity_type: Some("ToolUse".to_string()),
        detail: None,
        timestamp: None,
        overview: None,
        plan: None,
        user_input_request: None,
        tool_use: Some(ToolUse {
            tool_name: "run_command".to_string(),
            input: "{\"command\": \"cargo test\"}".to_string(),
        }),
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: None,
    };
    activities_b.push(act2_b);

    let visualizer = MultiverseVisualizer::new();
    let output = visualizer.generate(&session_a, &activities_a, &session_b, &activities_b);

    assert!(output.contains("shared_0[\"list_files\"]:::shared"));
    assert!(output.contains("divergence_point{\"Divergence\"}:::divergence"));
    assert!(output.contains("a_1[\"read_file\"]:::a_only"));
    assert!(output.contains("b_1[\"run_command\"]:::b_only"));
}
