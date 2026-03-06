use crate::{FindingSeverity, ReviewFinding};
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
        findings: &[ReviewFinding],
    ) -> std::io::Result<String> {
        let mut report = String::new();
        let modified_set: HashSet<String> = modified_files.iter().cloned().collect();

        // Group findings by file path to determine the highest severity per file
        let mut file_severities: HashMap<String, FindingSeverity> = HashMap::new();
        for finding in findings {
            let current_severity = file_severities
                .get(&finding.file_path)
                .copied()
                .unwrap_or(FindingSeverity::Info);

            let new_severity = match (current_severity, finding.severity) {
                (FindingSeverity::Error, _) | (_, FindingSeverity::Error) => FindingSeverity::Error,
                (FindingSeverity::Warning, _) | (_, FindingSeverity::Warning) => {
                    FindingSeverity::Warning
                }
                _ => FindingSeverity::Info,
            };

            file_severities.insert(finding.file_path.clone(), new_severity);
        }

        // We use unwrap() here because writing to a String is infallible.
        writeln!(report, "=== PROJECT RISK HEATMAP ===").unwrap();
        writeln!(report, "Root: {}", root.display()).unwrap();
        writeln!(report, "Modified Files: {}", modified_files.len()).unwrap();
        writeln!(report, "Files with Findings: {}", file_severities.len()).unwrap();
        writeln!(report, "\n--- DIRECTORY RISK HEATMAP ---").unwrap();

        if let Err(e) = Self::walk(root, root, 0, &mut report, &modified_set, &file_severities) {
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
        file_severities: &HashMap<String, FindingSeverity>,
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
        // Sort for consistent output
        entries_vec.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries_vec {
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Skip hidden files and common build artifacts
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
                Self::walk(
                    root,
                    &path,
                    depth + 1,
                    report,
                    modified_set,
                    file_severities,
                )?;
            } else {
                let mut relative_path_str = String::new();
                if let Ok(rel) = path.strip_prefix(root) {
                    relative_path_str = rel.to_string_lossy().replace('\\', "/");
                }

                let mut badges = String::new();
                if modified_set.contains(&relative_path_str) {
                    badges.push_str(" 🔥");
                }

                if let Some(severity) = file_severities.get(&relative_path_str) {
                    match severity {
                        FindingSeverity::Error => badges.push_str(" ❌"),
                        FindingSeverity::Warning => badges.push_str(" ⚠️"),
                        FindingSeverity::Info => badges.push_str(" ℹ️"),
                    }
                }

                if badges.is_empty() {
                    writeln!(report, "{prefix}📄 {name}").unwrap();
                } else {
                    writeln!(report, "{prefix}📄 {name}{badges}").unwrap();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_generate_risk_heatmap() {
        // Create a temporary directory structure for testing
        let temp_dir =
            std::env::temp_dir().join(format!("jules_risk_heatmap_test_{}", std::process::id()));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).unwrap();
        }
        fs::create_dir_all(&temp_dir).unwrap();

        // Create some files
        File::create(temp_dir.join("README.md")).unwrap();
        File::create(temp_dir.join("main.rs")).unwrap();

        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).unwrap();
        File::create(src_dir.join("lib.rs")).unwrap();
        File::create(src_dir.join("utils.rs")).unwrap();
        File::create(src_dir.join("error.rs")).unwrap();

        let modified_files = vec![
            "src/lib.rs".to_string(),
            "README.md".to_string(),
            "src/error.rs".to_string(),
        ];

        let findings = vec![
            ReviewFinding {
                file_path: "src/lib.rs".to_string(),
                line_number: Some(10),
                severity: FindingSeverity::Warning,
                message: "unwrap() detected".to_string(),
            },
            ReviewFinding {
                file_path: "src/error.rs".to_string(),
                line_number: Some(20),
                severity: FindingSeverity::Error,
                message: "Critical error".to_string(),
            },
            ReviewFinding {
                file_path: "src/error.rs".to_string(),
                line_number: Some(25),
                severity: FindingSeverity::Warning,
                message: "Another issue".to_string(),
            }, // Error should take precedence
        ];

        let heatmap = ProjectRiskHeatmap::new();
        let report = heatmap
            .generate(&temp_dir, &modified_files, &findings)
            .unwrap();

        // Clean up
        fs::remove_dir_all(&temp_dir).unwrap();

        println!("{report}");

        assert!(report.contains("=== PROJECT RISK HEATMAP ==="));
        assert!(report.contains("📄 README.md 🔥"));
        assert!(!report.contains("📄 README.md 🔥 ⚠️"));
        assert!(report.contains("📄 main.rs"));
        assert!(!report.contains("📄 main.rs 🔥"));
        assert!(report.contains("📂 src"));
        assert!(report.contains("📄 lib.rs 🔥 ⚠️"));
        assert!(report.contains("📄 error.rs 🔥 ❌"));
        assert!(report.contains("📄 utils.rs"));
        assert!(!report.contains("📄 utils.rs 🔥"));
    }
}
