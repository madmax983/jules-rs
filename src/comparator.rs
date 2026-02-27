use std::fmt::{self, Display, Formatter};

use crate::{Activity, Session};

/// Compares two sessions to identify divergences in execution and outcome.
#[derive(Debug, Default)]
pub struct SessionComparator;

/// Report containing the comparison results between two sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonReport {
    /// ID of the first session.
    pub session_a_id: String,
    /// ID of the second session.
    pub session_b_id: String,
    /// Total activities in session A.
    pub session_a_activity_count: usize,
    /// Total activities in session B.
    pub session_b_activity_count: usize,
    /// The index of the first activity where the sessions diverge.
    pub divergence_index: Option<usize>,
    /// Description of the divergence (e.g., "Different tool used").
    pub divergence_reason: Option<String>,
    /// Files modified in session A but not B.
    pub unique_files_a: Vec<String>,
    /// Files modified in session B but not A.
    pub unique_files_b: Vec<String>,
}

impl SessionComparator {
    /// Creates a new comparator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Compares two sessions and their activities.
    #[must_use]
    pub fn compare(
        &self,
        session_a: &Session,
        activities_a: &[Activity],
        session_b: &Session,
        activities_b: &[Activity],
    ) -> ComparisonReport {
        let divergence = self.find_divergence(activities_a, activities_b);
        let (unique_files_a, unique_files_b) = self.compare_files(activities_a, activities_b);

        ComparisonReport {
            session_a_id: session_a.name.clone(),
            session_b_id: session_b.name.clone(),
            session_a_activity_count: activities_a.len(),
            session_b_activity_count: activities_b.len(),
            divergence_index: divergence.as_ref().map(|(i, _)| *i),
            divergence_reason: divergence.map(|(_, r)| r),
            unique_files_a,
            unique_files_b,
        }
    }

    fn find_divergence(
        &self,
        activities_a: &[Activity],
        activities_b: &[Activity],
    ) -> Option<(usize, String)> {
        let len = activities_a.len().min(activities_b.len());

        for i in 0..len {
            let a = &activities_a[i];
            let b = &activities_b[i];

            // 1. Check Tool Use
            if let (Some(tool_a), Some(tool_b)) = (&a.tool_use, &b.tool_use) {
                if tool_a.tool_name != tool_b.tool_name {
                    return Some((
                        i,
                        format!(
                            "Different tool: '{}' vs '{}'",
                            tool_a.tool_name, tool_b.tool_name
                        ),
                    ));
                }
                // We might ignore input differences if we want to be lenient,
                // but strict comparison is safer for now.
                if tool_a.input != tool_b.input {
                    return Some((
                        i,
                        format!(
                            "Different tool input for '{}'",
                            tool_a.tool_name
                        ),
                    ));
                }
            } else if a.tool_use.is_some() != b.tool_use.is_some() {
                return Some((i, "One session used a tool, the other did not".to_string()));
            }

            // 2. Check Activity Status (e.g. one failed, one succeeded)
            if a.status != b.status {
                return Some((
                    i,
                    format!("Status mismatch: {:?} vs {:?}", a.status, b.status),
                ));
            }
        }

        if activities_a.len() != activities_b.len() {
            return Some((
                len,
                format!(
                    "Length mismatch: Session A has {} activities, Session B has {}",
                    activities_a.len(),
                    activities_b.len()
                ),
            ));
        }

        None
    }

    fn compare_files(
        &self,
        activities_a: &[Activity],
        activities_b: &[Activity],
    ) -> (Vec<String>, Vec<String>) {
        let files_a = self.collect_files(activities_a);
        let files_b = self.collect_files(activities_b);

        let unique_a = files_a
            .difference(&files_b)
            .cloned()
            .collect::<Vec<_>>();
        let unique_b = files_b
            .difference(&files_a)
            .cloned()
            .collect::<Vec<_>>();

        // Sorting for deterministic output
        let mut sorted_a = unique_a;
        sorted_a.sort();
        let mut sorted_b = unique_b;
        sorted_b.sort();

        (sorted_a, sorted_b)
    }

    fn collect_files(&self, activities: &[Activity]) -> std::collections::HashSet<String> {
        let mut files = std::collections::HashSet::new();
        for activity in activities {
            if let Some(output) = &activity.output {
                for file in &output.changed_files {
                    files.insert(file.path.clone());
                }
            }
        }
        files
    }
}

impl Display for ComparisonReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SESSION COMPARISON REPORT ===")?;
        writeln!(f, "Session A: {}", self.session_a_id)?;
        writeln!(f, "Session B: {}", self.session_b_id)?;
        writeln!(f)?;

        writeln!(f, "--- ACTIVITY SUMMARY ---")?;
        writeln!(f, "Count A: {}", self.session_a_activity_count)?;
        writeln!(f, "Count B: {}", self.session_b_activity_count)?;

        if let Some(idx) = self.divergence_index {
            writeln!(f, "DIVERGENCE FOUND at index {idx}")?;
            if let Some(reason) = &self.divergence_reason {
                writeln!(f, "Reason: {reason}")?;
            }
        } else {
            writeln!(f, "NO DIVERGENCE DETECTED (in common length)")?;
        }
        writeln!(f)?;

        writeln!(f, "--- FILE OUTCOME DIFFERENCES ---")?;
        if self.unique_files_a.is_empty() && self.unique_files_b.is_empty() {
            writeln!(f, "Both sessions modified the same set of files.")?;
        } else {
            if !self.unique_files_a.is_empty() {
                writeln!(f, "Only in A:")?;
                for file in &self.unique_files_a {
                    writeln!(f, "  - {file}")?;
                }
            }
            if !self.unique_files_b.is_empty() {
                writeln!(f, "Only in B:")?;
                for file in &self.unique_files_b {
                    writeln!(f, "  - {file}")?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, ChangedFile, SessionOutput, SessionState, ToolUse};

    fn mock_session(name: &str) -> Session {
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

    fn mock_activity(
        tool: Option<(&str, &str)>,
        files: &[&str],
        status: ActivityStatus,
    ) -> Activity {
        Activity {
            name: "act".to_string(),
            status: Some(status),
            stage_name: None,
            activity_type: None,
            detail: None,
            timestamp: None,
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: tool.map(|(n, i)| ToolUse {
                tool_name: n.to_string(),
                input: i.to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: if files.is_empty() {
                None
            } else {
                Some(SessionOutput {
                    changed_files: files
                        .iter()
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
    fn test_identical_sessions() {
        let s1 = mock_session("s1");
        let s2 = mock_session("s2");
        let acts = vec![mock_activity(
            Some(("ls", ".")),
            &[],
            ActivityStatus::Success,
        )];

        let comparator = SessionComparator::new();
        let report = comparator.compare(&s1, &acts, &s2, &acts);

        assert_eq!(report.divergence_index, None);
        assert!(report.unique_files_a.is_empty());
        assert!(report.unique_files_b.is_empty());
    }

    #[test]
    fn test_divergence_tool_name() {
        let s1 = mock_session("s1");
        let s2 = mock_session("s2");
        let acts_a = vec![mock_activity(
            Some(("ls", ".")),
            &[],
            ActivityStatus::Success,
        )];
        let acts_b = vec![mock_activity(
            Some(("grep", ".")),
            &[],
            ActivityStatus::Success,
        )];

        let comparator = SessionComparator::new();
        let report = comparator.compare(&s1, &acts_a, &s2, &acts_b);

        assert_eq!(report.divergence_index, Some(0));
        assert!(report
            .divergence_reason
            .unwrap()
            .contains("Different tool"));
    }

    #[test]
    fn test_divergence_length() {
        let s1 = mock_session("s1");
        let s2 = mock_session("s2");
        let acts_a = vec![mock_activity(
            Some(("ls", ".")),
            &[],
            ActivityStatus::Success,
        )];
        let acts_b = vec![
            mock_activity(Some(("ls", ".")), &[], ActivityStatus::Success),
            mock_activity(Some(("ls", ".")), &[], ActivityStatus::Success),
        ];

        let comparator = SessionComparator::new();
        let report = comparator.compare(&s1, &acts_a, &s2, &acts_b);

        assert_eq!(report.divergence_index, Some(1));
        assert!(report
            .divergence_reason
            .unwrap()
            .contains("Length mismatch"));
    }

    #[test]
    fn test_file_diffs() {
        let s1 = mock_session("s1");
        let s2 = mock_session("s2");
        let acts_a = vec![mock_activity(None, &["file1.txt"], ActivityStatus::Success)];
        let acts_b = vec![mock_activity(None, &["file2.txt"], ActivityStatus::Success)];

        let comparator = SessionComparator::new();
        let report = comparator.compare(&s1, &acts_a, &s2, &acts_b);

        assert_eq!(report.unique_files_a, vec!["file1.txt"]);
        assert_eq!(report.unique_files_b, vec!["file2.txt"]);
    }
}
