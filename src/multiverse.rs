use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::{Activity, Session};

/// Visualizes the divergence between two sessions as a Mermaid diagram.
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
    #[allow(clippy::too_many_lines)]
    pub fn to_mermaid(
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
            "    classDef shared fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathA fill:#e1f5fe,stroke:#0277bd,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathB fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        let div_idx = report
            .first_divergence_index
            .unwrap_or(activities_a.len().min(activities_b.len()));

        // Draw Shared path
        for (i, act) in activities_a.iter().enumerate().take(div_idx) {
            let node_id = format!("shared_{i}");
            let label = Self::get_label(act);

            let _ = writeln!(out, "    {node_id}[\"{label}\"]");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        let diverge_node = prev_node;

        // Subgraph A
        if div_idx < activities_a.len() {
            let _ = writeln!(out, "    subgraph Path A");
            let mut prev_a = diverge_node.clone();
            for (i, act) in activities_a.iter().enumerate().skip(div_idx) {
                let node_id = format!("a_{i}");
                let label = Self::get_label(act);

                let _ = writeln!(out, "    {node_id}[\"{label}\"]");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} pathA");
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // Subgraph B
        if div_idx < activities_b.len() {
            let _ = writeln!(out, "    subgraph Path B");
            let mut prev_b = diverge_node;
            for (i, act) in activities_b.iter().enumerate().skip(div_idx) {
                let node_id = format!("b_{i}");
                let label = Self::get_label(act);

                let _ = writeln!(out, "    {node_id}[\"{label}\"]");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} pathB");
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        out
    }

    fn get_label(activity: &Activity) -> String {
        let label = activity
            .activity_type
            .as_deref()
            .or(activity.stage_name.as_deref())
            .unwrap_or("Activity");
        let detail = activity.detail.as_deref().unwrap_or("");

        let text = if detail.is_empty() {
            label.to_string()
        } else {
            format!("{label}<br>{detail}")
        };

        text.replace('"', "'").replace('\n', "<br>")
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
    fn test_multiverse_mermaid_generation() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "bar"))),
            create_mock_activity("3", Some(("commit", "commit A"))),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "baz"))),
            create_mock_activity("3", Some(("commit", "commit B"))),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("subgraph Path A"));
        assert!(mermaid.contains("subgraph Path B"));
    }
}
