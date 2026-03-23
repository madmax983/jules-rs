#![cfg(feature = "forensics")]

use jules_rs::{Activity, ChangedFile, FindingSeverity, SessionForensics, SessionOutput};

#[test]
fn test_forensics_report_maps_findings_to_activity() {
    let mock_output = SessionOutput {
        changed_files: vec![ChangedFile {
            path: "src/main.rs".to_string(),
            diff: "@@ -1,3 +1,4 @@\n fn main() {\n+    let x = Some(1).unwrap();\n }\n".to_string(),
        }],
        commit_hash: None,
        pull_request: None,
    };

    let activity = Activity {
        name: "activity_123".to_string(),
        status: None,
        stage_name: None,
        activity_type: None,
        detail: None,
        timestamp: None,
        overview: None,
        plan: None,
        user_input_request: None,
        tool_use: None,
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: Some(mock_output),
    };

    let forensics = SessionForensics::new();
    let report = forensics.analyze_activities(&[activity]);

    assert_eq!(report.findings.len(), 1, "Expected exactly one finding");

    let forensics_finding = &report.findings[0];
    assert_eq!(forensics_finding.activity_name, "activity_123");

    let finding = &forensics_finding.finding;
    assert_eq!(finding.file_path, "src/main.rs");
    assert_eq!(finding.severity, FindingSeverity::Warning);
    assert!(finding.message.contains("unwrap()"));
}
