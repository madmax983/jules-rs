use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_markdown_generates_report_with_activities() {
    let server = MockServer::start().await;

    // Mock GET session
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "title": "Fix bug in login",
            "state": "COMPLETED",
            "prompt": "The login button is broken."
        })))
        .mount(&server)
        .await;

    // Mock GET activities (page 1)
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param_is_missing("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "activityType": "USER_INPUT",
                    "timestamp": "2023-10-27T10:00:00Z",
                    "userInputRequest": {
                        "question": "Which file?",
                        "suggestedResponses": []
                    }
                }
            ],
            "nextPageToken": "cursor-2"
        })))
        .mount(&server)
        .await;

    // Mock GET activities (page 2)
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .and(query_param("pageSize", "100"))
        .and(query_param("pageToken", "cursor-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-2",
                    "activityType": "TOOL_USE",
                    "timestamp": "2023-10-27T10:05:00Z",
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "{\"path\": \"src/login.rs\"}"
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
    let report = exporter.export_markdown("s-1").await.expect("export markdown");

    println!("{report}");

    assert!(report.contains("# Session Report: Fix bug in login"));
    assert!(report.contains("**ID:** `sessions/s-1`"));
    assert!(report.contains("**State:** `Completed`"));
    assert!(report.contains("The login button is broken."));

    // Check for Activity 1
    assert!(report.contains("2023-10-27T10:00:00Z - USER_INPUT"));
    assert!(report.contains("**Question:** Which file?"));

    // Check for Activity 2
    assert!(report.contains("2023-10-27T10:05:00Z - TOOL_USE"));
    assert!(report.contains("**Tool:** `read_file`"));
    assert!(report.contains("\"path\": \"src/login.rs\""));
}
