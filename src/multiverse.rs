use crate::{Activity, Session, SessionState};
use std::fmt::Write;

#[cfg(feature = "comparator")]
use crate::SessionComparator;

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
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();

        // Use SessionComparator to find divergence point
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let first_divergence_index = report.first_divergence_index.unwrap_or_else(|| {
            // If identical or one is a subset, divergence point is the length of the shorter one
            activities_a.len().min(activities_b.len())
        });

        // Mermaid header
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(
            out,
            "    %% Multiverse Divergence: {} vs {}",
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

        // Start Node
        let _ = writeln!(out, "    Start((Start))");

        let mut prev_node = "Start".to_string();

        // 1. Render Shared Ancestry
        for (i, activity) in activities_a.iter().enumerate().take(first_divergence_index) {
            let node_id = format!("shared_{i}");
            let label = Self::get_label(activity);
            let detail = activity.detail.as_deref().unwrap_or("");

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let node_text = if detail.is_empty() {
                label
            } else {
                format!("**{}**<br>{}", label, Self::escape_label(detail))
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    let thought_node_id = format!("shared_note_{i}");
                    let escaped_thoughts = Self::escape_note(thoughts);
                    let _ = writeln!(out, "    {thought_node_id}>\"{escaped_thoughts}\"]");
                    let _ = writeln!(out, "    {node_id} -.- {thought_node_id}");
                }
            }

            prev_node = node_id;
        }

        let branch_point = prev_node;

        // 2. Render Session A Branch
        let mut prev_a = branch_point.clone();
        if first_divergence_index < activities_a.len() {
            let _ = writeln!(out, "    subgraph SessionA[\"{}\"]", session_a.name);
            let _ = writeln!(out, "    direction TB");
            for (i, activity) in activities_a.iter().enumerate().skip(first_divergence_index) {
                let node_id = format!("a_{i}");
                let label = Self::get_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label
                } else {
                    format!("**{}**<br>{}", label, Self::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                if let Some(overview) = &activity.overview {
                    if let Some(thoughts) = &overview.thoughts {
                        let thought_node_id = format!("a_note_{i}");
                        let escaped_thoughts = Self::escape_note(thoughts);
                        let _ = writeln!(out, "    {thought_node_id}>\"{escaped_thoughts}\"]");
                        let _ = writeln!(out, "    {node_id} -.- {thought_node_id}");
                    }
                }

                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // 3. Render Session B Branch
        let mut prev_b = branch_point;
        if first_divergence_index < activities_b.len() {
            let _ = writeln!(out, "    subgraph SessionB[\"{}\"]", session_b.name);
            let _ = writeln!(out, "    direction TB");
            for (i, activity) in activities_b.iter().enumerate().skip(first_divergence_index) {
                let node_id = format!("b_{i}");
                let label = Self::get_label(activity);
                let detail = activity.detail.as_deref().unwrap_or("");

                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let node_text = if detail.is_empty() {
                    label
                } else {
                    format!("**{}**<br>{}", label, Self::escape_label(detail))
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{node_text}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");

                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }

                if let Some(overview) = &activity.overview {
                    if let Some(thoughts) = &overview.thoughts {
                        let thought_node_id = format!("b_note_{i}");
                        let escaped_thoughts = Self::escape_note(thoughts);
                        let _ = writeln!(out, "    {thought_node_id}>\"{escaped_thoughts}\"]");
                        let _ = writeln!(out, "    {node_id} -.- {thought_node_id}");
                    }
                }

                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // End Nodes for A and B
        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({end_state_a}))");
        let _ = writeln!(out, "    {prev_a} --> EndA");

        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class EndA success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class EndA error");
            }
            _ => {}
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({end_state_b}))");
        let _ = writeln!(out, "    {prev_b} --> EndB");

        match session_b.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class EndB success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class EndB error");
            }
            _ => {}
        }

        out
    }

    fn get_label(activity: &Activity) -> String {
        if let Some(tool) = &activity.tool_use {
            return tool.tool_name.clone();
        }
        if let Some(activity_type) = &activity.activity_type {
            return activity_type.clone();
        }
        if let Some(stage_name) = &activity.stage_name {
            return stage_name.clone();
        }
        "Activity".to_string()
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }

    fn escape_note(s: &str) -> String {
        // Truncate long thoughts and sanitize
        let s = s.replace('"', "'");
        if s.len() > 100 {
            format!("{}...", &s[..100])
        } else {
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ToolUse};

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
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_mermaid_generation() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        // Shared
        let act1 = create_mock_activity("1", Some(("read_file", "foo")));
        // Diverges here
        let act2_a = create_mock_activity("2", Some(("write_file", "a")));
        let act2_b = create_mock_activity("2", Some(("write_file", "b")));

        let activities_a = vec![act1.clone(), act2_a];
        let activities_b = vec![act1, act2_b];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        // Core tags
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));

        // Shared node
        assert!(mermaid.contains("shared_0{{\"read_file\"}}"));
        assert!(mermaid.contains("Start --> shared_0"));

        // A branch
        assert!(mermaid.contains("subgraph SessionA[\"sessions/A\"]"));
        assert!(mermaid.contains("a_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> a_1"));
        assert!(mermaid.contains("EndA((Completed))"));

        // B branch
        assert!(mermaid.contains("subgraph SessionB[\"sessions/B\"]"));
        assert!(mermaid.contains("b_1{{\"write_file\"}}"));
        assert!(mermaid.contains("shared_0 --> b_1"));
        assert!(mermaid.contains("EndB((Completed))"));
    }
}
