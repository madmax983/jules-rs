#![cfg(feature = "forensics")]

use jules_rs::SessionForensics;
use jules_rs::{Activity, ActivityStatus, ChangedFile, FindingSeverity, SessionOutput, ToolUse};

fn create_mock_activity(name: &str, tool: Option<(&str, &str)>, diff: Option<&str>) -> Activity {
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
        output: diff.map(|d| SessionOutput {
            changed_files: vec![ChangedFile {
                path: "src/main.rs".to_string(),
                diff: d.to_string(),
            }],
            commit_hash: None,
            pull_request: None,
        }),
    }
}

#[test]
fn test_forensics_traces_findings() {
    let diff_clean = "\
@@ -1,2 +1,2 @@
 fn main() {
-    println!(\"Hello\");
+    println!(\"World\");
 }";

    let diff_dirty = "\
@@ -10,3 +10,4 @@
 fn do_something() {
+    // TODO: implement this properly
+    let x = Some(1).unwrap();
 }";

    let activities = vec![
        create_mock_activity("1", Some(("read_file", "foo")), None),
        create_mock_activity("2", Some(("write_file", "clean")), Some(diff_clean)),
        create_mock_activity(
            "3",
            Some(("replace_with_git_merge_diff", "dirty")),
            Some(diff_dirty),
        ),
    ];

    let forensics = SessionForensics::new();
    let report = forensics.analyze(&activities);

    assert_eq!(report.total_findings, 2);
    assert_eq!(report.activity_findings.len(), 1);

    let activity_finding = &report.activity_findings[0];
    assert_eq!(activity_finding.activity_name, "3");
    assert_eq!(
        activity_finding.tool_name.as_deref(),
        Some("replace_with_git_merge_diff")
    );

    assert_eq!(activity_finding.findings.len(), 2);

    let unwrap_finding = activity_finding
        .findings
        .iter()
        .find(|f| f.message.contains("unwrap()"))
        .unwrap();
    assert_eq!(unwrap_finding.severity, FindingSeverity::Warning);

    let todo_finding = activity_finding
        .findings
        .iter()
        .find(|f| f.message.contains("TODO"))
        .unwrap();
    assert_eq!(todo_finding.severity, FindingSeverity::Info);
}
