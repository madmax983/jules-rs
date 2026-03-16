use std::fmt::Write;

use crate::{Activity, Session, SessionComparator, SessionState};

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
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::similar_names)]
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
        // Setup header and class defs
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(
            out,
            "    %% Multiverse: {} vs {}",
            session_a.name, session_b.name
        );

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
            "    classDef a_only fill:#fff3e0,stroke:#e65100,stroke-width:2px,stroke-dasharray: 5 5;"
        );
        let _ = writeln!(
            out,
            "    classDef b_only fill:#f3e5f5,stroke:#4a148c,stroke-width:2px,stroke-dasharray: 5 5;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let mut prev_shared = "Start".to_string();

        for (i, activity) in activities_a.iter().enumerate().take(divergence_index) {
            prev_shared = Self::format_node(&mut out, "shared", i, activity, &prev_shared, true);
        }

        let mut prev_a = prev_shared.clone();
        for (i, activity) in activities_a.iter().enumerate().skip(divergence_index) {
            prev_a = Self::format_node(&mut out, "a", i, activity, &prev_a, false);
        }

        let mut prev_b = prev_shared;
        for (i, activity) in activities_b.iter().enumerate().skip(divergence_index) {
            prev_b = Self::format_node(&mut out, "b", i, activity, &prev_b, false);
        }

        Self::format_end_node(&mut out, "a", session_a, &prev_a);
        Self::format_end_node(&mut out, "b", session_b, &prev_b);

        out
    }

    fn format_node(
        out: &mut String,
        prefix: &str,
        i: usize,
        activity: &Activity,
        prev_node: &str,
        is_shared: bool,
    ) -> String {
        let node_id = if is_shared {
            format!("act_shared_{i}")
        } else {
            format!("act_{prefix}_{i}")
        };

        let label = activity
            .tool_use
            .as_ref()
            .map(|t| t.tool_name.as_str())
            .or(activity.activity_type.as_deref())
            .or(activity.stage_name.as_deref())
            .unwrap_or("Activity");

        let (shape_open, shape_close) = if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
        };

        let escaped_label = Self::escape_label(label);
        let _ = writeln!(
            out,
            "    {node_id}{shape_open}\"{escaped_label}\"{shape_close}"
        );
        let _ = writeln!(out, "    {prev_node} --> {node_id}");

        if activity.tool_use.is_some() {
            let _ = writeln!(out, "    class {node_id} tool");
        }

        if !is_shared {
            let class_name = if prefix == "a" { "a_only" } else { "b_only" };
            let _ = writeln!(out, "    class {node_id} {class_name}");
        }

        node_id
    }

    fn format_end_node(out: &mut String, prefix: &str, session: &Session, prev_node: &str) {
        let node_id = format!("End_{prefix}");
        let end_state = session
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    {node_id}(({end_state}))");
        let _ = writeln!(out, "    {prev_node} --> {node_id}");

        match session.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {node_id} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {node_id} error");
            }
            _ => {
                let class_name = if prefix == "a" { "a_only" } else { "b_only" };
                let _ = writeln!(out, "    class {node_id} {class_name}");
            }
        }
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, SessionOutput, ToolUse};

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

    fn create_mock_activity(
        name: &str,
        tool: Option<(&str, &str)>,
        output_files: &[&str],
    ) -> Activity {
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
            output: if output_files.is_empty() {
                None
            } else {
                Some(SessionOutput {
                    changed_files: vec![],
                    commit_hash: None,
                    pull_request: None,
                })
            },
        }
    }

    #[test]
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_visualizer_mermaid() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Failed);

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), &[]),
            create_mock_activity("2", Some(("write_file", "bar")), &["src/main.rs"]),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), &[]),
            create_mock_activity("2", Some(("write_file", "baz")), &["src/lib.rs"]),
            create_mock_activity("3", Some(("run_bash", "echo")), &[]),
        ];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));

        // Shared node up to index 0
        assert!(mermaid.contains("act_shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("Start --> act_shared_0"));

        // Divergent paths starting from index 1
        assert!(mermaid.contains("act_a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("act_shared_0 --> act_a_1"));
        assert!(mermaid.contains("class act_a_1 a_only"));

        assert!(mermaid.contains("act_b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("act_shared_0 --> act_b_1"));
        assert!(mermaid.contains("class act_b_1 b_only"));

        assert!(mermaid.contains("act_b_2{{\"run_bash\"}}"));
        assert!(mermaid.contains("act_b_1 --> act_b_2"));

        // Ends
        assert!(mermaid.contains("End_a((Completed))"));
        assert!(mermaid.contains("act_a_1 --> End_a"));
        assert!(mermaid.contains("class End_a success"));

        assert!(mermaid.contains("End_b((Failed))"));
        assert!(mermaid.contains("act_b_2 --> End_b"));
        assert!(mermaid.contains("class End_b error"));
    }
}
