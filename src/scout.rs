use std::fmt::Write;
use std::fs;
use std::path::Path;

#[derive(Default)]
pub struct ProjectScout;

impl ProjectScout {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Scans the project directory and returns a formatted report.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if directory traversal fails.
    pub fn scan(&self, root: &Path) -> std::io::Result<String> {
        let mut report = String::new();
        // We use unwrap() here because writing to a String is infallible.
        writeln!(report, "=== PROJECT SCOUT REPORT ===").unwrap();
        writeln!(report, "Root: {}", root.display()).unwrap();

        writeln!(report, "\n--- DIRECTORY STRUCTURE ---").unwrap();
        if let Err(e) = Self::walk(root, 0, &mut report) {
            writeln!(report, "Error walking directory: {e}").unwrap();
        }

        writeln!(report, "\n--- KEY FILES SUMMARY ---").unwrap();
        Self::scan_key_files(root, &mut report);

        Ok(report)
    }

    fn walk(dir: &Path, depth: usize, report: &mut String) -> std::io::Result<()> {
        if depth > 4 {
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
                Self::walk(&path, depth + 1, report)?;
            } else {
                writeln!(report, "{prefix}📄 {name}").unwrap();
            }
        }
        Ok(())
    }

    fn scan_key_files(root: &Path, report: &mut String) {
        let key_files = [
            "README.md",
            "Cargo.toml",
            "package.json",
            "go.mod",
            "Makefile",
            "pyproject.toml",
            "requirements.txt",
        ];

        for file_name in &key_files {
            let path = root.join(file_name);
            if path.exists() {
                writeln!(report, "\nFound {file_name}:").unwrap();
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().take(15).collect();
                        for line in lines {
                            writeln!(report, "> {line}").unwrap();
                        }
                        if content.lines().count() > 15 {
                            writeln!(report, "... (truncated)").unwrap();
                        }
                    }
                    Err(_) => writeln!(report, "(Unable to read content)").unwrap(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_scout_scan() {
        // Create a temporary directory structure for testing
        let temp_dir =
            std::env::temp_dir().join(format!("jules_scout_test_{}", std::process::id()));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).unwrap();
        }
        fs::create_dir_all(&temp_dir).unwrap();

        // Create some files
        File::create(temp_dir.join("README.md"))
            .unwrap()
            .write_all(b"# Test Project\nThis is a test.")
            .unwrap();
        File::create(temp_dir.join("main.rs")).unwrap();

        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).unwrap();
        File::create(src_dir.join("lib.rs")).unwrap();

        // Create ignored directory
        fs::create_dir(temp_dir.join("target")).unwrap();
        File::create(temp_dir.join("target/artifact")).unwrap();

        let scout = ProjectScout::new();
        let report = scout.scan(&temp_dir).unwrap();

        // Clean up
        fs::remove_dir_all(&temp_dir).unwrap();

        println!("{report}");

        assert!(report.contains("=== PROJECT SCOUT REPORT ==="));
        assert!(report.contains("📄 README.md"));
        assert!(report.contains("📂 src"));
        assert!(report.contains("📄 lib.rs"));
        assert!(!report.contains("📂 target")); // Should be ignored
        assert!(report.contains("# Test Project")); // Content check
    }
}
