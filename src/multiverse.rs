use std::fmt::Write;

use crate::visualizer::escape_label;
use crate::{Activity, Session, SessionComparator};

/// Generates a Mermaid flowchart comparing two diverging sessions.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new multiverse visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart comparing two sessions and their activities.
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
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session Comparison");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e1f5fe,stroke:#0277bd,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let mut prev_node = "Start".to_string();

        let render_node =
            |out: &mut String, prefix: &str, i: usize, activity: &Activity, class_name: &str| {
                let node_id = format!("{prefix}_act_{i}");

                let label = if let Some(tool) = &activity.tool_use {
                    &tool.tool_name
                } else {
                    activity
                        .activity_type
                        .as_deref()
                        .or(activity.stage_name.as_deref())
                        .unwrap_or("Activity")
                };

                let detail = activity.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label.to_string()
                } else {
                    format!("**{label}**<br>{}", escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                } else {
                    let _ = writeln!(out, "    class {node_id} {class_name}");
                }
                node_id
            };

        // Shared History
        if divergence_idx > 0 {
            let _ = writeln!(out, "    subgraph Shared[\"Shared Timeline\"]");
            let _ = writeln!(out, "    direction TB");
            for (i, act) in activities_a.iter().enumerate().take(divergence_idx) {
                let current_node = render_node(&mut out, "shared", i, act, "shared");
                let _ = writeln!(out, "    {prev_node} --> {current_node}");
                prev_node = current_node;
            }
            let _ = writeln!(out, "    end");
        }

        let diverge_node = prev_node;

        // Session A branch
        if divergence_idx < activities_a.len() {
            let mut prev_a = diverge_node.clone();
            let _ = writeln!(
                out,
                "    subgraph BranchA[\"Session A: {}\"]",
                session_a.name
            );
            let _ = writeln!(out, "    direction TB");
            for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
                let current_node = render_node(&mut out, "A", i, act, "branchA");
                let _ = writeln!(out, "    {prev_a} --> {current_node}");
                prev_a = current_node;
            }
            let _ = writeln!(out, "    end");
        }

        // Session B branch
        if divergence_idx < activities_b.len() {
            let mut prev_b = diverge_node;
            let _ = writeln!(
                out,
                "    subgraph BranchB[\"Session B: {}\"]",
                session_b.name
            );
            let _ = writeln!(out, "    direction TB");
            for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
                let current_node = render_node(&mut out, "B", i, act, "branchB");
                let _ = writeln!(out, "    {prev_b} --> {current_node}");
                prev_b = current_node;
            }
            let _ = writeln!(out, "    end");
        }

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
    fn test_multiverse_visualizer() {
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
        assert!(mermaid.contains("shared_act_0{{\"read_file\"}}"));
        assert!(mermaid.contains("class shared_act_0 tool"));
        assert!(mermaid.contains("A_act_1{{\"write_file\"}}"));
        assert!(mermaid.contains("B_act_1{{\"write_file\"}}"));
    }
}
