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

    /// Generates a Mermaid flowchart comparing two sessions and their activities.
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
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e0e0e0,stroke:#616161,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathA fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef pathB fill:#fbe9e7,stroke:#d84315,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef success fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef error fill:#ffebee,stroke:#c62828,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let divergence_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let mut prev_node = "Start".to_string();

        // Shared Path
        #[allow(clippy::needless_range_loop)]
        for i in 0..divergence_idx {
            let act = &activities_a[i];
            let node_id = format!("shared_{i}");
            let label = act
                .activity_type
                .as_deref()
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");
            let detail = act.detail.as_deref().unwrap_or("");

            let (shape_open, shape_close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!("**{label}**<br>{}", Self::escape_label(detail))
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");

            prev_node = node_id;
        }

        let diverge_node = prev_node;

        // Path A
        if divergence_idx < activities_a.len() {
            let _ = writeln!(out, "    subgraph PathA[\"Path A: {}\"]", session_a.name);
            let mut prev_a = diverge_node.clone();
            #[allow(clippy::needless_range_loop)]
            for i in divergence_idx..activities_a.len() {
                let act = &activities_a[i];
                let node_id = format!("a_{i}");
                let label = act
                    .activity_type
                    .as_deref()
                    .or(act.stage_name.as_deref())
                    .unwrap_or("Activity");
                let detail = act.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label.to_string()
                } else {
                    format!("**{label}**<br>{}", Self::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} pathA");

                prev_a = node_id;
            }

            let end_a_state = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_a_id = "EndA";
            let _ = writeln!(out, "    {end_a_id}(({end_a_state}))");
            let _ = writeln!(out, "    {prev_a} --> {end_a_id}");
            Self::style_end_node(&mut out, end_a_id, session_a.state.as_ref());

            let _ = writeln!(out, "    end");
        } else {
            let end_a_state = session_a
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_a_id = "EndA";
            let _ = writeln!(out, "    {end_a_id}(({end_a_state}))");
            let _ = writeln!(out, "    {diverge_node} -->|Path A| {end_a_id}");
            Self::style_end_node(&mut out, end_a_id, session_a.state.as_ref());
        }

        // Path B
        if divergence_idx < activities_b.len() {
            let _ = writeln!(out, "    subgraph PathB[\"Path B: {}\"]", session_b.name);
            let mut prev_b = diverge_node.clone();
            #[allow(clippy::needless_range_loop)]
            for i in divergence_idx..activities_b.len() {
                let act = &activities_b[i];
                let node_id = format!("b_{i}");
                let label = act
                    .activity_type
                    .as_deref()
                    .or(act.stage_name.as_deref())
                    .unwrap_or("Activity");
                let detail = act.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if act.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label.to_string()
                } else {
                    format!("**{label}**<br>{}", Self::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} pathB");

                prev_b = node_id;
            }

            let end_b_state = session_b
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_b_id = "EndB";
            let _ = writeln!(out, "    {end_b_id}(({end_b_state}))");
            let _ = writeln!(out, "    {prev_b} --> {end_b_id}");
            Self::style_end_node(&mut out, end_b_id, session_b.state.as_ref());

            let _ = writeln!(out, "    end");
        } else {
            let end_b_state = session_b
                .state
                .as_ref()
                .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
            let end_b_id = "EndB";
            let _ = writeln!(out, "    {end_b_id}(({end_b_state}))");
            let _ = writeln!(out, "    {diverge_node} -->|Path B| {end_b_id}");
            Self::style_end_node(&mut out, end_b_id, session_b.state.as_ref());
        }

        out
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

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, ToolUse};

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
        output_files: Vec<&str>,
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
                    changed_files: output_files
                        .into_iter()
                        .map(|p| ChangedFile {
                            path: p.to_string(),
                            diff: String::new(),
                        })
                        .collect(),
                    commit_hash: None,
                    pull_request: None,
                })
            },
        }
    }

    #[test]
    fn test_multiverse_identical_sessions() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Completed);

        let activities_a = vec![create_mock_activity(
            "1",
            Some(("read_file", "foo")),
            vec![],
        )];
        let activities_b = vec![create_mock_activity(
            "1",
            Some(("read_file", "foo")),
            vec![],
        )];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{"));
        assert!(!mermaid.contains("subgraph PathA"));
        assert!(!mermaid.contains("subgraph PathB"));
        assert!(mermaid.contains("EndA((Completed))"));
        assert!(mermaid.contains("EndB((Completed))"));
        assert!(mermaid.contains("shared_0 -->|Path A| EndA"));
        assert!(mermaid.contains("shared_0 -->|Path B| EndB"));
    }

    #[test]
    fn test_multiverse_divergent_sessions() {
        let session_a = create_mock_session("sessions/A", SessionState::Completed);
        let session_b = create_mock_session("sessions/B", SessionState::Failed);

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "bar")), vec![]),
        ];
        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "baz")), vec![]),
            create_mock_activity("3", Some(("run_tests", "baz")), vec![]),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("shared_0{"));
        assert!(mermaid.contains("subgraph PathA"));
        assert!(mermaid.contains("subgraph PathB"));
        assert!(mermaid.contains("a_1{"));
        assert!(mermaid.contains("b_1{"));
        assert!(mermaid.contains("b_2{"));
        assert!(mermaid.contains("class EndA success"));
        assert!(mermaid.contains("class EndB error"));
    }
}
