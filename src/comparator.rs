use crate::{Activity, Session};
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Default)]
pub struct SessionComparator;

#[derive(Debug, Clone)]
pub struct ComparisonReport {
    pub session_a_id: String,
    pub session_b_id: String,
    pub activity_count_delta: i64,
    pub first_divergence_index: Option<usize>,
    pub files_a_only: Vec<String>,
    pub files_b_only: Vec<String>,
    pub files_intersection: Vec<String>,
}

impl SessionComparator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn compare(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> ComparisonReport {
        let count_a = activities_a.len();
        let count_b = activities_b.len();

        // Use try_from to safely cast, or saturating_sub/cast with check if preferred.
        // Since activity counts are unlikely to exceed i64::MAX, we can cast but allow potential wrap
        // or just use i128 intermediate if we were paranoid.
        // For this project context, strict casting is fine but clippy warns about potential wrap on 64-bit systems.
        // We will suppress the clippy warning as activity counts > 2^63 are impossible in practice.
        #[allow(clippy::cast_possible_wrap)]
        let activity_count_delta = (count_a as i64) - (count_b as i64);

        // Find divergence
        let mut first_divergence_index = None;
        let min_len = count_a.min(count_b);
        for i in 0..min_len {
            let act_a = &activities_a[i];
            let act_b = &activities_b[i];

            let tool_a = act_a.tool_use.as_ref().map(|t| (&t.tool_name, &t.input));
            let tool_b = act_b.tool_use.as_ref().map(|t| (&t.tool_name, &t.input));

            if tool_a != tool_b || act_a.status != act_b.status {
                first_divergence_index = Some(i);
                break;
            }
        }

        // If no divergence found yet but lengths differ, the divergence is at the end of the shorter one
        if first_divergence_index.is_none() && count_a != count_b {
            first_divergence_index = Some(min_len);
        }

        // Modified files analysis
        let get_files = |acts: &[Activity]| -> HashSet<String> {
            let mut files = HashSet::new();
            for act in acts {
                if let Some(output) = &act.output {
                    for file in &output.changed_files {
                        files.insert(file.path.clone());
                    }
                }
            }
            files
        };

        let files_a = get_files(activities_a);
        let files_b = get_files(activities_b);

        let mut unique_files_a: Vec<_> = files_a.difference(&files_b).cloned().collect();
        let mut unique_files_b: Vec<_> = files_b.difference(&files_a).cloned().collect();
        let mut files_intersection: Vec<_> = files_a.intersection(&files_b).cloned().collect();

        unique_files_a.sort();
        unique_files_b.sort();
        files_intersection.sort();

        ComparisonReport {
            session_a_id: session_a.name.clone(),
            session_b_id: session_b.name.clone(),
            activity_count_delta,
            first_divergence_index,
            files_a_only: unique_files_a,
            files_b_only: unique_files_b,
            files_intersection,
        }
    }
}

impl fmt::Display for ComparisonReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Session Comparison Report")?;
        writeln!(f, "=========================")?;
        writeln!(f, "Session A: {}", self.session_a_id)?;
        writeln!(f, "Session B: {}", self.session_b_id)?;
        writeln!(f)?;

        writeln!(f, "Activity Count Delta: {:+}", self.activity_count_delta)?;
        if let Some(idx) = self.first_divergence_index {
            writeln!(f, "First Divergence: Activity #{}", idx + 1)?;
        } else {
            writeln!(f, "First Divergence: None (Sessions are identical in flow)")?;
        }
        writeln!(f)?;

        writeln!(f, "Unique Modified Files (A):")?;
        if self.files_a_only.is_empty() {
            writeln!(f, "  (None)")?;
        } else {
            for file in &self.files_a_only {
                writeln!(f, "  - {file}")?;
            }
        }

        writeln!(f, "\nUnique Modified Files (B):")?;
        if self.files_b_only.is_empty() {
            writeln!(f, "  (None)")?;
        } else {
            for file in &self.files_b_only {
                writeln!(f, "  - {file}")?;
            }
        }

        writeln!(f, "\nShared Modified Files:")?;
        if self.files_intersection.is_empty() {
            writeln!(f, "  (None)")?;
        } else {
            for file in &self.files_intersection {
                writeln!(f, "  - {file}")?;
            }
        }

        Ok(())
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
    fn test_compare_divergence() {
        let session_a = create_mock_session("sessions/A");
        let session_b = create_mock_session("sessions/B");

        let activities_a = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "bar")), vec!["src/main.rs"]),
        ];

        let activities_b = vec![
            create_mock_activity("1", Some(("read_file", "foo")), vec![]),
            create_mock_activity("2", Some(("write_file", "baz")), vec!["src/lib.rs"]),
        ];

        let comparator = SessionComparator::new();
        let report = comparator.compare(&session_a, &activities_a, &session_b, &activities_b);

        assert_eq!(report.activity_count_delta, 0);
        assert_eq!(report.first_divergence_index, Some(1));
        assert!(report.files_a_only.contains(&"src/main.rs".to_string()));
        assert!(report.files_b_only.contains(&"src/lib.rs".to_string()));
        assert!(report.files_intersection.is_empty());
    }
}
