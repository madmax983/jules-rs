use crate::{Activity, Session, SessionComparator, SessionState};
use std::fmt::Write;

/// Visualizes the divergence between two sessions as a Mermaid diagram.
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
        // Mermaid header
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(
            out,
            "    %% Multiverse: {} vs {}",
            session_a.name, session_b.name
        );

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
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#fff9c4,stroke:#fbc02d,stroke-width:2px,stroke-dasharray: 5 5;"
        );

        // Start Node
        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        // Shared Flow
        for (i, activity) in activities_a.iter().enumerate().take(divergence_index) {
            let node_id = format!("shared_{i}");
            let label = Self::get_activity_label(activity);
            let detail = activity.detail.as_deref().unwrap_or("");
            let node_text = Self::format_node_text(&label, detail);

            let (shape_open, shape_close) = Self::get_activity_shape(activity);
            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_node = node_id;
        }

        let shared_end_node = prev_node.clone();

        // Branch A
        let mut prev_node_a = shared_end_node.clone();
        if divergence_index < activities_a.len() {
            let _ = writeln!(out, "    subgraph SessionA[\"{}\"]", session_a.name);
            for (i, activity) in activities_a.iter().enumerate().skip(divergence_index) {
                let node_id = format!("a_{i}");
                let label = Self::get_activity_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");
                let node_text = Self::format_node_text(&label, detail);

                let (shape_open, shape_close) = Self::get_activity_shape(activity);
                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node_a} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                prev_node_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }
        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_a = "EndA";
        let _ = writeln!(out, "    {end_node_a}(({end_state_a}))");
        let _ = writeln!(out, "    {prev_node_a} --> {end_node_a}");
        Self::style_end_node(&mut out, end_node_a, session_a.state.as_ref());

        // Branch B
        let mut prev_node_b = shared_end_node;
        if divergence_index < activities_b.len() {
            let _ = writeln!(out, "    subgraph SessionB[\"{}\"]", session_b.name);
            for (i, activity) in activities_b.iter().enumerate().skip(divergence_index) {
                let node_id = format!("b_{i}");
                let label = Self::get_activity_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");
                let node_text = Self::format_node_text(&label, detail);

                let (shape_open, shape_close) = Self::get_activity_shape(activity);
                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node_b} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                prev_node_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }
        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_b = "EndB";
        let _ = writeln!(out, "    {end_node_b}(({end_state_b}))");
        let _ = writeln!(out, "    {prev_node_b} --> {end_node_b}");
        Self::style_end_node(&mut out, end_node_b, session_b.state.as_ref());

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        if let Some(tool) = &activity.tool_use {
            tool.tool_name.clone()
        } else {
            activity
                .activity_type
                .clone()
                .or_else(|| activity.stage_name.clone())
                .unwrap_or_else(|| "Activity".to_string())
        }
    }

    fn get_activity_shape(activity: &Activity) -> (&'static str, &'static str) {
        if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
        }
    }

    fn format_node_text(label: &str, detail: &str) -> String {
        if detail.is_empty() {
            label.to_string()
        } else {
            format!("**{label}**<br>{}", Self::escape_label(detail))
        }
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }

    fn style_end_node(out: &mut String, node_id: &str, state: Option<&SessionState>) {
        match state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {node_id} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {node_id} error");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ToolUse};

    fn create_mock_session(name: &str, state: SessionState) -> Session {
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
    fn test_multiverse_mermaid() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Failed);

        let shared_activity = create_mock_activity("1", Some(("read_file", "foo")));
        let act_a = create_mock_activity("2", Some(("write_file", "bar")));
        let act_b = create_mock_activity("2", Some(("write_file", "baz")));

        let activities_a = vec![shared_activity.clone(), act_a];
        let activities_b = vec![shared_activity, act_b];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("subgraph SessionA"));
        assert!(mermaid.contains("subgraph SessionB"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> a_1"));
        assert!(mermaid.contains("shared_0 --> b_1"));
        assert!(mermaid.contains("EndA((Completed))"));
        assert!(mermaid.contains("EndB((Failed))"));
    }
}
