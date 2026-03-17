use std::fmt::Write;

use crate::comparator::SessionComparator;
use crate::visualizer::SessionVisualizer;
use crate::{Activity, Session};

/// Visualizes the divergence between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart detailing how two sessions diverge.
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();
        // Mermaid header
        let _ = writeln!(out, "graph TD");
        let _ = writeln!(out, "    %% Session A: {}", session_a.name);
        let _ = writeln!(out, "    %% Session B: {}", session_b.name);

        // Styling
        let _ = writeln!(
            out,
            "    classDef default fill:#f9f9f9,stroke:#333,stroke-width:1px;"
        );
        let _ = writeln!(
            out,
            "    classDef tool fill:#e8f5e9,stroke:#2e7d32,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchA fill:#ffebee,stroke:#c62828,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef branchB fill:#e3f2fd,stroke:#1565c0,stroke-width:2px;"
        );

        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let divergence_index = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "    Start((Start))");

        // Shared Subgraph
        let mut prev_node = "Start".to_string();
        if divergence_index > 0 {
            let _ = writeln!(out, "    subgraph Shared[\"Shared Timeline\"]");
            let _ = writeln!(out, "    direction TB");
            for (i, activity) in activities_a.iter().enumerate().take(divergence_index) {
                let node_id = format!("shared_{i}");
                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_node} --> {node_id}");
                if activity.tool_use.is_some() {
                    let _ = writeln!(out, "    class {node_id} tool");
                }
                prev_node = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        let branch_point = prev_node;

        // Session A Divergence
        if divergence_index < activities_a.len() {
            let _ = writeln!(out, "    subgraph SessionA[\"Session A\"]");
            let _ = writeln!(out, "    direction TB");
            let mut prev_a = branch_point.clone();
            for (i, activity) in activities_a.iter().enumerate().skip(divergence_index) {
                let node_id = format!("act_a_{i}");
                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_a} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} branchA");
                prev_a = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        // Session B Divergence
        if divergence_index < activities_b.len() {
            let _ = writeln!(out, "    subgraph SessionB[\"Session B\"]");
            let _ = writeln!(out, "    direction TB");
            let mut prev_b = branch_point;
            for (i, activity) in activities_b.iter().enumerate().skip(divergence_index) {
                let node_id = format!("act_b_{i}");
                let label = Self::get_activity_label(activity);
                let (shape_open, shape_close) = if activity.tool_use.is_some() {
                    ("{{", "}}")
                } else {
                    ("[", "]")
                };

                let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
                let _ = writeln!(out, "    {prev_b} --> {node_id}");
                let _ = writeln!(out, "    class {node_id} branchB");
                prev_b = node_id;
            }
            let _ = writeln!(out, "    end");
        }

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        let label = if let Some(tool) = &activity.tool_use {
            &tool.tool_name
        } else if let Some(t) = &activity.activity_type {
            t
        } else if let Some(s) = &activity.stage_name {
            s
        } else {
            "Activity"
        };
        SessionVisualizer::escape_label(label)
    }
}
