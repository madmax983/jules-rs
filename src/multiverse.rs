use std::fmt::Write;

use crate::{Activity, ComparisonReport, Session};

/// Visualizes a comparison between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart comparing two sessions.
    #[must_use]
    pub fn to_mermaid(
        &self,
        report: &ComparisonReport,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        // Mermaid header
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(
            out,
            "    %% Multiverse Comparison: {} vs {}",
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
            "    classDef shared fill:#f3e5f5,stroke:#7b1fa2,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );

        let div_idx = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        // 1. Shared Path
        let mut prev_node = "Start((Start))".to_string();
        for (i, act) in activities_a.iter().enumerate().take(div_idx) {
            let id = format!("shared_{i}");

            let label = act
                .activity_type
                .as_deref()
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!("**{label}**<br>{}", Self::escape_label(detail))
            };

            let _ = writeln!(out, "    {id}[\"{node_text}\"]");
            if i == 0 {
                let _ = writeln!(out, "    {prev_node} --> {id}");
            } else {
                let _ = writeln!(out, "    shared_{} --> {id}", i - 1);
            }
            let _ = writeln!(out, "    class {id} shared");
            prev_node = id;
        }

        let branch_point = prev_node;

        // 2. Branch A
        let mut prev_a = branch_point.clone();
        for (i, act) in activities_a.iter().enumerate().skip(div_idx) {
            let id = format!("a_{i}");

            let label = act
                .activity_type
                .as_deref()
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!("**{label}**<br>{}", Self::escape_label(detail))
            };

            let _ = writeln!(out, "    {id}[\"{node_text}\"]");
            let _ = writeln!(out, "    {prev_a} --> {id}");
            let _ = writeln!(out, "    class {id} branchA");
            prev_a = id;
        }

        // 3. Branch B
        let mut prev_b = branch_point;
        for (i, act) in activities_b.iter().enumerate().skip(div_idx) {
            let id = format!("b_{i}");

            let label = act
                .activity_type
                .as_deref()
                .or(act.stage_name.as_deref())
                .unwrap_or("Activity");

            let detail = act.detail.as_deref().unwrap_or("");
            let node_text = if detail.is_empty() {
                label.to_string()
            } else {
                format!("**{label}**<br>{}", Self::escape_label(detail))
            };

            let _ = writeln!(out, "    {id}[\"{node_text}\"]");
            let _ = writeln!(out, "    {prev_b} --> {id}");
            let _ = writeln!(out, "    class {id} branchB");
            prev_b = id;
        }

        out
    }

    fn escape_label(s: &str) -> String {
        s.replace('"', "'").replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, SessionState, ToolUse};

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
            tool_use: tool.map(|(t, i)| ToolUse {
                tool_name: t.to_string(),
                input: i.to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }
    }

    #[test]
    fn test_multiverse_visualizer_mermaid_output() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "bar"))),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo"))),
            create_mock_activity("2", Some(("write_file", "baz"))),
        ];

        let report = ComparisonReport {
            session_a_id: "sessions/A".to_string(),
            session_b_id: "sessions/B".to_string(),
            activity_count_delta: 0,
            first_divergence_index: Some(1),
            files_a_only: vec![],
            files_b_only: vec![],
            files_intersection: vec![],
        };

        let visualizer = MultiverseVisualizer::new();
        let mermaid = visualizer.to_mermaid(
            &report,
            &session_a,
            &activities_a,
            &session_b,
            &activities_b,
        );

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("shared_0"));
        assert!(mermaid.contains("a_1"));
        assert!(mermaid.contains("b_1"));
        assert!(mermaid.contains("shared_0 --> a_1"));
        assert!(mermaid.contains("shared_0 --> b_1"));
        assert!(mermaid.contains("class a_1 branchA"));
        assert!(mermaid.contains("class b_1 branchB"));
    }
}
