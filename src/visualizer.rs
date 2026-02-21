use std::fmt::Write;

use crate::{Activity, Session, SessionState};

/// Visualizes a session as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct SessionVisualizer;

impl SessionVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart from a session and its activities.
    #[must_use]
    pub fn to_mermaid(&self, session: &Session, activities: &[Activity]) -> String {
        let mut out = String::new();
        // Mermaid header
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Session: {}", session.name);

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

        // Plan Subgraph
        let mut last_plan_node = None;
        if let Some(plan) = &session.plan {
            let _ = writeln!(out, "    subgraph Plan[\"Session Plan\"]");
            let _ = writeln!(out, "    direction TB");
            for (i, step) in plan.steps.iter().enumerate() {
                let id = format!("step_{i}");
                let desc = Self::escape_label(&step.description);
                let _ = writeln!(out, "    {id}[\"{desc}\"]");
                if i > 0 {
                    let prev = format!("step_{}", i - 1);
                    let _ = writeln!(out, "    {prev} --> {id}");
                }
                let _ = writeln!(out, "    class {id} plan");
                last_plan_node = Some(id);
            }
            let _ = writeln!(out, "    end");

            if !plan.steps.is_empty() {
                let _ = writeln!(out, "    Start --> step_0");
            }
        }

        // Determine connection point from Plan to Activities
        let mut prev_node = last_plan_node.unwrap_or_else(|| "Start".to_string());

        // Execution Flow
        for (i, activity) in activities.iter().enumerate() {
            let node_id = format!("act_{i}");
            let label = activity
                .activity_type
                .as_deref()
                .or(activity.stage_name.as_deref())
                .unwrap_or("Activity");
            let detail = activity.detail.as_deref().unwrap_or("");

            // Differentiate shapes: Tools {{}}, Others []
            let (shape_open, shape_close) = if activity.tool_use.is_some() {
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

            if activity.tool_use.is_some() {
                let _ = writeln!(out, "    class {node_id} tool");
            }

            // Attach "Thoughts" as a note
            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    let thought_node_id = format!("note_{i}");
                    let escaped_thoughts = Self::escape_note(thoughts);
                    // Use a right-side note
                    let _ = writeln!(out, "    {thought_node_id}>\"{escaped_thoughts}\"]");
                    let _ = writeln!(out, "    {node_id} -.- {thought_node_id}");
                }
            }

            prev_node = node_id;
        }

        // End Node
        let end_state = session
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    End(({end_state}))");
        let _ = writeln!(out, "    {prev_node} --> End");

        // Style End based on session state
        match session.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class End success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class End error");
            }
            _ => {}
        }

        out
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
    use crate::{Plan, PlanStep};

    #[test]
    fn test_empty_session_graph() {
        let session = Session {
            name: "sessions/1".to_string(),
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
        };
        let activities = vec![];

        let viz = SessionVisualizer::new();
        let mermaid = viz.to_mermaid(&session, &activities);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));
        assert!(mermaid.contains("End((Completed))"));
        assert!(mermaid.contains("Start --> End"));
        assert!(mermaid.contains("class End success"));
    }

    #[test]
    fn test_planned_session_graph() {
        let session = Session {
            name: "sessions/2".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: Some(SessionState::InProgress),
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: Some(Plan {
                steps: vec![
                    PlanStep {
                        description: "Step 1".to_string(),
                        status: "pending".to_string(),
                    },
                    PlanStep {
                        description: "Step 2".to_string(),
                        status: "pending".to_string(),
                    },
                ],
            }),
            output: None,
            outputs: vec![],
        };
        let activities = vec![];

        let viz = SessionVisualizer::new();
        let mermaid = viz.to_mermaid(&session, &activities);

        assert!(mermaid.contains("subgraph Plan"));
        assert!(mermaid.contains("step_0[\"Step 1\"]"));
        assert!(mermaid.contains("step_1[\"Step 2\"]"));
        assert!(mermaid.contains("step_0 --> step_1"));
        assert!(mermaid.contains("Start --> step_0"));
        assert!(mermaid.contains("step_1 --> End"));
    }
}
