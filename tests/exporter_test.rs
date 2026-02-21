use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_session_to_markdown_basic() {
    let server = MockServer::start().await;

    // Mock get_session
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "title": "Fix bug",
            "state": "COMPLETED",
            "prompt": "Fix the null pointer exception",
            "plan": {
                "steps": [
                    { "description": "Identify bug", "status": "COMPLETED" },
                    { "description": "Fix it", "status": "PENDING" }
                ]
            }
        })))
        .mount(&server)
        .await;

    // Mock list_activities
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "status": "SUCCESS",
                    "overview": {
                        "thoughts": "I found the bug in `main.rs`."
                    },
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "{ \"path\": \"main.rs\" }"
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-2",
                    "status": "SUCCESS",
                    "viewDiff": {
                        "diff": "--- a/main.rs\n+++ b/main.rs\n- let x = null;\n+ let x = 0;"
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build client");

    let exporter = SessionExporter::new(&client);
    let markdown = exporter
        .export_to_markdown("s-1")
        .await
        .expect("export failed");

    // Verify Header
    println!("{markdown}");
    assert!(markdown.contains("# Fix bug"));
    assert!(markdown.contains("**ID:** `sessions/s-1`"));
    assert!(markdown.contains("**Status:** Completed"));

    // Verify Prompt
    assert!(markdown.contains("## Prompt"));
    assert!(markdown.contains("Fix the null pointer exception"));

    // Verify Plan
    assert!(markdown.contains("## Plan"));
    assert!(markdown.contains("- [x] Identify bug (COMPLETED)"));
    assert!(markdown.contains("- [ ] Fix it (PENDING)"));

    // Verify Activities
    assert!(markdown.contains("## Activity Log"));
    assert!(markdown.contains("### sessions/s-1/activities/a-1"));
    assert!(markdown.contains("**Thoughts:**\n> I found the bug in `main.rs`."));
    assert!(markdown.contains("**Tool Use:** `read_file`"));
    assert!(markdown.contains("```json\n{ \"path\": \"main.rs\" }\n```"));

    assert!(markdown.contains("### sessions/s-1/activities/a-2"));
    assert!(markdown.contains("**Diff View:**"));
    assert!(markdown.contains("```diff\n--- a/main.rs"));
}

#[tokio::test]
async fn export_session_pagination() {
    let server = MockServer::start().await;

    // Mock get_session
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-2",
            "state": "IN_PROGRESS"
        })))
        .mount(&server)
        .await;

    // Page 1
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-2/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param_is_missing("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                { "name": "activity-1" }
            ],
            "nextPageToken": "page-2"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Page 2
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-2/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param("pageToken", "page-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                { "name": "activity-2" }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build client");

    let exporter = SessionExporter::new(&client);
    let markdown = exporter
        .export_to_markdown("s-2")
        .await
        .expect("export failed");

    assert!(markdown.contains("### activity-1"));
    assert!(markdown.contains("### activity-2"));
}
