#![cfg(feature = "reviewer")]

use jules_rs::{ChangedFile, FindingSeverity, SessionOutput, SessionReviewer};

#[test]
fn test_detects_unwrap_and_counts_lines() {
    let diff = "\
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,4 @@
 fn main() {
-    println!(\"Hello World\");
+    let a = Some(1);
+    println!(\"{}\", a.unwrap());
 }
";

    let changed_file = ChangedFile {
        path: "src/main.rs".to_string(),
        diff: diff.to_string(),
    };

    let session_output = SessionOutput {
        changed_files: vec![changed_file],
        commit_hash: None,
        pull_request: None,
    };

    let outputs = vec![session_output];
    let reviewer = SessionReviewer::new();
    let report = reviewer.review_session(&outputs);

    assert_eq!(report.lines_added, 2);
    assert_eq!(report.lines_removed, 1);

    let unwrap_finding = report
        .findings
        .iter()
        .find(|f| f.message.contains("unwrap()"));
    assert!(
        unwrap_finding.is_some(),
        "Expected finding for unwrap() usage"
    );

    let finding = unwrap_finding.unwrap();
    assert_eq!(finding.severity, FindingSeverity::Warning);
    assert_eq!(finding.file_path, "src/main.rs");
}

#[test]
fn test_detects_todos() {
    let diff = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,3 @@
 fn main() {
-    // old code
+    // TODO: fix this later
 }
";

    let changed_file = ChangedFile {
        path: "src/lib.rs".to_string(),
        diff: diff.to_string(),
    };

    let session_output = SessionOutput {
        changed_files: vec![changed_file],
        commit_hash: None,
        pull_request: None,
    };

    let reviewer = SessionReviewer::new();
    let report = reviewer.review_session(&[session_output]);

    let todo_finding = report.findings.iter().find(|f| f.message.contains("TODO"));
    assert!(todo_finding.is_some(), "Expected finding for TODO comment");

    let finding = todo_finding.unwrap();
    assert_eq!(finding.severity, FindingSeverity::Info);
    assert_eq!(finding.file_path, "src/lib.rs");
}

#[test]
fn test_flags_missing_tests() {
    let diff = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,3 @@
 fn main() {
-    // old code
+    // new code
 }
";

    let changed_file = ChangedFile {
        path: "src/lib.rs".to_string(),
        diff: diff.to_string(),
    };

    let session_output = SessionOutput {
        changed_files: vec![changed_file],
        commit_hash: None,
        pull_request: None,
    };

    let reviewer = SessionReviewer::new();
    let report = reviewer.review_session(&[session_output]);

    let test_finding = report
        .findings
        .iter()
        .find(|f| f.message.contains("test file"));
    assert!(
        test_finding.is_some(),
        "Expected finding about missing tests"
    );

    let finding = test_finding.unwrap();
    assert_eq!(finding.severity, FindingSeverity::Warning);
}
