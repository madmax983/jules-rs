use crate::reviewer::{FindingSeverity, ReviewFinding};
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

        // Group findings by file path to easily look up severity later
        let mut file_findings: HashMap<String, Vec<FindingSeverity>> = HashMap::new();
        for finding in findings {
            file_findings
                .entry(finding.file_path.clone())
                .or_default()
                .push(finding.severity);
        }

        writeln!(report, "=== PROJECT RISK HEATMAP ===").unwrap();
        writeln!(report, "Root: {}", root.display()).unwrap();
        writeln!(report, "Modified Files: {}", modified_files.len()).unwrap();
        writeln!(report, "Files with Findings: {}", file_findings.len()).unwrap();
        writeln!(report, "\n--- DIRECTORY RISK HEATMAP ---").unwrap();

        if let Err(e) = Self::walk(root, root, 0, &mut report, &modified_set, &file_findings) {
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
        file_findings: &HashMap<String, Vec<FindingSeverity>>,
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
                Self::walk(root, &path, depth + 1, report, modified_set, file_findings)?;
            } else {
                let mut relative_path_str = String::new();
                if let Ok(rel) = path.strip_prefix(root) {
                    relative_path_str = rel.to_string_lossy().replace('\\', "/");
                }

                let mut badges = Vec::new();

                if modified_set.contains(&relative_path_str) {
                    badges.push("🔥 (Modified)");
                }

                if let Some(severities) = file_findings.get(&relative_path_str) {
                    let has_error = severities.contains(&FindingSeverity::Error);
                    let has_warning = severities.contains(&FindingSeverity::Warning);
                    let has_info = severities.contains(&FindingSeverity::Info);

                    if has_error {
                        badges.push("❌ (Error)");
                    } else if has_warning {
                        badges.push("⚠️ (Warning)");
                    } else if has_info {
                        badges.push("ℹ️ (Info)");
                    }
                }

                if badges.is_empty() {
                    writeln!(report, "{prefix}📄 {name}").unwrap();
                } else {
                    writeln!(report, "{prefix}📄 {name} {}", badges.join(" ")).unwrap();
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
    fn test_risk_heatmap_generation() {
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
        File::create(temp_dir.join("Cargo.toml")).unwrap();

        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).unwrap();
        File::create(src_dir.join("lib.rs")).unwrap();
        File::create(src_dir.join("utils.rs")).unwrap();
        File::create(src_dir.join("config.rs")).unwrap();

        let modified_files = vec![
            "src/lib.rs".to_string(),
            "README.md".to_string(),
            "Cargo.toml".to_string(),
        ];

        let findings = vec![
            ReviewFinding {
                file_path: "src/lib.rs".to_string(),
                line_number: Some(10),
                severity: FindingSeverity::Error,
                message: "Critical error".to_string(),
            },
            ReviewFinding {
                file_path: "src/lib.rs".to_string(),
                line_number: Some(12),
                severity: FindingSeverity::Warning,
                message: "A warning".to_string(),
            },
            ReviewFinding {
                file_path: "Cargo.toml".to_string(),
                line_number: None,
                severity: FindingSeverity::Warning,
                message: "Missing deps".to_string(),
            },
            ReviewFinding {
                file_path: "README.md".to_string(),
                line_number: None,
                severity: FindingSeverity::Info,
                message: "Typo".to_string(),
            },
            ReviewFinding {
                file_path: "src/config.rs".to_string(), // not modified but has finding
                line_number: Some(1),
                severity: FindingSeverity::Warning,
                message: "Deprecated".to_string(),
            },
        ];

        let heatmap = ProjectRiskHeatmap::new();
        let report = heatmap
            .generate(&temp_dir, &modified_files, &findings)
            .unwrap();

        // Clean up
        fs::remove_dir_all(&temp_dir).unwrap();

        println!("{report}");

        assert!(report.contains("=== PROJECT RISK HEATMAP ==="));
        assert!(report.contains("📄 README.md 🔥 (Modified) ℹ️ (Info)"));
        assert!(report.contains("📄 main.rs"));
        assert!(!report.contains("📄 main.rs 🔥"));
        assert!(report.contains("📄 Cargo.toml 🔥 (Modified) ⚠️ (Warning)"));
        assert!(report.contains("📂 src"));
        // Error overrides Warning in badge
        assert!(report.contains("📄 lib.rs 🔥 (Modified) ❌ (Error)"));
        assert!(report.contains("📄 utils.rs"));
        assert!(report.contains("📄 config.rs ⚠️ (Warning)"));
    }
}
