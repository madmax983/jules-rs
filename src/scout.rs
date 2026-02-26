use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Scans a project directory to generate a "Mission Briefing".
#[derive(Debug, Default)]
pub struct ProjectScout;

impl ProjectScout {
    /// Creates a new `ProjectScout`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Scans the given path and returns a `MissionBriefing`.
    ///
    /// # Errors
    /// Returns an IO error if the directory cannot be read.
    pub fn scout(&self, root: &Path) -> Result<MissionBriefing, std::io::Error> {
        let mut briefing = MissionBriefing {
            root_path: root.to_path_buf(),
            file_count: 0,
            dir_count: 0,
            extensions: HashMap::new(),
            top_level_files: Vec::new(),
        };

        // We scan recursively but limit depth to avoid infinite loops or massive scans
        self.scan_recursive(root, &mut briefing, 0)?;

        // Sort top level files for deterministic output
        briefing.top_level_files.sort();

        Ok(briefing)
    }

    #[allow(clippy::self_only_used_in_recursion)]
    fn scan_recursive(
        &self,
        dir: &Path,
        briefing: &mut MissionBriefing,
        depth: usize,
    ) -> Result<(), std::io::Error> {
        // Safety brake for deep recursion
        if depth > 10 {
            return Ok(());
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(());
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            // Get file name
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            // Skip hidden files/dirs (starting with .) except specific config files
            if name.starts_with('.') && name != ".gitignore" && name != ".env" && name != ".github"
            {
                continue;
            }

            // Skip common build/dependency directories
            if name == "target"
                || name == "node_modules"
                || name == "dist"
                || name == "build"
                || name == "vendor"
            {
                continue;
            }

            if metadata.is_dir() {
                briefing.dir_count += 1;
                // Recurse
                self.scan_recursive(&path, briefing, depth + 1)?;
            } else {
                briefing.file_count += 1;

                // Track extension
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    *briefing.extensions.entry(ext.to_string()).or_insert(0) += 1;
                }

                // Track top-level files
                if depth == 0 {
                    briefing.top_level_files.push(name.to_string());
                }
            }
        }
        Ok(())
    }
}

/// A summary of the project structure.
#[derive(Debug, Default)]
pub struct MissionBriefing {
    /// The root path scanned.
    pub root_path: PathBuf,
    /// Total number of files found (respecting ignore rules).
    pub file_count: usize,
    /// Total number of directories found (respecting ignore rules).
    pub dir_count: usize,
    /// Count of files by extension.
    pub extensions: HashMap<String, usize>,
    /// List of files in the root directory.
    pub top_level_files: Vec<String>,
}

impl fmt::Display for MissionBriefing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "🦅  MISSION BRIEFING")?;
        writeln!(f, "====================")?;
        writeln!(f, "Target: {}", self.root_path.display())?;
        writeln!(
            f,
            "Intel: {} files, {} directories found",
            self.file_count, self.dir_count
        )?;
        writeln!(f)?;

        if !self.top_level_files.is_empty() {
            writeln!(f, "📂 TOP LEVEL FILES")?;
            for file in &self.top_level_files {
                writeln!(f, "  - {file}")?;
            }
            writeln!(f)?;
        }

        if self.extensions.is_empty() {
            writeln!(f, "🔧 DETECTED TECH: None (no file extensions found)")?;
        } else {
            writeln!(f, "🔧 DETECTED TECH")?;
            // Sort by count descending
            let mut sorted_exts: Vec<_> = self.extensions.iter().collect();
            sorted_exts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

            for (ext, count) in sorted_exts.iter().take(8) {
                writeln!(f, "  - .{ext:<4} : {count} files")?;
            }
            if sorted_exts.len() > 8 {
                writeln!(f, "  ... and {} more types", sorted_exts.len() - 8)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn create_temp_dir() -> PathBuf {
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut path = std::env::temp_dir();
        path.push(format!("jules_scout_test_{timestamp}_{count}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_scout_basic_structure() {
        let root = create_temp_dir();

        // Setup structure
        // /root
        //   Cargo.toml
        //   README.md
        //   src/
        //     main.rs
        //     lib.rs
        //   tests/
        //     integration.rs

        File::create(root.join("Cargo.toml")).unwrap();
        File::create(root.join("README.md")).unwrap();

        fs::create_dir(root.join("src")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();
        File::create(root.join("src/lib.rs")).unwrap();

        fs::create_dir(root.join("tests")).unwrap();
        File::create(root.join("tests/integration.rs")).unwrap();

        let scout = ProjectScout::new();
        let briefing = scout.scout(&root).unwrap();

        assert_eq!(briefing.file_count, 5); // Cargo.toml, README.md, main.rs, lib.rs, integration.rs
        assert_eq!(briefing.dir_count, 2); // src, tests
        assert!(briefing.top_level_files.contains(&"Cargo.toml".to_string()));
        assert!(briefing.top_level_files.contains(&"README.md".to_string()));
        assert_eq!(*briefing.extensions.get("rs").unwrap(), 3);
        assert_eq!(*briefing.extensions.get("toml").unwrap(), 1);
        assert_eq!(*briefing.extensions.get("md").unwrap(), 1);

        // Clean up
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_scout_ignores() {
        let root = create_temp_dir();

        // Setup structure
        // /root
        //   .git/ (should ignore)
        //     config
        //   target/ (should ignore)
        //     debug/
        //       app
        //   visible.txt

        fs::create_dir(root.join(".git")).unwrap();
        File::create(root.join(".git/config")).unwrap();

        fs::create_dir_all(root.join("target/debug")).unwrap();
        File::create(root.join("target/debug/app")).unwrap();

        File::create(root.join("visible.txt")).unwrap();

        let scout = ProjectScout::new();
        let briefing = scout.scout(&root).unwrap();

        assert_eq!(briefing.file_count, 1); // Only visible.txt
        assert_eq!(briefing.top_level_files, vec!["visible.txt".to_string()]);

        // Clean up
        let _ = fs::remove_dir_all(root);
    }
}
