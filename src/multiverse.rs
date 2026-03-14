use std::fmt::Write;

use crate::{Activity, Session, SessionComparator, SessionState};

/// Mashup of comparator and visualizer to generate a flowchart detailing how two sessions diverge.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer {
    comparator: SessionComparator,
}

impl MultiverseVisualizer {
    /// Creates a new `MultiverseVisualizer`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            comparator: SessionComparator::new(),
        }
    }

    fn escape_label(label: &str) -> String {
        label.replace('\"', "&quot;").replace('\n', "<br>")
    }

    fn get_activity_label(activity: &Activity) -> String {
        if let Some(tool) = &activity.tool_use {
            tool.tool_name.clone()
        } else if let Some(activity_type) = &activity.activity_type {
            activity_type.clone()
        } else if let Some(stage_name) = &activity.stage_name {
            stage_name.clone()
        } else {
            "Activity".to_string()
        }
    }

    /// Generates a Mermaid flowchart detailing the divergence of two sessions.
    #[must_use]
    #[allow(clippy::similar_names)]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session Comparison");
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
            "    classDef shared fill:#fff3e0,stroke:#e65100,stroke-width:2px,stroke-dasharray: 5 5;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#e8eaf6,stroke:#3f51b5,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#fce4ec,stroke:#e91e63,stroke-width:2px;"
        );

        let _ = writeln!(out, "    Start((Start))");

        let report = self
            .comparator
            .compare(session_a, activities_a, session_b, activities_b);
        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(activities_a.len().min(activities_b.len()));

        let mut prev_node = "Start".to_string();

        // Render shared path
        for (i, activity) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_act_{i}");
            let label = Self::escape_label(&Self::get_activity_label(activity));

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        let diverge_node = prev_node;

        // Render Branch A
        let mut prev_node_a = diverge_node.clone();
        for (i, activity) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("act_a_{i}");
            let label = Self::escape_label(&Self::get_activity_label(activity));

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"A: {label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node_a} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} branchA");
            prev_node_a = node_id;
        }

        let end_state_a = session_a
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({end_state_a}))");
        let _ = writeln!(out, "    {prev_node_a} --> EndA");
        match session_a.state {
            Some(SessionState::Completed) => {
                let _ = writeln!(out, "    class EndA success");
            }
            Some(SessionState::Failed) => {
                let _ = writeln!(out, "    class EndA error");
            }
            _ => {}
        }

        // Render Branch B
        let mut prev_node_b = diverge_node;
        for (i, activity) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("act_b_{i}");
            let label = Self::escape_label(&Self::get_activity_label(activity));

            let (shape_open, shape_close) = if activity.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{shape_open}\"B: {label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node_b} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} branchB");
            prev_node_b = node_id;
        }

        let end_state_b = session_b
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({end_state_b}))");
        let _ = writeln!(out, "    {prev_node_b} --> EndB");
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activity, Session, SessionState, ToolUse};

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_visualizer_shared_path() {
        let session_a = Session {
            name: "sessions/sess-1".to_string(),
            id: Some("sess-1".to_string()),
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

        let session_b = Session {
            name: "sessions/sess-2".to_string(),
            id: Some("sess-2".to_string()),
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

        let act1 = Activity {
            name: "act-1".to_string(),
            status: Some(crate::ActivityStatus::Success),
            stage_name: Some("Start".to_string()),
            activity_type: Some("Action".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "file1.rs".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        };

        let act2 = Activity {
            name: "act-2".to_string(),
            status: Some(crate::ActivityStatus::Success),
            stage_name: Some("Start".to_string()),
            activity_type: Some("Action".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "file1.rs".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        };

        let activities_a = vec![act1.clone()];
        let activities_b = vec![act2];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        // Everything matches
        assert!(mermaid.contains("shared_act_0"));
        assert!(mermaid.contains("shared"));
        assert!(!mermaid.contains("act_a_0"));
        assert!(!mermaid.contains("act_b_0"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_visualizer_diverged_path() {
        let session_a = Session {
            name: "sessions/sess-1".to_string(),
            id: Some("sess-1".to_string()),
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

        let session_b = Session {
            name: "sessions/sess-2".to_string(),
            id: Some("sess-2".to_string()),
            prompt: None,
            title: None,
            description: None,
            state: Some(SessionState::Failed),
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

        let act1 = Activity {
            name: "act-1".to_string(),
            status: Some(crate::ActivityStatus::Success),
            stage_name: Some("Start".to_string()),
            activity_type: Some("Action".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "read_file".to_string(),
                input: "file1.rs".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        };

        let act2 = Activity {
            name: "act-2".to_string(),
            status: Some(crate::ActivityStatus::Success),
            stage_name: Some("Start".to_string()),
            activity_type: Some("Action".to_string()),
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "list_files".to_string(),
                input: "/".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        };

        let activities_a = vec![act1];
        let activities_b = vec![act2];

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        // Diverges from the first activity
        assert!(!mermaid.contains("shared_act_0"));
        assert!(mermaid.contains("act_a_0"));
        assert!(mermaid.contains("act_b_0"));
        assert!(mermaid.contains("read_file"));
        assert!(mermaid.contains("list_files"));
    }
}
