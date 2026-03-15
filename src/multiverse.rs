use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::{Activity, Session};

/// Visualizes the divergence between two sessions as a Mermaid flowchart.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart detailing how two sessions diverge.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::similar_names)]
    pub fn generate_flowchart(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e1bee7,stroke:#8e24aa,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef path_a fill:#bbdefb,stroke:#1976d2,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef path_b fill:#ffcc80,stroke:#f57c00,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let first_divergence = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        // Shared Path
        if first_divergence > 0 {
            let _ = writeln!(out, "    subgraph Shared[\"Shared Path\"]");
            let _ = writeln!(out, "    direction TB");
            for i in 0..first_divergence {
                let activity = &activities_a[i];
                let node_id = format!("shared_{i}");

                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} shared");
                prev_node = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        let shared_end = prev_node;

        // Path A
        if first_divergence < activities_a.len() {
            let _ = writeln!(out, "    subgraph PathA[\"Path A ({})\"]", session_a.name);
            let _ = writeln!(out, "    direction TB");
            let mut prev_a = shared_end.clone();

            for i in first_divergence..activities_a.len() {
                let activity = &activities_a[i];
                let node_id = format!("act_a_{i}");

                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} path_a");
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // Path B
        if first_divergence < activities_b.len() {
            let _ = writeln!(out, "    subgraph PathB[\"Path B ({})\"]", session_b.name);
            let _ = writeln!(out, "    direction TB");
            let mut prev_b = shared_end.clone();

            for i in first_divergence..activities_b.len() {
                let activity = &activities_b[i];
                let node_id = format!("act_b_{i}");

                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} path_b");
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        let base_label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(act_type) = &activity.activity_type {
            act_type
        } else if let Some(stage) = &activity.stage_name {
            stage
        } else {
            "Activity"
        };
        Self::escape_label(base_label)
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, SessionState, ToolUse};

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

    fn create_mock_activity(name: &str, tool: Option<(&str, &str)>) -> Activity {
        Activity {
            name: name.to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: Some("test_stage".to_string()),
            activity_type: Some("test_type".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: tool.map(|(tname, input)| ToolUse {
                tool_name: tname.to_string(),
                input: input.to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_visualizer() {
        let session_a = create_mock_session("Session A");
        let session_b = create_mock_session("Session B");

        let activities_a = vec![
            create_mock_activity("1", Some(("shared_tool", "shared_input"))),
            create_mock_activity("2", Some(("tool_a", "input_a"))),
            create_mock_activity("3", None),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("shared_tool", "shared_input"))),
            create_mock_activity("2", Some(("tool_b", "input_b"))),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.generate_flowchart(&session_a, &activities_a, &session_b, &activities_b);

        // Assert shared subgraph
        assert!(mermaid.contains("subgraph Shared[\"Shared Path\"]"));
        assert!(mermaid.contains("shared_0{{\"shared_tool\"}}"));

        // Assert Path A
        assert!(mermaid.contains("subgraph PathA[\"Path A (Session A)\"]"));
        assert!(mermaid.contains("act_a_1{{\"tool_a\"}}"));
        assert!(mermaid.contains("act_a_2[\"test_type\"]"));

        // Assert Path B
        assert!(mermaid.contains("subgraph PathB[\"Path B (Session B)\"]"));
        assert!(mermaid.contains("act_b_1{{\"tool_b\"}}"));
    }
}
