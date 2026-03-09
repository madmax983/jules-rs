use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::{Activity, Session, SessionState};

/// Visualizes the divergence between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart detailing how two sessions diverge based on tool execution.
    #[must_use]
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
            "    classDef timelineA fill:#fff3e0,stroke:#e65100,stroke-width:2px,stroke-dasharray: 5 5;"
        );
        let _ = writeln!(
            out,
            "    classDef timelineB fill:#f3e5f5,stroke:#4a148c,stroke-width:2px,stroke-dasharray: 5 5;"
        );

        // Start Node
        let _ = writeln!(out, "    Start((Start))");

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let div_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let mut prev_node = "Start".to_string();

        // Shared timeline
        for (i, act) in activities_a.iter().enumerate().take(div_idx) {
            let node_id = format!("shared_{i}");

            let label = Self::get_activity_label(act);
            let text = Self::format_node_text(act, &label);

            let (shape_open, shape_close) = Self::get_node_shapes(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"{text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");

            if act.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_node = node_id;
        }

        let shared_end = prev_node;

        // Timeline A
        let mut prev_a = shared_end.clone();
        for (i, act) in activities_a.iter().enumerate().skip(div_idx) {
            let node_id = format!("act_a_{i}");

            let label = Self::get_activity_label(act);
            let text = Self::format_node_text(act, &label);

            let (shape_open, shape_close) = Self::get_node_shapes(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"[A] {text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_a} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} timelineA");

            prev_a = node_id;
        }

        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({end_state_a} A))");
        let _ = writeln!(out, "    {prev_a} --> EndA");
        Self::style_end_node(&mut out, "EndA", session_a.state.as_ref());

        // Timeline B
        let mut prev_b = shared_end;
        for (i, act) in activities_b.iter().enumerate().skip(div_idx) {
            let node_id = format!("act_b_{i}");

            let label = Self::get_activity_label(act);
            let text = Self::format_node_text(act, &label);

            let (shape_open, shape_close) = Self::get_node_shapes(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"[B] {text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_b} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} timelineB");

            prev_b = node_id;
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({end_state_b} B))");
        let _ = writeln!(out, "    {prev_b} --> EndB");
        Self::style_end_node(&mut out, "EndB", session_b.state.as_ref());

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        if let Some(tool) = &activity.tool_use {
            return tool.tool_name.clone();
        }
        activity
            .activity_type
            .as_deref()
            .or(activity.stage_name.as_deref())
            .unwrap_or("Activity")
            .to_string()
    }

    fn format_node_text(activity: &Activity, label: &str) -> String {
        let detail = activity.detail.as_deref().unwrap_or("");
        if detail.is_empty() {
            label.to_string()
        } else {
            format!("**{label}**<br>{}", Self::escape_label(detail))
        }
    }

    fn get_node_shapes(activity: &Activity) -> (&'static str, &'static str) {
        if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
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
    fn test_multiverse_visualizer_divergence() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "bar"))),
            create_mock_activity("3", Some(("run_in_bash_session", "cargo check"))),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "baz"))),
        ];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        // Shared pipeline
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("Start --> shared_0"));

        // Divergence logic
        // Activity 2 differs ("write_file" with different inputs), so divergence index is 1.
        assert!(mermaid.contains("act_a_1{{\"[A] write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> act_a_1"));
        assert!(mermaid.contains("act_a_2{{\"[A] run_in_bash_session\"}}"));
        assert!(mermaid.contains("act_a_1 --> act_a_2"));
        assert!(mermaid.contains("act_a_2 --> EndA"));

        assert!(mermaid.contains("act_b_1{{\"[B] write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> act_b_1"));
        assert!(mermaid.contains("act_b_1 --> EndB"));

        // End nodes
        assert!(mermaid.contains("EndA((Completed A))"));
        assert!(mermaid.contains("EndB((Completed B))"));
        assert!(mermaid.contains("class EndA success"));
        assert!(mermaid.contains("class EndB success"));
    }
}
