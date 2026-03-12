use std::fmt::Write;

use crate::{Activity, Session, SessionComparator, SessionState};

/// Visualizes the divergence between two sessions as a Mermaid flowchart.
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
    #[must_use]
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
            "    classDef plan fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        // Shared timeline (before divergence)
        for (i, activity) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_{i}");
            self.write_activity_node(&mut out, &node_id, activity);
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            prev_node = node_id;
        }

        // Branching points
        let branch_a_start = prev_node.clone();
        let branch_b_start = prev_node;

        // Session A timeline (after divergence)
        let _ = writeln!(out, "    subgraph SessionA[\"{}\"]", session_a.name);
        let _ = writeln!(out, "    direction TB");
        let mut prev_a = branch_a_start;
        for (i, activity) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_{i}");
            self.write_activity_node(&mut out, &node_id, activity);
            let _ = writeln!(out, "    {prev_a} --> {node_id}");
            prev_a = node_id;
        }
        let end_a_state = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({end_a_state}))");
        let _ = writeln!(out, "    {prev_a} --> EndA");
        self.style_end_node(&mut out, "EndA", session_a.state);
        let _ = writeln!(out, "    end");

        // Session B timeline (after divergence)
        let _ = writeln!(out, "    subgraph SessionB[\"{}\"]", session_b.name);
        let _ = writeln!(out, "    direction TB");
        let mut prev_b = branch_b_start;
        for (i, activity) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_{i}");
            self.write_activity_node(&mut out, &node_id, activity);
            let _ = writeln!(out, "    {prev_b} --> {node_id}");
            prev_b = node_id;
        }
        let end_b_state = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({end_b_state}))");
        let _ = writeln!(out, "    {prev_b} --> EndB");
        self.style_end_node(&mut out, "EndB", session_b.state);
        let _ = writeln!(out, "    end");

        out
    }

    #[allow(clippy::unused_self)]
    fn write_activity_node(&self, out: &mut String, node_id: &str, activity: &Activity) {
        let label = if let Some(tool) = &activity.tool_use {
            tool.tool_name.clone()
        } else if let Some(act_type) = &activity.activity_type {
            act_type.clone()
        } else if let Some(stage) = &activity.stage_name {
            stage.clone()
        } else {
            "Activity".to_string()
        };

        let detail = activity.detail.as_deref().unwrap_or("");

        let (shape_open, shape_close) = if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
        };

        let node_text = if detail.is_empty() {
            label
        } else {
            format!("**{label}**<br>{}", Self::escape_label(detail))
        };

        let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
        if activity.tool_use.is_some() {
            let _ = writeln!(out, "    class {node_id} tool");
        }
    }

    #[allow(clippy::unused_self)]
    fn style_end_node(&self, out: &mut String, node_id: &str, state: Option<SessionState>) {
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
    fn test_multiverse_visualizer() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "bar")), vec!["src/main.rs"]),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "baz")), vec!["src/lib.rs"]),
        ];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("EndA((Completed))"));
        assert!(mermaid.contains("EndB((Completed))"));
    }
}
