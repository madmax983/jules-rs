use std::fmt::Write;

use crate::{Activity, Session};

/// Visualizes session execution flow using Mermaid diagrams.
///
/// This exporter generates a flowchart that represents the session's plan
/// and the sequence of activities that occurred during execution.
#[derive(Debug, Default)]
pub struct SessionVisualizer;

impl SessionVisualizer {
    /// Creates a new visualizer instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart from a session and its activities.
    ///
    /// The graph includes:
    /// - The session plan (if available).
    /// - A timeline of activities (thoughts, tool usage, outputs).
    /// - Visual cues for status (success/failure).
    ///
    /// # Errors
    ///
    /// Returns a [`std::fmt::Error`] if string formatting fails (unlikely).
    pub fn to_mermaid(
        &self,
        session: &Session,
        activities: &[Activity],
    ) -> Result<String, std::fmt::Error> {
        let mut out = String::new();

        writeln!(out, "graph TD")?;
        writeln!(out, "    %% Styles")?;
        writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        )?;
        writeln!(
            out,
            "    classDef success fill:#e6fffa,stroke:#38b2ac,stroke-width:2px;"
        )?;
        writeln!(
            out,
            "    classDef failure fill:#fff5f5,stroke:#e53e3e,stroke-width:2px;"
        )?;
        writeln!(
            out,
            "    classDef tool fill:#ebf8ff,stroke:#3182ce,stroke-width:1px,stroke-dasharray: 5 5;"
        )?;
        writeln!(
            out,
            "    classDef thought fill:#fffaf0,stroke:#dd6b20,stroke-width:1px;"
        )?;
        writeln!(out)?;

        // Session Start
        writeln!(out, "    Start((\"Session: {}\"))", session.name)?;
        if let Some(state) = session.state {
            writeln!(out, "    Start --> |{state:?}| PlanStart")?;
        } else {
            writeln!(out, "    Start --> PlanStart")?;
        }

        // Plan Subgraph
        writeln!(out)?;
        writeln!(out, "    subgraph Plan [\"Items to Complete\"]")?;
        writeln!(out, "        direction TB")?;
        writeln!(out, "        PlanStart((Start))")?;

        let mut last_plan_node = "PlanStart".to_string();

        if let Some(plan) = &session.plan {
            for (i, step) in plan.steps.iter().enumerate() {
                let node_id = format!("P{i}");
                let status_class = if step.status.to_lowercase() == "completed" {
                    "success"
                } else {
                    "default"
                };
                let label = Self::escape_label(&step.description);

                writeln!(out, "        {node_id}[\"{label}\"]:::{status_class}")?;
                writeln!(out, "        {last_plan_node} --> {node_id}")?;
                last_plan_node = node_id;
            }
        } else {
            writeln!(out, "        NoPlan[\"No Plan Available\"]")?;
            writeln!(out, "        PlanStart --> NoPlan")?;
            last_plan_node = "NoPlan".to_string();
        }
        writeln!(out, "    end")?;

        // Execution Subgraph
        writeln!(out)?;
        writeln!(out, "    subgraph Execution [\"Activity Timeline\"]")?;
        writeln!(out, "        direction TB")?;

        let mut last_exec_node = last_plan_node;

        for (i, activity) in activities.iter().enumerate() {
            let node_id = format!("A{i}");
            let (shape_open, shape_close, style_class) = Self::get_node_style(activity);
            let label = Self::get_node_label(activity);

            writeln!(
                out,
                "        {node_id}{shape_open}\"{label}\"{shape_close}:::{style_class}"
            )?;
            writeln!(out, "        {last_exec_node} --> {node_id}")?;

            // Add tool details as a side note/sub-node if available
            if let Some(tool) = &activity.tool_use {
                let tool_node_id = format!("T{i}");
                let tool_label = format!("Tool: {}", tool.tool_name);
                writeln!(out, "        {tool_node_id}[/\"{tool_label}\"\\]:::tool")?;
                writeln!(out, "        {node_id} -.-> {tool_node_id}")?;
            }

            last_exec_node = node_id;
        }

        writeln!(out, "    end")?;

        // Final State
        writeln!(out)?;
        writeln!(out, "    End((\"End\"))")?;
        writeln!(out, "    {last_exec_node} --> End")?;

        Ok(out)
    }

    fn get_node_style(activity: &Activity) -> (&'static str, &'static str, &'static str) {
        if activity.tool_use.is_some() {
            return ("(", ")", "tool");
        }
        if let Some(overview) = &activity.overview {
            if overview.thoughts.is_some() {
                return (">", "]", "thought");
            }
        }
        match activity.status {
            Some(crate::types::ActivityStatus::Success) => ("(", ")", "success"),
            Some(crate::types::ActivityStatus::Failed) => ("(", ")", "failure"),
            _ => ("[", "]", "default"),
        }
    }

    fn get_node_label(activity: &Activity) -> String {
        if let Some(tool) = &activity.tool_use {
            return format!("Call: {}", tool.tool_name);
        }
        if let Some(overview) = &activity.overview {
            if let Some(summary) = &overview.summary {
                return Self::escape_label(summary);
            }
        }
        Self::escape_label(&activity.name)
    }

    fn escape_label(text: &str) -> String {
        text.replace('"', "'").chars().take(50).collect::<String>()
            + if text.len() > 50 { "..." } else { "" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Activity, ActivityStatus, Plan, PlanStep, Session, SessionState, ToolUse};

    #[test]
    fn test_mermaid_generation() {
        let session = Session {
            name: "sessions/s-1".to_string(),
            id: Some("s-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("Bug Fix".to_string()),
            description: None,
            state: Some(SessionState::Completed),
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
                        description: "Analyze code".to_string(),
                        status: "completed".to_string(),
                    },
                    PlanStep {
                        description: "Write test".to_string(),
                        status: "pending".to_string(),
                    },
                ],
            }),
            output: None,
            outputs: vec![],
        };

        let activities = vec![Activity {
            name: "sessions/s-1/activities/a-1".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }];

        let visualizer = SessionVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session, &activities).unwrap();

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("subgraph Plan"));
        assert!(mermaid.contains("Analyze code"));
        assert!(mermaid.contains("read_file"));
        assert!(mermaid.contains("classDef success"));
    }
}
