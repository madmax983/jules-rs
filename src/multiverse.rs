use std::fmt::Write;

use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session, SessionComparator};

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
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Multiverse Session B: {}", session_b.name);

        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e6f3ff,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef diverge_a fill:#ffe6e6,stroke:#cc0000,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef diverge_b fill:#e6ffe6,stroke:#00cc00,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(std::cmp::max(activities_a.len(), activities_b.len()));

        let _ = writeln!(out, "    start((Start))");
        let mut prev_node = "start".to_string();

        // Shared Path
        for (i, act) in activities_a.iter().enumerate().take(std::cmp::min(
            divergence_idx,
            std::cmp::min(activities_a.len(), activities_b.len()),
        )) {
            let node_id = format!("shared_{i}");

            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.clone())
                .or_else(|| act.activity_type.clone())
                .or_else(|| act.stage_name.clone())
                .unwrap_or_else(|| "Activity".to_string());

            let detail = act.detail.as_deref().unwrap_or("");
            let escaped_detail = SessionVisualizer::escape_label(detail);
            let node_text = format!("**{label}**<br>{escaped_detail}");

            let _ = writeln!(out, "    {node_id}{{{{\"{node_text}\"}}}}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        let diverge_point_node = prev_node.clone();

        // Divergence A Path
        let mut prev_node_a = diverge_point_node.clone();
        for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("act_a_{i}");

            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.clone())
                .or_else(|| act.activity_type.clone())
                .or_else(|| act.stage_name.clone())
                .unwrap_or_else(|| "Activity".to_string());

            let detail = act.detail.as_deref().unwrap_or("");
            let escaped_detail = SessionVisualizer::escape_label(detail);
            let node_text = format!("**{label}**<br>{escaped_detail}");

            let _ = writeln!(out, "    {node_id}[\"{node_text}\"]");
            let _ = writeln!(out, "    {prev_node_a} -->|Session A| {node_id}");
            let _ = writeln!(out, "    class {node_id} diverge_a");
            prev_node_a = node_id;
        }

        // Divergence B Path
        let mut prev_node_b = diverge_point_node.clone();
        for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("act_b_{i}");

            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.clone())
                .or_else(|| act.activity_type.clone())
                .or_else(|| act.stage_name.clone())
                .unwrap_or_else(|| "Activity".to_string());

            let detail = act.detail.as_deref().unwrap_or("");
            let escaped_detail = SessionVisualizer::escape_label(detail);
            let node_text = format!("**{label}**<br>{escaped_detail}");

            let _ = writeln!(out, "    {node_id}[\"{node_text}\"]");
            let _ = writeln!(out, "    {prev_node_b} -->|Session B| {node_id}");
            let _ = writeln!(out, "    class {node_id} diverge_b");
            prev_node_b = node_id;
        }

        let _ = writeln!(out, "    end_a(((End A)))");
        let _ = writeln!(out, "    {prev_node_a} --> end_a");
        let _ = writeln!(out, "    end_b(((End B)))");
        let _ = writeln!(out, "    {prev_node_b} --> end_b");

        out
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
        tool_name: Option<&str>,
        activity_type: Option<&str>,
        stage_name: Option<&str>,
        detail: Option<&str>,
    ) -> Activity {
        Activity {
            name: name.to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: stage_name.map(std::string::ToString::to_string),
            activity_type: activity_type.map(std::string::ToString::to_string),
            detail: detail.map(std::string::ToString::to_string),
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: tool_name.map(|name| ToolUse {
                tool_name: name.to_string(),
                input: String::new(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_multiverse_mermaid_generation() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let mut act_a_1 =
            create_mock_activity("1", Some("read_file"), None, None, Some("read foo"));
        if let Some(ref mut tu) = act_a_1.tool_use {
            tu.input = "foo".to_string();
        }
        let mut act_a_2 =
            create_mock_activity("2", Some("write_file"), None, None, Some("write bar"));
        if let Some(ref mut tu) = act_a_2.tool_use {
            tu.input = "bar".to_string();
        }
        let activities_a = vec![act_a_1, act_a_2];

        let mut act_b_1 =
            create_mock_activity("1", Some("read_file"), None, None, Some("read foo"));
        if let Some(ref mut tu) = act_b_1.tool_use {
            tu.input = "foo".to_string();
        }
        let mut act_b_2 =
            create_mock_activity("2", Some("write_file"), None, None, Some("write baz"));
        if let Some(ref mut tu) = act_b_2.tool_use {
            tu.input = "baz".to_string();
        }
        let activities_b = vec![act_b_1, act_b_2];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        println!("{mermaid}");

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("%% Multiverse Session A: sessions/A"));
        assert!(mermaid.contains("%% Multiverse Session B: sessions/B"));
        assert!(mermaid.contains("shared_0{{\"**read_file**<br>read foo\"}}"));
        assert!(mermaid.contains("act_a_1[\"**write_file**<br>write bar\"]"));
        assert!(mermaid.contains("act_b_1[\"**write_file**<br>write baz\"]"));
        assert!(mermaid.contains("class shared_0 shared"));
        assert!(mermaid.contains("class act_a_1 diverge_a"));
        assert!(mermaid.contains("class act_b_1 diverge_b"));
    }
}
