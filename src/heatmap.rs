use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::path::Path;

/// Generates a directory tree heatmap highlighting modified files.
#[derive(Default)]
pub struct ProjectHeatmap;

impl ProjectHeatmap {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Scans the project directory and returns a formatted heatmap report.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if directory traversal fails.
    pub fn generate(&self, root: &Path, modified_files: &[String]) -> std::io::Result<String> {
        let mut report = String::new();
        let modified_set: HashSet<String> = modified_files.iter().cloned().collect();

        // We use unwrap() here because writing to a String is infallible.
        writeln!(report, "=== PROJECT HEATMAP ===").unwrap();
        writeln!(report, "Root: {}", root.display()).unwrap();
        writeln!(report, "Modified Files: {}", modified_files.len()).unwrap();
        writeln!(report, "\n--- DIRECTORY HEATMAP ---").unwrap();

        if let Err(e) = Self::walk(root, root, 0, &mut report, &modified_set) {
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
                Self::walk(root, &path, depth + 1, report, modified_set)?;
            } else {
                let mut relative_path_str = String::new();
                if let Ok(rel) = path.strip_prefix(root) {
                    relative_path_str = rel.to_string_lossy().replace('\\', "/");
                }

                if modified_set.contains(&relative_path_str) {
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
    use std::fs::File;

    #[test]
    fn test_heatmap_generation() {
        // Create a temporary directory structure for testing
        let temp_dir =
            std::env::temp_dir().join(format!("jules_heatmap_test_{}", std::process::id()));
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

        let modified_files = vec!["src/lib.rs".to_string(), "README.md".to_string()];

        let heatmap = ProjectHeatmap::new();
        let report = heatmap.generate(&temp_dir, &modified_files).unwrap();

        // Clean up
        fs::remove_dir_all(&temp_dir).unwrap();

        println!("{report}");

        assert!(report.contains("=== PROJECT HEATMAP ==="));
        assert!(report.contains("📄 README.md 🔥 (Modified)"));
        assert!(report.contains("📄 main.rs"));
        assert!(!report.contains("📄 main.rs 🔥 (Modified)"));
        assert!(report.contains("📂 src"));
        assert!(report.contains("📄 lib.rs 🔥 (Modified)"));
        assert!(report.contains("📄 utils.rs"));
        assert!(!report.contains("📄 utils.rs 🔥 (Modified)"));
    }
}
