#![cfg(feature = "multiverse")]

use jules_rs::MultiverseVisualizer;
use jules_rs::{
    Activity, ActivityStatus, ChangedFile, Session, SessionOutput, SessionState, ToolUse,
};

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

fn create_mock_activity(
    name: &str,
    tool: Option<(&str, &str)>,
    output_files: Vec<&str>,
) -> Activity {
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
        tool_use: tool.map(|(name, input)| ToolUse {
            tool_name: name.to_string(),
            input: input.to_string(),
        }),
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: if output_files.is_empty() {
            None
        } else {
            Some(SessionOutput {
                changed_files: output_files
                    .into_iter()
                    .map(|p| ChangedFile {
                        path: p.to_string(),
                        diff: String::new(),
                    })
                    .collect(),
                commit_hash: None,
                pull_request: None,
            })
        },
    }
}

#[test]
fn test_multiverse_visualizer_mermaid() {
    let session_a = create_mock_session("sessions/A");
    let session_b = create_mock_session("sessions/B");

    let activities_a = vec![
        create_mock_activity("1", Some(("read_file", "foo")), vec![]),
        create_mock_activity("2", Some(("write_file", "bar")), vec!["src/main.rs"]),
    ];

    let activities_b = vec![
        create_mock_activity("1", Some(("read_file", "foo")), vec![]),
        create_mock_activity("2", Some(("write_file", "baz")), vec!["src/lib.rs"]),
    ];

    let visualizer = MultiverseVisualizer::new();
    let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

    println!("{mermaid}");

    // General Structure
    assert!(mermaid.contains("graph TD"));
    assert!(mermaid.contains("Start((Start))"));

    // Expected Subgraphs
    assert!(mermaid.contains("subgraph Shared"));
    assert!(mermaid.contains("subgraph SessionA"));
    assert!(mermaid.contains("subgraph SessionB"));

    // Correct tool shape and priority text handling
    assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
    assert!(mermaid.contains("act_a_1{{\"write_file\"}}"));
    assert!(mermaid.contains("act_b_1{{\"write_file\"}}"));
}
