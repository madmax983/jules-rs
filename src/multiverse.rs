use crate::{Activity, Session, SessionComparator, SessionState};
use std::fmt::Write;

/// Visualizes the divergence of two sessions as a Mermaid diagram.
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
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
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
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let divergence_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        // Shared Path
        for (i, activity) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_act_{i}");
            let label = Self::activity_label(activity);

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_node = node_id;
        }

        let shared_end_node = prev_node;

        // Branch A
        let _ = writeln!(out, "    subgraph BranchA[\"{}\"]", session_a.name);
        let mut prev_a = shared_end_node.clone();
        for (i, activity) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_act_{i}");
            let label = Self::activity_label(activity);

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_a} --> {node_id}");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_a = node_id;
        }

        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_a = "EndA";
        let _ = writeln!(out, "    {end_node_a}(({end_state_a}))");
        let _ = writeln!(out, "    {prev_a} --> {end_node_a}");
        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {end_node_a} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {end_node_a} error");
            }
            _ => {}
        }
        let _ = writeln!(out, "    end");

        // Branch B
        let _ = writeln!(out, "    subgraph BranchB[\"{}\"]", session_b.name);
        let mut prev_b = shared_end_node;
        for (i, activity) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_act_{i}");
            let label = Self::activity_label(activity);

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_b} --> {node_id}");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_b = node_id;
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_b = "EndB";
        let _ = writeln!(out, "    {end_node_b}(({end_state_b}))");
        let _ = writeln!(out, "    {prev_b} --> {end_node_b}");
        match session_b.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {end_node_b} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {end_node_b} error");
            }
            _ => {}
        }
        let _ = writeln!(out, "    end");

        out
    }

    fn activity_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(atype) = &activity.activity_type {
            atype
        } else if let Some(stage) = &activity.stage_name {
            stage
        } else {
            "Activity"
        };

        let mut final_label = label.to_string();
        if let Some(detail) = &activity.detail {
            if !detail.is_empty() {
                final_label = format!("**{label}**<br>{}", Self::escape_label(detail));
            }
        }

        final_label
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ToolUse};

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
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_to_mermaid() {
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

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));

        // Shared node
        assert!(mermaid.contains("shared_act_0{{\"read_file\"}}"));
        assert!(mermaid.contains("Start --> shared_act_0"));
        assert!(mermaid.contains("class shared_act_0 tool"));

        // Branch A
        assert!(mermaid.contains("subgraph BranchA[\"sessions/A\"]"));
        assert!(mermaid.contains("a_act_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_act_0 --> a_act_1"));
        assert!(mermaid.contains("a_act_1 --> EndA"));
        assert!(mermaid.contains("EndA((Completed))"));

        // Branch B
        assert!(mermaid.contains("subgraph BranchB[\"sessions/B\"]"));
        assert!(mermaid.contains("b_act_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_act_0 --> b_act_1"));
        assert!(mermaid.contains("b_act_1 --> EndB"));
        assert!(mermaid.contains("EndB((Completed))"));
    }
}
