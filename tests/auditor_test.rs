#![cfg(feature = "auditor")]

use std::env;
use std::path::PathBuf;
use std::process::Command;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn director_exe() -> PathBuf {
    let mut path = env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("director");
    path
}

#[tokio::test]
async fn audit_command_fetches_and_generates_report() {
    let server = MockServer::start().await;

    // Mock session fetch
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "state": "COMPLETED",
            "plan": {
                "steps": [
                    { "description": "Step 1", "status": "completed" }
                ]
            }
        })))
        .mount(&server)
        .await;

    // Mock activities fetch
    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1/activities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "activities": [
                {
                    "name": "act-1",
                    "status": "SUCCESS",
                    "toolUse": {
                        "toolName": "read_file",
                        "input": "{}"
                    },
                    "output": {
                        "changedFiles": [
                            { "path": "src/main.rs", "diff": "" }
                        ]
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let output = Command::new(director_exe())
        .arg("audit")
        .arg("s-1")
        .env("JULES_API_KEY", "dummy")
        .env("JULES_API_URL", format!("{}/v1alpha", server.uri()))
        .output()
        .expect("failed to execute director");

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check if the report is correctly formatted containing both analytics and diagnostics
    assert!(stdout.contains("Session Analytics Report"));
    assert!(stdout.contains("Plan Progress"));
    assert!(stdout.contains("Session Diagnoses"));
}
