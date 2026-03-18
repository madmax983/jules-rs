use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session};

/// Generates a Mermaid flowchart comparing two sessions.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    #[allow(clippy::too_many_lines, clippy::similar_names)]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        acts_a: &[Activity],
        session_b: &Session,
        acts_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Multiverse Session Comparison");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        // Styling
        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef shared fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef a_only fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef b_only fill:#fff3e0,stroke:#e65100,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#fff9c4,stroke:#fbc02d,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, acts_a, session_b, acts_b);
        let divergence_idx = report
            .first_divergence_index
            .unwrap_or(acts_a.len().min(acts_b.len()));

        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        // Shared Path
        for (i, act) in acts_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_{i}");
            let label = Self::get_activity_label(act);
            let detail = act.detail.as_deref().unwrap_or("");
            let text = Self::format_node_text(&label, detail);

            let (open, close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{open}\"{text}\"{close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        let diverge_node = prev_node;

        // Session A Branch
        let _ = writeln!(out, "    subgraph SessionA [\"{}\"]", session_a.name);
        let mut prev_a = diverge_node.clone();
        for (i, act) in acts_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_{i}");
            let label = Self::get_activity_label(act);
            let detail = act.detail.as_deref().unwrap_or("");
            let text = Self::format_node_text(&label, detail);

            let (open, close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{open}\"{text}\"{close}");
            let _ = writeln!(out, "    {prev_a} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} a_only");
            prev_a = node_id;
        }
        let _ = writeln!(
            out,
            "    EndA(({:?}))",
            session_a
                .state
                .as_ref()
                .unwrap_or(&crate::SessionState::Failed)
        );
        let _ = writeln!(out, "    {prev_a} --> EndA");
        let _ = writeln!(out, "    end");

        // Session B Branch
        let _ = writeln!(out, "    subgraph SessionB [\"{}\"]", session_b.name);
        let mut prev_b = diverge_node;
        for (i, act) in acts_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_{i}");
            let label = Self::get_activity_label(act);
            let detail = act.detail.as_deref().unwrap_or("");
            let text = Self::format_node_text(&label, detail);

            let (open, close) = if act.tool_use.is_some() {
                ("{{", "}}")
            } else {
                ("[", "]")
            };

            let _ = writeln!(out, "    {node_id}{open}\"{text}\"{close}");
            let _ = writeln!(out, "    {prev_b} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} b_only");
            prev_b = node_id;
        }
        let _ = writeln!(
            out,
            "    EndB(({:?}))",
            session_b
                .state
                .as_ref()
                .unwrap_or(&crate::SessionState::Failed)
        );
        let _ = writeln!(out, "    {prev_b} --> EndB");
        let _ = writeln!(out, "    end");

        out
    }

    fn get_activity_label(act: &Activity) -> String {
        act.activity_type
            .as_deref()
            .or(act.stage_name.as_deref())
            .unwrap_or("Activity")
            .to_string()
    }

    fn format_node_text(label: &str, detail: &str) -> String {
        if detail.is_empty() {
            label.to_string()
        } else {
            format!("**{label}**<br>{}", SessionVisualizer::escape_label(detail))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState, ToolUse};

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
    #[allow(clippy::too_many_lines)]
    fn test_multiverse_mermaid_output() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let acts_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "bar")), vec!["src/main.rs"]),
        ];

        let acts_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "baz")), vec!["src/lib.rs"]),
        ];

        let viz = MultiverseVisualizer::new();
        let mermaid = viz.to_mermaid(&session_a, &acts_a, &session_b, &acts_b);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("-->"));
        assert!(mermaid.contains("sessions/A"));
        assert!(mermaid.contains("sessions/B"));
    }
}
