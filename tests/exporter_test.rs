use jules_rs::{JulesClient, SessionExporter};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn export_markdown_generates_report() {
    let server = MockServer::start().await;

    // Mock session retrieval
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "title": "Test Session",
            "state": "COMPLETED",
            "prompt": "Fix the bug",
            "createTime": "2023-10-27T10:00:00Z"
        })))
        .mount(&server)
        .await;

    // Mock activities retrieval
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "sessions/s-1/activities/a-1",
                    "status": "SUCCESS",
                    "timestamp": "2023-10-27T10:01:00Z",
                    "stageName": "Planning",
                    "overview": {
                        "summary": "Analyzing the issue",
                        "thoughts": "I need to check the logs."
                    }
                },
                {
                    "name": "sessions/s-1/activities/a-2",
                    "status": "SUCCESS",
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "{\"path\": \"src/main.rs\"}"
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
    let report = exporter
        .export_markdown("sessions/s-1")
        .await
        .expect("export markdown");

    println!("{report}");

    assert!(report.contains("# Session Report: Test Session"));
    assert!(report.contains("**ID:** `sessions/s-1`"));
    assert!(report.contains("**State:** `Completed`"));
    assert!(report.contains("## Prompt"));
    assert!(report.contains("Fix the bug"));

    assert!(report.contains("### Activity: sessions/s-1/activities/a-1"));
    assert!(report.contains("**Stage:** Planning"));
    assert!(report.contains("**Summary:** Analyzing the issue"));
    assert!(report.contains("**Thoughts:**\n> I need to check the logs."));

    assert!(report.contains("### Activity: sessions/s-1/activities/a-2"));
    assert!(report.contains("**Tool Call:** `read_file`"));
    assert!(report.contains("src/main.rs"));
}
