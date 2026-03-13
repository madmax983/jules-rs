use crate::{Activity, Session, SessionComparator};
use std::fmt::Write;

/// Visualizes how two sessions diverge using a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart detailing the shared path and divergence point of two sessions.
    #[must_use]
    #[allow(clippy::similar_names)]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session Comparison");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        let _ = writeln!(
            out,
            "    classDef shared fill:#f9f9f9,stroke:#333,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef node_a fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef node_b fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        let divergence_index = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        // Shared Path
        if divergence_index > 0 {
            let _ = writeln!(out, "    subgraph Shared[\"Shared Timeline\"]");
            let _ = writeln!(out, "    direction TB");
            for (i, act) in activities_a.iter().enumerate().take(divergence_index) {
                let node_id = format!("shared_{i}");
                let label = Self::get_activity_label(act);

                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node} --> {node_id}");

                if act.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                } else {
                    let _ = writeln!(out, "    class {node_id} shared");
                }
                prev_node = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        let shared_end_node = prev_node;

        // Path A
        if divergence_index < activities_a.len() {
            let _ = writeln!(
                out,
                "    subgraph Timeline_A[\"Session A: {}\"]",
                session_a.name
            );
            let _ = writeln!(out, "    direction TB");
            let mut prev_a = shared_end_node.clone();

            for (i, act) in activities_a.iter().enumerate().skip(divergence_index) {
                let node_id = format!("a_{i}");
                let label = Self::get_activity_label(act);

                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");

                if act.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                } else {
                    let _ = writeln!(out, "    class {node_id} node_a");
                }
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // Path B
        if divergence_index < activities_b.len() {
            let _ = writeln!(
                out,
                "    subgraph Timeline_B[\"Session B: {}\"]",
                session_b.name
            );
            let _ = writeln!(out, "    direction TB");
            let mut prev_b = shared_end_node;

            for (i, act) in activities_b.iter().enumerate().skip(divergence_index) {
                let node_id = format!("b_{i}");
                let label = Self::get_activity_label(act);

                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");

                if act.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                } else {
                    let _ = writeln!(out, "    class {node_id} node_b");
                }
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            tool.tool_name.clone()
        } else if let Some(act_type) = &activity.activity_type {
            act_type.clone()
        } else if let Some(stage) = &activity.stage_name {
            stage.clone()
        } else {
            "Activity".to_string()
        };
        Self::escape_label(&label)
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

    fn create_mock_activity(
        name: &str,
        tool: Option<(&str, &str)>,
        activity_type: Option<&str>,
        stage_name: Option<&str>,
    ) -> Activity {
        Activity {
            name: name.to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: stage_name.map(std::string::ToString::to_string),
            activity_type: activity_type.map(std::string::ToString::to_string),
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
            output: None,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_identical_sessions() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), None, None),
            create_mock_activity("2", None, Some("Think"), None),
        ];

        let activities_b = activities_a.clone();

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("subgraph Shared[\"Shared Timeline\"]"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("shared_1[\"Think\"]"));
        assert!(!mermaid.contains("Timeline_A"));
        assert!(!mermaid.contains("Timeline_B"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_divergent_sessions() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), None, None),
            create_mock_activity("2", Some(("write_file", "bar")), None, None),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), None, None),
            create_mock_activity("2", Some(("run_cmd", "baz")), None, None),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("subgraph Shared[\"Shared Timeline\"]"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));

        assert!(mermaid.contains("subgraph Timeline_A"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> a_1"));

        assert!(mermaid.contains("subgraph Timeline_B"));
        assert!(mermaid.contains("b_1{{\"run_cmd\"}}"));
        assert!(mermaid.contains("shared_0 --> b_1"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_completely_different_sessions() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![create_mock_activity(
            "1",
            Some(("read_file", "foo")),
            None,
            None,
        )];

        let activities_b = vec![create_mock_activity(
            "1",
            Some(("write_file", "bar")),
            None,
            None,
        )];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(!mermaid.contains("subgraph Shared"));

        assert!(mermaid.contains("subgraph Timeline_A"));
        assert!(mermaid.contains("Start --> a_0"));
        assert!(mermaid.contains("a_0{{\"read_file\"}}"));

        assert!(mermaid.contains("subgraph Timeline_B"));
        assert!(mermaid.contains("Start --> b_0"));
        assert!(mermaid.contains("b_0{{\"write_file\"}}"));
    }
}
