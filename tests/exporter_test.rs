use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_markdown_generates_report_with_paginated_activities() {
    let server = MockServer::start().await;

    // Mock Get Session
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "title": "Fix Bug",
            "prompt": "Fix the null pointer exception",
            "state": "COMPLETED",
            "createTime": "2023-10-27T10:00:00Z",
            "output": {
                "changedFiles": [
                    { "path": "src/main.rs", "diff": "@@ -1 +1 @@\n-old\n+new" }
                ]
            }
        })))
        .mount(&server)
        .await;

    // Mock List Activities (Page 1)
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param_is_missing("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "activityType": "Thought",
                    "status": "SUCCESS",
                    "overview": {
                        "thoughts": "I need to check the code."
                    }
                }
            ],
            "nextPageToken": "token-2"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Mock List Activities (Page 2)
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param("pageToken", "token-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-2",
                    "activityType": "ToolUse",
                    "status": "SUCCESS",
                    "toolUse": {
                        "toolName": "readFile",
                        "input": "{ \"path\": \"src/main.rs\" }"
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .unwrap();

    let exporter = SessionExporter::new(&client);
    let report = exporter
        .export_markdown("s-1")
        .await
        .expect("export failed");

    // Check Header and Metadata
    assert!(report.contains("# Session Report: Fix Bug"));
    assert!(report.contains("**ID:** `sessions/s-1`"));
    assert!(report.contains("**Status:** `Completed`"));
    assert!(report.contains("**Created:** 2023-10-27T10:00:00Z"));

    // Check Prompt
    assert!(report.contains("> Fix the null pointer exception"));

    // Check Activities
    assert!(report.contains("I need to check the code."));
    assert!(report.contains("**Tool Use:** `readFile`"));
    assert!(report.contains("{ \"path\": \"src/main.rs\" }"));

    // Check Final Output
    assert!(report.contains("## Final Output"));
    assert!(report.contains("**Changed Files:**"));
    assert!(report.contains("- `src/main.rs`"));
}
