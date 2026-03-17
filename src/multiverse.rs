use crate::{Activity, Session, SessionComparator, SessionVisualizer};
use std::fmt::Write;

/// Visualizes the divergence between two sessions as a Mermaid flowchart.
#[derive(Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

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
        let _ = writeln!(
            out,
            "    %% Multiverse: {} vs {}",
            session_a.name, session_b.name
        );

        // Styling
        let _ = writeln!(
            out,
            "    classDef shared fill:#f5f5f5,stroke:#9e9e9e,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef diffA fill:#e3f2fd,stroke:#2196f3,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef diffB fill:#fce4ec,stroke:#e91e63,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        // Shared activities
        for (i, act) in activities_a.iter().enumerate().take(divergence_idx) {

            let node_id = format!("shared_{i}");
            let label = Self::get_label(act);
            let shape = Self::get_shape(act, &label);

            let _ = writeln!(out, "    {node_id}{shape}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        // Divergence A
        if divergence_idx < activities_a.len() {
            let mut prev_a = prev_node.clone();
            for (i, act) in activities_a.iter().enumerate().skip(divergence_idx) {
                let node_id = format!("a_{i}");
                let label = Self::get_label(act);
                let shape = Self::get_shape(act, &label);

                let _ = writeln!(out, "    {node_id}{shape}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} diffA");
                prev_a = node_id;
            }
            let _ = writeln!(
                out,
                "    EndA(({}))",
                session_a
                    .state
                    .as_ref()
                    .map_or("Unknown".to_string(), |s| format!("{s:?}"))
            );
            let _ = writeln!(out, "    {prev_a} --> EndA");
        } else {
            let _ = writeln!(
                out,
                "    EndA(({}))",
                session_a
                    .state
                    .as_ref()
                    .map_or("Unknown".to_string(), |s| format!("{s:?}"))
            );
            let _ = writeln!(out, "    {prev_node} --> EndA");
        }

        // Divergence B
        if divergence_idx < activities_b.len() {
            let mut prev_b = prev_node.clone();
            for (i, act) in activities_b.iter().enumerate().skip(divergence_idx) {
                let node_id = format!("b_{i}");
                let label = Self::get_label(act);
                let shape = Self::get_shape(act, &label);

                let _ = writeln!(out, "    {node_id}{shape}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} diffB");
                prev_b = node_id;
            }
            let _ = writeln!(
                out,
                "    EndB(({}))",
                session_b
                    .state
                    .as_ref()
                    .map_or("Unknown".to_string(), |s| format!("{s:?}"))
            );
            let _ = writeln!(out, "    {prev_b} --> EndB");
        } else {
            let _ = writeln!(
                out,
                "    EndB(({}))",
                session_b
                    .state
                    .as_ref()
                    .map_or("Unknown".to_string(), |s| format!("{s:?}"))
            );
            let _ = writeln!(out, "    {prev_node} --> EndB");
        }

        out
    }

    fn get_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else {
            activity
                .activity_type
                .as_deref()
                .or(activity.stage_name.as_deref())
                .unwrap_or("Activity")
        };
        SessionVisualizer::escape_label(label)
    }

    fn get_shape(activity: &Activity, label: &str) -> String {
        if activity.tool_use.is_some() {
            format!("{{\"{label}\"}}")
        } else {
            format!("[\"{label}\"]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, SessionState, ToolUse};

    #[test]
    fn test_multiverse_visualizer_mermaid() {
        let session_a = Session {
            name: "sessions/A".to_string(),
            state: Some(SessionState::Completed),
            id: None,
            prompt: None,
            title: None,
            description: None,
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
            name: "sessions/B".to_string(),
            state: Some(SessionState::Failed),
            id: None,
            prompt: None,
            title: None,
            description: None,
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

        let activities_a = vec![
            Activity {
                name: "1".to_string(),
                status: Some(ActivityStatus::Success),
                activity_type: Some("Action".to_string()),
                tool_use: Some(ToolUse {
                    tool_name: "read_file".to_string(),
                    input: "foo".to_string(),
                }),
                stage_name: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "2".to_string(),
                status: Some(ActivityStatus::Success),
                activity_type: Some("Action".to_string()),
                tool_use: Some(ToolUse {
                    tool_name: "write_file".to_string(),
                    input: "bar".to_string(),
                }),
                stage_name: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let activities_b = vec![
            Activity {
                name: "1".to_string(),
                status: Some(ActivityStatus::Success),
                activity_type: Some("Action".to_string()),
                tool_use: Some(ToolUse {
                    tool_name: "read_file".to_string(),
                    input: "foo".to_string(),
                }),
                stage_name: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "2".to_string(),
                status: Some(ActivityStatus::Failed),
                activity_type: Some("Action".to_string()),
                tool_use: Some(ToolUse {
                    tool_name: "write_file".to_string(),
                    input: "baz".to_string(),
                }),
                stage_name: None,
                detail: None,
                timestamp: None,
                overview: None,
                plan: None,
                user_input_request: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &activities_a, &session_b, &activities_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("Start((Start))"));
        assert!(mermaid.contains("shared_0{\"read_file\"}"));
        assert!(mermaid.contains("a_1{\"write_file\"}"));
        assert!(mermaid.contains("b_1{\"write_file\"}"));
        assert!(mermaid.contains("EndA((Completed))"));
        assert!(mermaid.contains("EndB((Failed))"));
        assert!(mermaid.contains("class shared_0 shared"));
        assert!(mermaid.contains("class a_1 diffA"));
        assert!(mermaid.contains("class b_1 diffB"));
    }
}
