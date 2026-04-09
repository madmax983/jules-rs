use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::{Activity, Session, SessionState};

/// Generates a Mermaid flowchart detailing how two sessions diverge.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart comparing two sessions.
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
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        let _ = writeln!(
            out,
            "    classDef shared fill:#e0e0e0,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef node_a fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef node_b fill:#fbe9e7,stroke:#d84315,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef success fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef error fill:#ffebee,stroke:#c62828,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_shared = "Start".to_string();

        // Shared timeline
        for (i, act) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_{i}");
            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.as_str())
                .or(act.activity_type.as_deref())
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!(
                    "**{label}**<br>{}",
                    crate::visualizer::SessionVisualizer::escape_label(detail)
                )
            };

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_shared} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_shared = node_id;
        }

        // Branch A
        let mut prev_a = prev_shared.clone();
        for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_{i}");
            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.as_str())
                .or(act.activity_type.as_deref())
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!(
                    "**{label}**<br>{}",
                    crate::visualizer::SessionVisualizer::escape_label(detail)
                )
            };

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let edge_label = if prev_a.starts_with("shared_") || prev_a == "Start" {
                "|Session A| "
            } else {
                ""
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_a} -->{edge_label}{node_id}");
            let _ = writeln!(out, "    class {node_id} node_a");
            prev_a = node_id;
        }

        // Branch B
        let mut prev_b = prev_shared;
        for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_{i}");
            let label = act
                .tool_use
                .as_ref()
                .map(|t| t.tool_name.as_str())
                .or(act.activity_type.as_deref())
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!(
                    "**{label}**<br>{}",
                    crate::visualizer::SessionVisualizer::escape_label(detail)
                )
            };

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let edge_label = if prev_b.starts_with("shared_") || prev_b == "Start" {
                "|Session B| "
            } else {
                ""
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_b} -->{edge_label}{node_id}");
            let _ = writeln!(out, "    class {node_id} node_b");
            prev_b = node_id;
        }

        // End nodes
        let end_a_state = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({end_a_state} A))");
        let _ = writeln!(out, "    {prev_a} --> EndA");
        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class EndA success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class EndA error");
            }
            _ => {
                let _ = writeln!(out, "    class EndA node_a");
            }
        }

        let end_b_state = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({end_b_state} B))");
        let _ = writeln!(out, "    {prev_b} --> EndB");
        match session_b.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class EndB success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class EndB error");
            }
            _ => {
                let _ = writeln!(out, "    class EndB node_b");
            }
        }

        out
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
    fn test_multiverse_mermaid_generation() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Failed);

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
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("EndA((Completed A))"));
        assert!(mermaid.contains("EndB((Failed B))"));
    }

    #[test]
    fn test_identical_sessions() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Completed);

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "bar"))),
        ];

        let activities_b = activities_a.clone();

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("shared_0"));
        assert!(mermaid.contains("shared_1"));
        assert!(!mermaid.contains("a_0"));
        assert!(!mermaid.contains("b_0"));
        assert!(!mermaid.contains("a_1"));
        assert!(!mermaid.contains("b_1"));
    }

    #[test]
    fn test_entirely_different_sessions() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Completed);

        let activities_a = vec![create_mock_activity("1", Some(("read_file", "foo")))];

        let activities_b = vec![create_mock_activity("1", Some(("read_file", "bar")))];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(!mermaid.contains("shared_0"));
        assert!(mermaid.contains("a_0"));
        assert!(mermaid.contains("b_0"));
    }
}
