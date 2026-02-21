use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_session_to_markdown_formats_activities_correctly() {
    let server = MockServer::start().await;

    // Mock session details
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "id": "s-1",
            "prompt": "Create a hello world tool",
            "state": "COMPLETED",
            "createTime": "2023-10-27T10:00:00Z",
            "updateTime": "2023-10-27T10:05:00Z"
        })))
        .mount(&server)
        .await;

    // Mock activities list
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:00:01Z",
                    "overview": {
                        "state": "PLANNING",
                        "thoughts": "I should start by checking the files.",
                        "summary": "Checking files"
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-2",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:00:02Z",
                    "toolUse": {
                        "toolName": "list_files",
                        "input": "{ \"path\": \".\" }"
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-3",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:00:03Z",
                    "output": {
                        "changedFiles": [
                            {
                                "path": "hello.rs",
                                "diff": "fn main() { println!(\"Hello\"); }"
                            }
                        ]
                    }
                }
            ],
            "nextPageToken": ""
        })))
        .mount(&server)
        .await;

    let client = JulesClient::builder("test-api-key")
        .base_url(format!("{}/v1alpha", server.uri()))
        .build()
        .expect("build test client");

    let exporter = SessionExporter::new(client, "s-1");
    let markdown = exporter
        .export_to_markdown()
        .await
        .expect("export to markdown");

    // Verify key elements in the markdown
    assert!(markdown.contains("# Session Report: s-1"));
    assert!(markdown.contains("**Status:** Completed"));
    assert!(markdown.contains("## Activity 1"));
    assert!(markdown.contains("**Thoughts:**\nI should start by checking the files."));
    assert!(markdown.contains("**Tool:** `list_files`"));
    assert!(markdown.contains("```json\n{ \"path\": \".\" }\n```"));
    assert!(markdown.contains("## Changed Files"));
    assert!(markdown.contains("### hello.rs"));
    assert!(markdown.contains("fn main() { println!(\"Hello\"); }"));
}
