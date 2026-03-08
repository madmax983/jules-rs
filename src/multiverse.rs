use std::fmt::Write;

use crate::{Activity, Session, SessionComparator};

#[derive(Default)]
pub struct MultiverseVisualizer {}

impl MultiverseVisualizer {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    #[must_use]
    pub fn generate(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);

        let mut output = String::new();
        let _ = writeln!(output, "```mermaid");
        let _ = writeln!(output, "flowchart TD");
        let _ = writeln!(
            output,
            "    classDef shared fill:#e2e8f0,stroke:#64748b,stroke-width:2px,color:#0f172a"
        );
        let _ = writeln!(
            output,
            "    classDef divergence fill:#fef08a,stroke:#ca8a04,stroke-width:2px,color:#854d0e"
        );
        let _ = writeln!(
            output,
            "    classDef a_only fill:#bfdbfe,stroke:#2563eb,stroke-width:2px,color:#1e3a8a"
        );
        let _ = writeln!(
            output,
            "    classDef b_only fill:#bbf7d0,stroke:#16a34a,stroke-width:2px,color:#14532d"
        );

        // Shared logic: output nodes up to the point of divergence.
        let mut prev_node: Option<String> = None;
        let divergence_idx = report.first_divergence_index.unwrap_or(activities_a.len());

        for (i, activity) in activities_a.iter().enumerate().take(divergence_idx) {
            let node_id = format!("shared_{i}");
            let label = Self::build_node_label(activity);

            let _ = writeln!(output, "    {node_id}[\"{label}\"]:::shared");

            if let Some(prev) = &prev_node {
                let _ = writeln!(output, "    {prev} --> {node_id}");
            }
            prev_node = Some(node_id);
        }

        // Divergence node
        let divergence_node = "divergence_point";
        let _ = writeln!(
            output,
            "    {divergence_node}{{\"Divergence\"}}:::divergence"
        );
        if let Some(prev) = prev_node {
            let _ = writeln!(output, "    {prev} --> {divergence_node}");
        }

        // Path A
        let mut prev_a = divergence_node.to_string();
        for (i, activity) in activities_a.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("a_{i}");
            let label = Self::build_node_label(activity);

            let _ = writeln!(output, "    {node_id}[\"{label}\"]:::a_only");
            let _ = writeln!(output, "    {prev_a} --> {node_id}");
            prev_a = node_id;
        }

        // Path B
        let mut prev_b = divergence_node.to_string();
        for (i, activity) in activities_b.iter().enumerate().skip(divergence_idx) {
            let node_id = format!("b_{i}");
            let label = Self::build_node_label(activity);

            let _ = writeln!(output, "    {node_id}[\"{label}\"]:::b_only");
            let _ = writeln!(output, "    {prev_b} --> {node_id}");
            prev_b = node_id;
        }

        let _ = writeln!(output, "```");
        output
    }

    fn build_node_label(activity: &Activity) -> String {
        let title: &str = if let Some(tool_use) = &activity.tool_use {
            &tool_use.tool_name
        } else if let Some(activity_type) = &activity.activity_type {
            activity_type
        } else if let Some(stage_name) = &activity.stage_name {
            stage_name
        } else {
            "Unknown"
        };
        // Escape quotes
        title.replace('"', "&quot;")
    }
}
