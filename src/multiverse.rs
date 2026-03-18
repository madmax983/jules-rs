use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session, SessionState};

/// Visualizes the divergence between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new multiverse visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart detailing how two sessions diverge.
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let divergence_index = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session Comparison");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        // Styling
        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef success fill:#e1f5fe,stroke:#0277bd,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef error fill:#ffebee,stroke:#c62828,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef plan fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef a_only fill:#fff3e0,stroke:#e65100,stroke-width:2px,stroke-dasharray: 5 5;"
        );
        let _ = writeln!(
            out,
            "    classDef b_only fill:#f3e5f5,stroke:#4a148c,stroke-width:2px,stroke-dasharray: 5 5;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let mut prev_node = "Start".to_string();

        // Shared Path
        if divergence_index > 0 {
            let _ = writeln!(out, "    subgraph Shared[\"Shared Path\"]");
            let _ = writeln!(out, "    direction TB");
            for (i, activity) in activities_a.iter().enumerate().take(divergence_index) {
                let node_id = format!("shared_{i}");
                let node_text = Self::format_activity_label(activity);

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node} --> {node_id}");
                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }
                prev_node = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        let shared_end_node = prev_node;

        // Divergence A
        if divergence_index < activities_a.len() {
            let _ = writeln!(
                out,
                "    subgraph Divergence_A[\"Session A: {}\"]",
                session_a.name
            );
            let _ = writeln!(out, "    direction TB");
            let mut prev_a = shared_end_node.clone();
            for (i, activity) in activities_a.iter().enumerate().skip(divergence_index) {
                let node_id = format!("a_{i}");
                let node_text = Self::format_activity_label(activity);

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} a_only");
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");

            let end_state_a = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_id_a = "End_A";
            let _ = writeln!(out, "    {end_id_a}(({end_state_a}))");
            let _ = writeln!(out, "    {prev_a} --> {end_id_a}");
            match session_a.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class {end_id_a} success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class {end_id_a} error");
                }
                _ => {}
            }
        } else {
            let end_state_a = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_id_a = "End_A";
            let _ = writeln!(out, "    {end_id_a}(({end_state_a}))");
            let _ = writeln!(out, "    {shared_end_node} --> {end_id_a}");
            match session_a.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class {end_id_a} success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class {end_id_a} error");
                }
                _ => {}
            }
        }

        // Divergence B
        if divergence_index < activities_b.len() {
            let _ = writeln!(
                out,
                "    subgraph Divergence_B[\"Session B: {}\"]",
                session_b.name
            );
            let _ = writeln!(out, "    direction TB");
            let mut prev_b = shared_end_node.clone();
            for (i, activity) in activities_b.iter().enumerate().skip(divergence_index) {
                let node_id = format!("b_{i}");
                let node_text = Self::format_activity_label(activity);

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} b_only");
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");

            let end_state_b = session_b
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_id_b = "End_B";
            let _ = writeln!(out, "    {end_id_b}(({end_state_b}))");
            let _ = writeln!(out, "    {prev_b} --> {end_id_b}");
            match session_b.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class {end_id_b} success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class {end_id_b} error");
                }
                _ => {}
            }
        } else {
            let end_state_b = session_b
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_id_b = "End_B";
            let _ = writeln!(out, "    {end_id_b}(({end_state_b}))");
            let _ = writeln!(out, "    {shared_end_node} --> {end_id_b}");
            match session_b.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class {end_id_b} success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class {end_id_b} error");
                }
                _ => {}
            }
        }

        out
    }

    fn format_activity_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(act_type) = &activity.activity_type {
            act_type
        } else if let Some(stage) = &activity.stage_name {
            stage
        } else {
            "Activity"
        };

        let detail = activity.detail.as_deref().unwrap_or("");

        if detail.is_empty() {
            SessionVisualizer::escape_label(label)
        } else {
            format!(
                "**{}**<br>{}",
                SessionVisualizer::escape_label(label),
                SessionVisualizer::escape_label(detail)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ToolUse};

    fn mock_session(name: &str, state: SessionState) -> Session {
        Session {
            name: name.to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: Some(state),
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

    fn mock_activity(name: &str, tool: Option<&str>) -> Activity {
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
            tool_use: tool.map(|t| ToolUse {
                tool_name: t.to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    fn test_multiverse_visualize() {
        let session_a = mock_session("Session A", SessionState::Completed);
        let session_b = mock_session("Session B", SessionState::Failed);

        let activities_a = vec![
            mock_activity("shared_act", Some("read_file")),
            mock_activity("divergent_a", Some("write_file")),
        ];

        let activities_b = vec![
            mock_activity("shared_act", Some("read_file")), // Same tool name and input => shared
            mock_activity("divergent_b", Some("run_test")), // Divergence starts here
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        println!("{mermaid}");

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("b_1{{\"run_test\"}}"));
        assert!(mermaid.contains("End_A((Completed))"));
        assert!(mermaid.contains("End_B((Failed))"));
        assert!(mermaid.contains("shared_0 --> a_1"));
        assert!(mermaid.contains("shared_0 --> b_1"));
        assert!(mermaid.contains("a_1 --> End_A"));
        assert!(mermaid.contains("b_1 --> End_B"));
    }
}
