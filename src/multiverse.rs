use crate::{Activity, Session};
use std::fmt::Write;

/// Visualizes the divergence between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new multiverse visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart comparing two sessions and showing their divergence.
    #[must_use]
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
        let _ = writeln!(out, "    %% Multiverse Divergence");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        // Styling
        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef common fill:#e1bee7,stroke:#8e24aa,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef divergeA fill:#bbdefb,stroke:#1976d2,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef divergeB fill:#c8e6c9,stroke:#388e3c,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef divergeNode fill:#ffcc80,stroke:#f57c00,stroke-width:2px,shape:diamond;"
        );

        let comparator = crate::SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(activities_a.len().min(activities_b.len()));

        // Draw common path
        for (i, act) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("common_{i}");
            let label = act
                .activity_type
                .as_deref()
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let node_text = Self::escape_label(label);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} common");
            prev_node = node_id;
        }

        // Draw divergence node
        if divergence_idx < activities_a.len() || divergence_idx < activities_b.len() {
            let diverge_node = "Divergence";
            let _ = writeln!(out, "    {diverge_node}{{\"Divergence Point\"}}");
            let _ = writeln!(out, "    {prev_node} --> {diverge_node}");
            let _ = writeln!(out, "    class {diverge_node} divergeNode");

            // Branch A
            let mut prev_a = diverge_node.to_string();
            let _ = writeln!(out, "    subgraph BranchA[\"{}\"]", session_a.name);
            for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
                let node_id = format!("a_{i}");
                let label = act
                    .activity_type
                    .as_deref()
                    .or(act.stage_name.as_deref())
                    .unwrap_or("Activity");

                let node_text = Self::escape_label(label);
                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} divergeA");
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
            let end_a = format!(
                "EndA(({:?}))",
                session_a
                    .state
                    .as_ref()
                    .unwrap_or(&crate::SessionState::Unknown)
            );
            let _ = writeln!(out, "    {prev_a} --> {end_a}");

            // Branch B
            let mut prev_b = diverge_node.to_string();
            let _ = writeln!(out, "    subgraph BranchB[\"{}\"]", session_b.name);
            for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
                let node_id = format!("b_{i}");
                let label = act
                    .activity_type
                    .as_deref()
                    .or(act.stage_name.as_deref())
                    .unwrap_or("Activity");

                let node_text = Self::escape_label(label);
                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} divergeB");
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
            let end_b = format!(
                "EndB(({:?}))",
                session_b
                    .state
                    .as_ref()
                    .unwrap_or(&crate::SessionState::Unknown)
            );
            let _ = writeln!(out, "    {prev_b} --> {end_b}");
        } else {
            let end_state = format!(
                "End(({:?}))",
                session_a
                    .state
                    .as_ref()
                    .unwrap_or(&crate::SessionState::Unknown)
            );
            let _ = writeln!(out, "    {prev_node} --> {end_state}");
        }

        out
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
            output: None,
        }
    }

    #[test]
    fn test_multiverse_visualizer_mermaid() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "bar"))),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "baz"))),
        ];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Divergence"));
        assert!(mermaid.contains("sessions/A"));
        assert!(mermaid.contains("sessions/B"));
    }
}
