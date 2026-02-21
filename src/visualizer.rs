use std::fmt::Write;

use crate::{Activity, Session};

/// Visualizes a session as a Mermaid flowchart.
#[derive(Debug, Default)]
pub struct SessionVisualizer;

impl SessionVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart string from a session and its activities.
    ///
    /// The graph includes:
    /// - A subgraph for the session plan (if available).
    /// - A sequence of nodes for each activity.
    /// - Specific styles for tool use, thoughts, and outputs.
    #[must_use]
    pub fn to_mermaid(&self, session: &Session, activities: &[Activity]) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Session: {}", session.name);
        let _ = writeln!(out, "    Start([Start])");

        // Style definitions
        let _ = writeln!(out, "    classDef plan fill:#f9f,stroke:#333,stroke-width:2px;");
        let _ = writeln!(out, "    classDef act fill:#bbf,stroke:#333,stroke-width:2px;");
        let _ = writeln!(out, "    classDef tool fill:#ff9,stroke:#333,stroke-width:2px;");
        let _ = writeln!(out, "    classDef thought fill:#dfd,stroke:#333,stroke-width:2px,stroke-dasharray: 5 5;");
        let _ = writeln!(out, "    classDef error fill:#f99,stroke:#333,stroke-width:2px;");

        // Plan Subgraph
        if let Some(plan) = &session.plan {
            if !plan.steps.is_empty() {
                let _ = writeln!(out, "\n    subgraph Plan [Plan]");
                for (i, step) in plan.steps.iter().enumerate() {
                    let step_id = format!("P{i}");
                    let status_icon = if matches!(step.status.to_lowercase().as_str(), "completed" | "done" | "success") {
                        "✅"
                    } else {
                        "⬜"
                    };
                    // Escape quotes in description
                    let desc = step.description.replace('"', "'");
                    let _ = writeln!(out, "        {step_id}[\"{status_icon} {desc}\"]:::plan");
                    if i > 0 {
                        let prev_id = format!("P{}", i - 1);
                        let _ = writeln!(out, "        {prev_id} --> {step_id}");
                    }
                }
                let _ = writeln!(out, "    end");
                let _ = writeln!(out, "    Start --> Plan");
            }
        }

        let mut last_node_id = "Start".to_string();
        if let Some(plan) = &session.plan {
            if !plan.steps.is_empty() {
                last_node_id = "Plan".to_string();
            }
        }

        // Activities
        for (i, activity) in activities.iter().enumerate() {
            let act_id = format!("A{i}");
            let label = activity.stage_name.as_deref().unwrap_or("Activity");
            let _ = writeln!(out, "    {act_id}([\"{label}\"]):::act");
            let _ = writeln!(out, "    {last_node_id} --> {act_id}");
            last_node_id = act_id.clone();

            // Detailed nodes attached to the activity

            // Thoughts / Overview
            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    let thought_id = format!("T{i}");
                    let short_thought = truncate(&thoughts.replace('"', "'"), 50);
                    let _ = writeln!(out, "    {thought_id}[\"💭 {short_thought}\"]:::thought");
                    let _ = writeln!(out, "    {act_id} -.-> {thought_id}");
                }
            }

            // Tool Use
            if let Some(tool) = &activity.tool_use {
                let tool_id = format!("TL{i}");
                let tool_name = &tool.tool_name;
                let _ = writeln!(out, "    {tool_id}{{\"🛠️ {tool_name}\"}}:::tool");
                let _ = writeln!(out, "    {act_id} --> {tool_id}");
                last_node_id = tool_id; // Flow continues from tool
            }

            // User Input
            if let Some(req) = &activity.user_input_request {
                let req_id = format!("Q{i}");
                let q = &req.question.replace('"', "'");
                let _ = writeln!(out, "    {req_id}[/\"❓ {q}\"/]");
                let _ = writeln!(out, "    {act_id} --> {req_id}");
                last_node_id = req_id;
            }

            // Errors
            if let Some(status) = activity.status {
                if matches!(status, crate::types::ActivityStatus::Failed) {
                     let err_id = format!("E{i}");
                     let _ = writeln!(out, "    {err_id}((\"❌ Failed\")):::error");
                     let _ = writeln!(out, "    {act_id} --> {err_id}");
                }
            }
        }

        // End
        let _ = writeln!(out, "    End([End])");
        let _ = writeln!(out, "    {last_node_id} --> End");

        out
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Activity, ActivityStatus, Overview, Plan, PlanStep, Session, ToolUse};

    #[test]
    fn test_empty_session() {
        let session = Session {
            name: "sessions/1".into(),
            id: Some("1".into()),
            prompt: Some("do something".into()),
            title: None,
            description: None,
            state: None,
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
        assert!(mermaid.contains("Start([Start])"));
        assert!(mermaid.contains("End([End])"));
        assert!(mermaid.contains("Start --> End"));
    }

    #[test]
    fn test_session_with_plan_and_activities() {
        let session = Session {
            name: "sessions/2".into(),
            id: Some("2".into()),
            prompt: None,
            title: None,
            description: None,
            state: None,
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: Some(Plan {
                steps: vec![
                    PlanStep { description: "Step 1".into(), status: "completed".into() },
                    PlanStep { description: "Step 2".into(), status: "pending".into() },
                ],
            }),
            output: None,
            outputs: vec![],
        };

        let activities = vec![
            Activity {
                name: "act1".into(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Thinking".into()),
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: Some(Overview {
                    state: None,
                    thoughts: Some("I should check the files.".into()),
                    summary: None,
                }),
                plan: None,
                user_input_request: None,
                tool_use: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "act2".into(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Executing".into()),
                activity_type: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "list_files".into(),
                    input: "{}".into(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let viz = SessionVisualizer::new();
        let mermaid = viz.to_mermaid(&session, &activities);

        // Check Plan
        assert!(mermaid.contains("subgraph Plan"));
        assert!(mermaid.contains("Step 1"));
        assert!(mermaid.contains("Step 2"));

        // Check Activities
        assert!(mermaid.contains("Thinking"));
        assert!(mermaid.contains("Executing"));

        // Check Thoughts
        assert!(mermaid.contains("I should check the files."));

        // Check Tool Use
        assert!(mermaid.contains("list_files"));
    }
}
