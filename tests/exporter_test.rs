use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_session_to_markdown() {
    let server = MockServer::start().await;

    // Mock session details
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "id": "s-1",
            "title": "Refactor Login",
            "state": "COMPLETED",
            "prompt": "Refactor the login module",
            "createTime": "2023-10-27T10:00:00Z"
        })))
        .mount(&server)
        .await;

    // Mock activities (page 1)
    // The API typically returns newest first.
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-2",
                    "activityType": "TOOL_USE",
                    "timestamp": "2023-10-27T10:05:00Z",
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "{\"path\": \"src/auth.rs\"}"
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-1",
                    "activityType": "THOUGHT",
                    "timestamp": "2023-10-27T10:01:00Z",
                    "overview": {
                        "thoughts": "I need to check the auth module."
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let exporter = SessionExporter::new(client);
    let markdown = exporter
        .export_to_markdown("sessions/s-1")
        .await
        .expect("export session");

    // Verify content
    assert!(markdown.contains("# Session Report: Refactor Login"));
    assert!(markdown.contains("**Session ID:** `sessions/s-1`"));
    assert!(markdown.contains("Refactor the login module"));

    // Check order: a-1 (thought) should come before a-2 (tool use) because we reverse the list
    let thought_idx = markdown
        .find("I need to check the auth module")
        .expect("thought found");
    let tool_idx = markdown.find("read_file").expect("tool found");

    assert!(thought_idx < tool_idx, "Activities should be chronological");

    // Check formatting
    assert!(markdown.contains("```json"));
    // The pretty printer might add spaces/newlines, so we just check key parts
    assert!(markdown.contains("\"path\": \"src/auth.rs\""));
}
