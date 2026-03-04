use crate::{FindingSeverity, ReviewReport};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::Path;

/// Generates a directory tree heatmap highlighting modified files and review findings.
#[derive(Default)]
pub struct ProjectRiskHeatmap;

impl ProjectRiskHeatmap {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Scans the project directory and returns a formatted risk heatmap report.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if directory traversal fails.
    pub fn generate(
        &self,
        root: &Path,
        modified_files: &[String],
        review_report: &ReviewReport,
    ) -> std::io::Result<String> {
        let mut report = String::new();
        let modified_set: HashSet<String> = modified_files.iter().cloned().collect();

        // Map file path -> highest severity
        let mut file_severity: HashMap<String, FindingSeverity> = HashMap::new();
        for finding in &review_report.findings {
            let path = finding.file_path.clone();
            let current = file_severity.get(&path).copied();
            let new_severity = match (current, finding.severity) {
                (Some(FindingSeverity::Error), _) | (_, FindingSeverity::Error) => {
                    FindingSeverity::Error
                }
                (Some(FindingSeverity::Warning), _) | (_, FindingSeverity::Warning) => {
                    FindingSeverity::Warning
                }
                _ => FindingSeverity::Info,
            };
            file_severity.insert(path, new_severity);
        }

        writeln!(report, "=== PROJECT RISK HEATMAP ===").unwrap();
        writeln!(report, "Root: {}", root.display()).unwrap();
        writeln!(report, "Modified Files: {}", modified_files.len()).unwrap();
        writeln!(report, "Review Findings: {}", review_report.findings.len()).unwrap();
        writeln!(report, "\n--- DIRECTORY RISK HEATMAP ---").unwrap();

        if let Err(e) = Self::walk(root, root, 0, &mut report, &modified_set, &file_severity) {
            writeln!(report, "Error walking directory: {e}").unwrap();
        }

        Ok(report)
    }

    fn walk(
        root: &Path,
        dir: &Path,
        depth: usize,
        report: &mut String,
        modified_set: &HashSet<String>,
        file_severity: &HashMap<String, FindingSeverity>,
    ) -> std::io::Result<()> {
        if depth > 5 {
            return Ok(());
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(());
        };

        let mut entries_vec = Vec::new();
        for entry in entries.flatten() {
            entries_vec.push(entry);
        }
        entries_vec.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries_vec {
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if name.starts_with('.')
                || name == "target"
                || name == "node_modules"
                || name == "dist"
                || name == "build"
                || name == "venv"
                || name == "__pycache__"
            {
                continue;
            }

            let prefix = "  ".repeat(depth);
            if path.is_dir() {
                writeln!(report, "{prefix}📂 {name}").unwrap();
                Self::walk(root, &path, depth + 1, report, modified_set, file_severity)?;
            } else {
                let mut relative_path_str = String::new();
                if let Ok(rel) = path.strip_prefix(root) {
                    relative_path_str = rel.to_string_lossy().replace('\\', "/");
                }

                if let Some(severity) = file_severity.get(&relative_path_str) {
                    writeln!(
                        report,
                        "{prefix}📄 {name} ⚠️ ({})",
                        severity.to_string().to_uppercase()
                    )
                    .unwrap();
                } else if modified_set.contains(&relative_path_str) {
                    writeln!(report, "{prefix}📄 {name} 🔥 (Modified)").unwrap();
                } else {
                    writeln!(report, "{prefix}📄 {name}").unwrap();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ReviewFinding;
    use std::fs::{self, File};

    #[test]
    fn test_risk_heatmap_generation() {
        let temp_dir =
            std::env::temp_dir().join(format!("jules_risk_heatmap_test_{}", std::process::id()));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).unwrap();
        }
        fs::create_dir_all(&temp_dir).unwrap();

        File::create(temp_dir.join("README.md")).unwrap();
        File::create(temp_dir.join("main.rs")).unwrap();

        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).unwrap();
        File::create(src_dir.join("lib.rs")).unwrap();
        File::create(src_dir.join("utils.rs")).unwrap();

        let modified_files = vec!["src/lib.rs".to_string(), "README.md".to_string()];

        let review_report = ReviewReport {
            lines_added: 10,
            lines_removed: 2,
            findings: vec![
                ReviewFinding {
                    file_path: "src/lib.rs".to_string(),
                    line_number: Some(10),
                    severity: FindingSeverity::Warning,
                    message: "unwrap()".to_string(),
                },
                ReviewFinding {
                    file_path: "src/lib.rs".to_string(),
                    line_number: Some(15),
                    severity: FindingSeverity::Error,
                    message: "TODO".to_string(),
                },
            ],
        };

        let heatmap = ProjectRiskHeatmap::new();
        let report = heatmap
            .generate(&temp_dir, &modified_files, &review_report)
            .unwrap();

        fs::remove_dir_all(&temp_dir).unwrap();

        println!("{report}");

        assert!(report.contains("=== PROJECT RISK HEATMAP ==="));
        assert!(report.contains("📄 README.md 🔥 (Modified)"));
        assert!(report.contains("📄 main.rs"));
        assert!(!report.contains("📄 main.rs 🔥 (Modified)"));
        assert!(report.contains("📂 src"));
        assert!(report.contains("📄 lib.rs ⚠️ (ERROR)"));
        assert!(report.contains("📄 utils.rs"));
    }
}
