use std::fmt::Write;

use crate::{Activity, Session, SessionState};

/// Visualizes how two sessions diverge as a Mermaid diagram.
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
            "    classDef shared fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#e8eaf6,stroke:#283593,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#fbe9e7,stroke:#d84315,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let count_a = activities_a.len();
        let count_b = activities_b.len();
        let min_len = count_a.min(count_b);

        let mut first_divergence_index = min_len;

        for (i, act_a) in activities_a.iter().enumerate().take(min_len) {
            let act_b = &activities_b[i];

            let tool_a = act_a.tool_use.as_ref().map(|t| (&t.tool_name, &t.input));
            let tool_b = act_b.tool_use.as_ref().map(|t| (&t.tool_name, &t.input));

            if tool_a != tool_b || act_a.status != act_b.status {
                first_divergence_index = i;
                break;
            }
        }

        let mut prev_node = "Start".to_string();

        // Shared Prefix
        for (i, activity) in activities_a.iter().enumerate().take(first_divergence_index) {
            let node_id = format!("shared_{i}");
            let label = Self::get_label(activity);

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }
            prev_node = node_id;
        }

        let divergence_root = prev_node;

        // Branch A
        let mut prev_a = divergence_root.clone();
        if first_divergence_index < count_a {
            let _ = writeln!(out, "    subgraph SessionA[\"{}\"]", session_a.name);
            for (i, activity) in activities_a
                .iter()
                .enumerate()
                .take(count_a)
                .skip(first_divergence_index)
            {
                let node_id = format!("a_{i}");
                let label = Self::get_label(activity);

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} branchA");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // End A
        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let id_end_a = "EndA";
        let _ = writeln!(out, "    {id_end_a}(({end_state_a}))");
        let _ = writeln!(out, "    {prev_a} --> {id_end_a}");
        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {id_end_a} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {id_end_a} error");
            }
            _ => {}
        }

        // Branch B
        let mut prev_b = divergence_root;
        if first_divergence_index < count_b {
            let _ = writeln!(out, "    subgraph SessionB[\"{}\"]", session_b.name);
            for (i, activity) in activities_b
                .iter()
                .enumerate()
                .take(count_b)
                .skip(first_divergence_index)
            {
                let node_id = format!("b_{i}");
                let label = Self::get_label(activity);

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} branchB");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // End B
        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let id_end_b = "EndB";
        let _ = writeln!(out, "    {id_end_b}(({end_state_b}))");
        let _ = writeln!(out, "    {prev_b} --> {id_end_b}");
        match session_b.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class {id_end_b} success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class {id_end_b} error");
            }
            _ => {}
        }

        out
    }

    fn get_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            tool.tool_name.clone()
        } else if let Some(act_type) = &activity.activity_type {
            act_type.clone()
        } else if let Some(stage) = &activity.stage_name {
            stage.clone()
        } else {
            "Activity".to_string()
        };

        Self::escape_label(&label)
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
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
    fn test_multiverse_visualizer() {
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

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("class shared_0 shared"));
        assert!(mermaid.contains("class shared_0 tool"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("EndA((Completed))"));
        assert!(mermaid.contains("EndB((Failed))"));
        assert!(mermaid.contains("class a_1 branchA"));
        assert!(mermaid.contains("class b_1 branchB"));
    }
}
