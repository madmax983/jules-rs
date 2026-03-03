use crate::SessionOutput;
use serde::Serialize;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Severity level for a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingSeverity {
    /// Informational note or minor suggestion.
    Info,
    /// Potential issue that should be reviewed.
    Warning,
    /// Critical issue that must be fixed.
    Error,
}

impl Display for FindingSeverity {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// A specific finding identified during a code review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    /// Path of the file related to the finding.
    pub file_path: String,
    /// Line number where the finding occurred, if applicable.
    pub line_number: Option<usize>,
    /// Severity of the finding.
    pub severity: FindingSeverity,
    /// Descriptive message explaining the finding.
    pub message: String,
}

/// The complete review report for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReport {
    /// Total lines added in the session.
    pub lines_added: usize,
    /// Total lines removed in the session.
    pub lines_removed: usize,
    /// List of findings identified during the review.
    pub findings: Vec<ReviewFinding>,
}

impl Display for ReviewReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Session Review Report")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Lines Added:   +{}", self.lines_added)?;
        writeln!(f, "Lines Removed: -{}", self.lines_removed)?;
        writeln!(f)?;

        if self.findings.is_empty() {
            writeln!(f, "No issues found.")?;
            return Ok(());
        }

        writeln!(f, "Findings:")?;
        for finding in &self.findings {
            let line_info = match finding.line_number {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            writeln!(
                f,
                "[{}] {}{} - {}",
                finding.severity, finding.file_path, line_info, finding.message
            )?;
        }

        Ok(())
    }
}

/// Analyzes session outputs to generate a code review report.
#[derive(Default)]
pub struct SessionReviewer {}

impl SessionReviewer {
    /// Creates a new `SessionReviewer`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Analyzes the provided session outputs and generates a `ReviewReport`.
    #[must_use]
    pub fn review_session(&self, outputs: &[SessionOutput]) -> ReviewReport {
        let mut lines_added = 0;
        let mut lines_removed = 0;
        let mut findings = Vec::new();
        let mut has_test_changes = false;

        for output in outputs {
            for changed_file in &output.changed_files {
                let path = &changed_file.path;
                let is_rust_file = std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"));
                let is_test_file = path.contains("test") || path.contains("spec");

                if is_test_file {
                    has_test_changes = true;
                }

                let mut current_line_number = 0;

                for line in changed_file.diff.lines() {
                    // Approximate line counting for unified diff chunks
                    if line.starts_with("@@ ") {
                        if let Some(chunk_info) = line.split(" +").nth(1) {
                            if let Some(start_line_str) = chunk_info.split(',').next() {
                                if let Ok(start_line) = start_line_str.parse::<usize>() {
                                    current_line_number = start_line;
                                }
                            }
                        }
                    } else if line.starts_with('+') && !line.starts_with("+++") {
                        lines_added += 1;

                        // Rule: detect unwrap()
                        if is_rust_file && line.contains("unwrap()") {
                            findings.push(ReviewFinding {
                                file_path: path.clone(),
                                line_number: Some(current_line_number),
                                severity: FindingSeverity::Warning,
                                message: "Usage of `unwrap()` detected. Consider using safe error handling (e.g., `?` or `match`) instead.".to_string(),
                            });
                        }

                        // Rule: detect TODO or FIXME
                        if line.contains("TODO") || line.contains("FIXME") {
                            findings.push(ReviewFinding {
                                file_path: path.clone(),
                                line_number: Some(current_line_number),
                                severity: FindingSeverity::Info,
                                message: "TODO or FIXME comment added. Consider tracking this as an issue.".to_string(),
                            });
                        }

                        current_line_number += 1;
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        lines_removed += 1;
                    } else if line.starts_with(' ') {
                        current_line_number += 1;
                    }
                }
            }
        }

        if lines_added > 0 && !has_test_changes {
            findings.push(ReviewFinding {
                file_path: "Global".to_string(),
                line_number: None,
                severity: FindingSeverity::Warning,
                message: "Code was modified but no test files were changed. Consider adding or updating tests.".to_string(),
            });
        }

        ReviewReport {
            lines_added,
            lines_removed,
            findings,
        }
    }
}
