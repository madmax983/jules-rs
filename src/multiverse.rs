use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session, SessionState};

#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    pub fn to_mermaid(
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
            "    classDef branchA fill:#fff3e0,stroke:#e65100,stroke-width:2px,stroke-dasharray: 5 5;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#f3e5f5,stroke:#4a148c,stroke-width:2px,stroke-dasharray: 5 5;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let div_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        for (i, activity) in activities_a.iter().take(div_idx).enumerate() {
            let node_id = format!("shared_{i}");
            let label = Self::get_label(activity);
            let detail = activity.detail.as_deref().unwrap_or("");

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!("**{label}**<br>{}", SessionVisualizer::escape_label(detail))
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            prev_node = node_id;
        }

        let shared_end = prev_node.clone();

        if div_idx < activities_a.len() {
            let _ = writeln!(out, "    subgraph Session_A[\"{}\"]", session_a.name);
            let _ = writeln!(out, "    direction TB");
            let mut branch_prev = shared_end.clone();

            for (i, activity) in activities_a.iter().enumerate().skip(div_idx) {
                let node_id = format!("act_a_{i}");
                let label = Self::get_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label.to_string()
                } else {
                    format!("**{label}**<br>{}", SessionVisualizer::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {branch_prev} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                branch_prev = node_id;
            }

            let end_state = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let _ = writeln!(out, "    End_A(({end_state}))");
            let _ = writeln!(out, "    {branch_prev} --> End_A");

            match session_a.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class End_A success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class End_A error");
                }
                _ => {}
            }
            let _ = writeln!(out, "    class Session_A branchA");
            let _ = writeln!(out, "    end");
        }

        if div_idx < activities_b.len() {
            let _ = writeln!(out, "    subgraph Session_B[\"{}\"]", session_b.name);
            let _ = writeln!(out, "    direction TB");
            let mut branch_prev = shared_end.clone();

            for (i, activity) in activities_b.iter().enumerate().skip(div_idx) {
                let node_id = format!("act_b_{i}");
                let label = Self::get_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label.to_string()
                } else {
                    format!("**{label}**<br>{}", SessionVisualizer::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {branch_prev} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                branch_prev = node_id;
            }

            let end_state = session_b
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let _ = writeln!(out, "    End_B(({end_state}))");
            let _ = writeln!(out, "    {branch_prev} --> End_B");

            match session_b.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class End_B success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class End_B error");
                }
                _ => {}
            }
            let _ = writeln!(out, "    class Session_B branchB");
            let _ = writeln!(out, "    end");
        }

        if div_idx == activities_a.len() && div_idx == activities_b.len() {
            let end_state = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let _ = writeln!(out, "    End(({end_state}))");
            let _ = writeln!(out, "    {shared_end} --> End");

            match session_a.state {
                Some(SessionState::Completed) => {
                    let _ = writeln!(out, "    class End success");
                }
                Some(SessionState::Failed) => {
                    let _ = writeln!(out, "    class End error");
                }
                _ => {}
            }
        }

        out
    }

    fn get_label(activity: &Activity) -> &str {
        if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(a_type) = &activity.activity_type {
            a_type
        } else if let Some(s_name) = &activity.stage_name {
            s_name
        } else {
            "Activity"
        }
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
    #[allow(clippy::similar_names)]
    fn test_multiverse_divergence() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        // Shared activity
        let act0 = create_mock_activity("0", Some(("read_file", "Cargo.toml")), None, None);

        // Divergent activities
        let act_a_1 = create_mock_activity("1_a", Some(("write_file", "src/main.rs")), None, None);
        let act_b_1 = create_mock_activity("1_b", Some(("write_file", "src/lib.rs")), None, None);

        let activities_a = vec![act0.clone(), act_a_1];
        let activities_b = vec![act0, act_b_1];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("act_a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("act_b_1{{\"write_file\"}}"));
    }
}
