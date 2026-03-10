use crate::{Activity, Session, SessionComparator};
use std::fmt::Write;

/// Visualizes a comparison between two sessions as a Mermaid diagram.
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
    pub fn visualize(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Comparison");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        // Styling
        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathA fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathB fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");

        let mut prev_node = "Start".to_string();

        // 1. Shared Path
        for (i, act) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_act_{i}");
            let label = Self::format_activity_label(act);

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

        let shared_tail = prev_node;

        // 2. Path A
        let mut prev_a = shared_tail.clone();
        for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_act_{i}");
            let label = Self::format_activity_label(act);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_a} -->|Session A| {node_id}");
            let _ = writeln!(out, "    class {node_id} pathA");

            prev_a = node_id;
        }

        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_a = "End_A";
        let _ = writeln!(out, "    {end_node_a}(({end_state_a}))");
        let _ = writeln!(out, "    {prev_a} --> {end_node_a}");
        let _ = writeln!(out, "    class {end_node_a} pathA");

        // 3. Path B
        let mut prev_b = shared_tail;
        for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_act_{i}");
            let label = Self::format_activity_label(act);

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_b} -->|Session B| {node_id}");
            let _ = writeln!(out, "    class {node_id} pathB");

            prev_b = node_id;
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let end_node_b = "End_B";
        let _ = writeln!(out, "    {end_node_b}(({end_state_b}))");
        let _ = writeln!(out, "    {prev_b} --> {end_node_b}");
        let _ = writeln!(out, "    class {end_node_b} pathB");

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

        label.replace('"', "'")
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
    fn test_multiverse_visualizer_generates_mermaid() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), None, None),
            create_mock_activity("2", Some(("write_file", "bar")), None, None),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), None, None),
            create_mock_activity("2", Some(("write_file", "baz")), None, None),
        ];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.visualize(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));

        // They share the first activity
        assert!(mermaid.contains("shared_act_0"));

        // They diverge on the second activity
        assert!(mermaid.contains("a_act_1"));
        assert!(mermaid.contains("b_act_1"));

        // It should use proper labelling prioritization: tool_use.tool_name > activity_type > stage_name
        assert!(mermaid.contains("read_file"));
        assert!(mermaid.contains("write_file"));
    }
}
