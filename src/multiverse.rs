use crate::{Activity, Session, SessionComparator};
use std::fmt::Write;

/// Visualizes the divergence of two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new multiverse visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart mapping out the divergent paths of two sessions.
    #[must_use]
    #[allow(clippy::similar_names)]
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
            "    %% Multiverse: {} vs {}",
            session_a.name, session_b.name
        );

        // Styling
        let _ = writeln!(
            out,
            "    classDef shared fill:#f5f5f5,stroke:#9e9e9e,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#e1f5fe,stroke:#0288d1,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#fce4ec,stroke:#c2185b,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let shared_len = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");
        let _ = writeln!(out, "    class Start shared");

        let mut prev_node = "Start".to_string();

        // Shared Trunk
        for (i, act) in activities_a.iter().enumerate().take(shared_len) {
            let node_id = format!("shared_{i}");
            let label = Self::format_node_text(act);
            let (shape_open, shape_close) = Self::node_shape(act);

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");

            prev_node = node_id;
        }

        let branch_point = prev_node;

        // Branch A
        let mut prev_a = branch_point.clone();
        for (i, act) in activities_a.iter().enumerate().skip(shared_len) {
            let node_id = format!("a_{i}");
            let label = Self::format_node_text(act);
            let (shape_open, shape_close) = Self::node_shape(act);

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_a} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} branchA");

            prev_a = node_id;
        }

        // Branch B
        let mut prev_b = branch_point;
        for (i, act) in activities_b.iter().enumerate().skip(shared_len) {
            let node_id = format!("b_{i}");
            let label = Self::format_node_text(act);
            let (shape_open, shape_close) = Self::node_shape(act);

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_b} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} branchB");

            prev_b = node_id;
        }

        // End Nodes
        if !activities_a.is_empty() || shared_len == 0 {
            let _ = writeln!(
                out,
                "    EndA(({}))",
                session_a.state.as_ref().map_or("Unknown", |s| match s {
                    crate::SessionState::Completed => "Completed A",
                    crate::SessionState::Failed => "Failed A",
                    _ => "Unknown A",
                })
            );
            let _ = writeln!(out, "    {prev_a} --> EndA");
            let _ = writeln!(out, "    class EndA branchA");
        }

        if !activities_b.is_empty() || shared_len == 0 {
            let _ = writeln!(
                out,
                "    EndB(({}))",
                session_b.state.as_ref().map_or("Unknown", |s| match s {
                    crate::SessionState::Completed => "Completed B",
                    crate::SessionState::Failed => "Failed B",
                    _ => "Unknown B",
                })
            );
            let _ = writeln!(out, "    {prev_b} --> EndB");
            let _ = writeln!(out, "    class EndB branchB");
        }

        out
    }

    fn format_node_text(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(activity_type) = &activity.activity_type {
            activity_type
        } else if let Some(stage) = &activity.stage_name {
            stage
        } else {
            "Activity"
        };

        let detail = activity.detail.as_deref().unwrap_or("");

        if detail.is_empty() {
            label.to_string()
        } else {
            let escaped_detail = detail.replace('"', "'").replace('\n', "<br>");
            format!("**{label}**<br>{escaped_detail}")
        }
    }

    fn node_shape(activity: &Activity) -> (&'static str, &'static str) {
        if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ToolUse};

    fn dummy_session(name: &str) -> Session {
        Session {
            name: name.to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: Some(crate::SessionState::Completed),
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

    fn dummy_activity(name: &str, tool: Option<&str>) -> Activity {
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
            tool_use: tool.map(|t| ToolUse {
                tool_name: t.to_string(),
                input: String::new(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    fn test_to_mermaid_split() {
        let session_a = dummy_session("SessionA");
        let session_b = dummy_session("SessionB");

        let shared_act1 = dummy_activity("act1", Some("read_file"));
        let shared_act2 = dummy_activity("act2", Some("grep"));

        let act_a = dummy_activity("act3", Some("write_file"));
        let act_b = dummy_activity("act4", Some("run_bash"));

        let activities_a = vec![shared_act1.clone(), shared_act2.clone(), act_a];
        let activities_b = vec![shared_act1, shared_act2, act_b];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        // Verify shared trunk
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("shared_1{{\"grep\"}}"));
        assert!(mermaid.contains("class shared_0 shared"));

        // Verify Branch A
        assert!(mermaid.contains("a_2{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_1 --> a_2"));
        assert!(mermaid.contains("class a_2 branchA"));

        // Verify Branch B
        assert!(mermaid.contains("b_2{{\"run_bash\"}}"));
        assert!(mermaid.contains("shared_1 --> b_2"));
        assert!(mermaid.contains("class b_2 branchB"));
    }
}
