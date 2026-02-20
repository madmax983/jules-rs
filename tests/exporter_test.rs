use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_session_to_markdown_formats_correctly() {
    let server = MockServer::start().await;

    // Mock session details
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "id": "s-1",
            "title": "Refactor Login",
            "state": "COMPLETED",
            "prompt": "Refactor the login module to use OAuth2.",
            "createTime": "2023-10-27T10:00:00Z",
            "updateTime": "2023-10-27T10:05:00Z"
        })))
        .mount(&server)
        .await;

    // Mock activities list (page 1)
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .and(query_param("pageSize", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:01:00Z",
                    "overview": {
                        "thoughts": "I need to check existing auth implementation.",
                        "summary": "Checking auth."
                    },
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "src/auth.rs"
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-2",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:02:00Z",
                    "viewDiff": {
                        "diff": "--- a/src/auth.rs\n+++ b/src/auth.rs\n@@ -1,1 +1,2 @@\n-fn login() {}\n+fn login_oauth() {}"
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

    let exporter = SessionExporter::new(&client);
    let markdown = exporter
        .export_to_markdown("s-1")
        .await
        .expect("export markdown");

    // Assertions
    assert!(markdown.contains("# Session: Refactor Login"));
    assert!(markdown.contains("**ID**: s-1"));
    assert!(markdown.contains("**Status**: Completed"));
    assert!(markdown.contains("**Prompt**: Refactor the login module to use OAuth2."));

    // Check Activity 1
    assert!(markdown.contains("### Activity: a-1"));
    assert!(markdown.contains("**Thoughts**: I need to check existing auth implementation."));
    assert!(markdown.contains("**Tool Use**: `read_file`"));
    assert!(markdown.contains("src/auth.rs"));

    // Check Activity 2
    assert!(markdown.contains("### Activity: a-2"));
    assert!(markdown.contains("```diff"));
    assert!(markdown.contains("-fn login() {}"));
    assert!(markdown.contains("+fn login_oauth() {}"));
}
