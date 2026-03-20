use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session, SessionComparator, SessionState};
use std::fmt::Write;

/// Visualizes the divergence between two sessions as a Mermaid flowchart.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart comparing two sessions.
    #[must_use]
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        // Mermaid header
        let _ = writeln!(out, "graph TD");
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
            "    classDef shared fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef sessionA fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef sessionB fill:#f3e5f5,stroke:#4a148c,stroke-width:2px;"
        );

        // Start Node
        let _ = writeln!(out, "    Start((Start))");

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let div_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let mut prev_node = "Start".to_string();

        // Shared Path
        for (i, act) in activities_a.iter().enumerate().take(div_idx) {
            let node_id = format!("shared_act_{i}");
            let label = Self::format_node_label(act);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");

            prev_node = node_id;
        }

        let shared_end_node = prev_node;

        // Session A Divergent Path
        let mut prev_node_a = shared_end_node.clone();
        for (i, act) in activities_a.iter().enumerate().skip(div_idx) {
            let node_id = format!("A_act_{i}");
            let label = Self::format_node_label(act);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"[A] {label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node_a} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} sessionA");

            prev_node_a = node_id;
        }

        // Session B Divergent Path
        let mut prev_node_b = shared_end_node.clone();
        for (i, act) in activities_b.iter().enumerate().skip(div_idx) {
            let node_id = format!("B_act_{i}");
            let label = Self::format_node_label(act);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"[B] {label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node_b} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} sessionB");

            prev_node_b = node_id;
        }

        // End Nodes
        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    End_A(({end_state_a}))");
        let _ = writeln!(out, "    {prev_node_a} --> End_A");

        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class End_A success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class End_A error");
            }
            _ => {}
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    End_B(({end_state_b}))");
        let _ = writeln!(out, "    {prev_node_b} --> End_B");

        match session_b.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class End_B success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class End_B error");
            }
            _ => {}
        }

        out
    }

    fn format_node_label(act: &Activity) -> String {
        let label_text = act
            .tool_use
            .as_ref()
            .map(|t| t.tool_name.clone())
            .or_else(|| act.activity_type.clone())
            .or_else(|| act.stage_name.clone())
            .unwrap_or_else(|| "Activity".to_string());

        SessionVisualizer::escape_label(&label_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState, ToolUse};

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
    fn test_identical_sessions() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![create_mock_activity(
            "1",
            Some(("read_file", "foo")),
            vec![],
        )];
        let activities_b = vec![create_mock_activity(
            "1",
            Some(("read_file", "foo")),
            vec![],
        )];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));
        assert!(mermaid.contains("shared_act_0"));
        assert!(!mermaid.contains("A_act"));
        assert!(!mermaid.contains("B_act"));
        assert!(mermaid.contains("End_A((Completed))"));
        assert!(mermaid.contains("End_B((Completed))"));
    }

    #[test]
    fn test_divergent_sessions() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "bar")), vec![]),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "baz")), vec![]),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));
        assert!(mermaid.contains("shared_act_0"));
        assert!(mermaid.contains("A_act_1"));
        assert!(mermaid.contains("B_act_1"));
        assert!(mermaid.contains("End_A((Completed))"));
        assert!(mermaid.contains("End_B((Completed))"));
    }
}
