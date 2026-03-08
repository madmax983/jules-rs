use std::fmt::Write;

use crate::{Activity, Session, SessionComparator};

/// Visualizes the divergence between two sessions as a Mermaid diagram.
#[derive(Debug, Default)]
pub struct MultiverseVisualizer;

impl MultiverseVisualizer {
    /// Creates a new visualizer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a Mermaid flowchart showing the shared path and divergence of two sessions.
    #[must_use]
    pub fn to_mermaid(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> String {
        let mut out = String::new();

        // Use SessionComparator to find the divergence index
        let comparator = SessionComparator::new();
        let report = comparator.compare(session_a, activities_a, session_b, activities_b);
        let divergence_index = report
            .first_divergence_index
            .unwrap_or_else(|| activities_a.len().min(activities_b.len()));

        let _ = writeln!(out, "graph TD");
        let _ = writeln!(
            out,
            "    %% Multiverse: {} vs {}",
            session_a.name, session_b.name
        );

        // Styling
        let _ = writeln!(
            out,
            "    classDef shared fill:#e0e0e0,stroke:#9e9e9e,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef sessionA fill:#e3f2fd,stroke:#1e88e5,stroke-width:2px;"
        );
        let _ = writeln!(
            out,
            "    classDef sessionB fill:#fce4ec,stroke:#d81b60,stroke-width:2px;"
        );

        // Start Node
        let _ = writeln!(out, "    Start((Start))");
        let mut prev_node = "Start".to_string();

        // 1. Shared Path
        for (i, act) in activities_a.iter().enumerate().take(divergence_index) {
            let node_id = format!("shared_{i}");
            let label = Self::get_activity_label(act);

            let (shape_open, shape_close) = Self::get_node_shape(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {prev_node} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} shared");
            prev_node = node_id;
        }

        let branch_point = prev_node;

        // 2. Session A Divergence
        let mut a_prev = branch_point.clone();
        for (i, act) in activities_a.iter().enumerate().skip(divergence_index) {
            let node_id = format!("a_{i}");
            let label = Self::get_activity_label(act);

            let (shape_open, shape_close) = Self::get_node_shape(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {a_prev} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} sessionA");
            a_prev = node_id;
        }

        let a_status = session_a
            .state
            .as_ref()
            .map_or("Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndA(({a_status}))");
        let _ = writeln!(out, "    {a_prev} --> EndA");
        let _ = writeln!(out, "    class EndA sessionA");

        // 3. Session B Divergence
        let mut b_prev = branch_point;
        for (i, act) in activities_b.iter().enumerate().skip(divergence_index) {
            let node_id = format!("b_{i}");
            let label = Self::get_activity_label(act);

            let (shape_open, shape_close) = Self::get_node_shape(act);
            let _ = writeln!(out, "    {node_id}{shape_open}\"{label}\"{shape_close}");
            let _ = writeln!(out, "    {b_prev} --> {node_id}");
            let _ = writeln!(out, "    class {node_id} sessionB");
            b_prev = node_id;
        }

        let b_status = session_b
            .state
            .as_ref()
            .map_or("Unknown".to_string(), |s| format!("{s:?}"));
        let _ = writeln!(out, "    EndB(({b_status}))");
        let _ = writeln!(out, "    {b_prev} --> EndB");
        let _ = writeln!(out, "    class EndB sessionB");

        out
    }

    fn get_activity_label(activity: &Activity) -> String {
        let label = activity
            .tool_use
            .as_ref()
            .map(|t| t.tool_name.as_str())
            .or(activity.activity_type.as_deref())
            .or(activity.stage_name.as_deref())
            .unwrap_or("Activity");

        let detail = activity.detail.as_deref().unwrap_or("");

        if detail.is_empty() {
            label.to_string()
        } else {
            format!(
                "**{label}**<br>{}",
                detail.replace('"', "'").replace('\n', "<br>")
            )
        }
    }

    fn get_node_shape(activity: &Activity) -> (&'static str, &'static str) {
        if activity.tool_use.is_some() {
            ("{{", "}}")
        } else {
            ("[", "]")
        }
    }
}
