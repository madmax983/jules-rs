use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use fs2::FileExt;
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

fn temp_state_path() -> PathBuf {
    let mut path = env::temp_dir();
    let pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("director-test-{pid}-{timestamp}.json"));
    path
}

#[test]
fn usage_error_returns_exit_code_10() {
    let output = Command::new(director_exe())
        .arg("--unknown-flag")
        .output()
        .expect("failed to execute process");

    assert_eq!(output.status.code(), Some(10));
}

#[test]
fn usage_error_returns_json_when_requested() {
    let output = Command::new(director_exe())
        .arg("--unknown-flag")
        .arg("--json")
        .output()
        .expect("failed to execute process");

    assert_eq!(output.status.code(), Some(10));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("output should be JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["exit_code"], 10);
    assert!(json["error"].is_string());
}

#[test]
fn init_command_succeeds_and_outputs_json() {
    let state_path = temp_state_path();
    let output = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .arg("--json")
        .output()
        .expect("failed to execute process");

    assert_eq!(output.status.code(), Some(0));
    assert!(state_path.exists());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("output should be JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["exit_code"], 0);

    fs::remove_file(state_path).ok();
}

#[test]
fn lock_coordination_prevents_concurrent_runs() {
    let state_path = temp_state_path();
    let lock_path = PathBuf::from(format!("{}.lock", state_path.display()));

    let lock_file = fs::File::create(&lock_path).expect("failed to create lock file");
    lock_file.lock_exclusive().expect("failed to lock file");

    let output = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("failed to execute director");

    assert_eq!(
        output.status.code(),
        Some(13),
        "Director should fail with lock error (13) when lock is held"
    );

    lock_file.unlock().ok();
    fs::remove_file(&state_path).ok();
    fs::remove_file(&lock_path).ok();
}

#[tokio::test]
async fn tick_creates_session_for_pending_task() {
    let server = MockServer::start().await;
    let state_path = temp_state_path();

    Mock::given(method("POST"))
        .and(path("/v1alpha/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "url": "https://jules.app/sessions/s-1",
            "state": "QUEUED"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1alpha/sources/source"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sources/source",
            "githubRepo": {
                "owner": "test",
                "repo": "test",
                "defaultBranch": { "displayName": "main" }
            }
        })))
        .mount(&server)
        .await;

    let _ = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("init failed");

    let output = Command::new(director_exe())
        .arg("tick")
        .arg("--state")
        .arg(&state_path)
        .env("JULES_API_KEY", "dummy")
        .env("JULES_API_URL", format!("{}/v1alpha", server.uri()))
        .output()
        .expect("tick failed");

    if output.status.code() != Some(0) {
        eprintln!(
            "Tick failed with stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(output.status.code(), Some(0));

    let state_content = fs::read_to_string(&state_path).expect("read state");
    let state_json: serde_json::Value = serde_json::from_str(&state_content).expect("parse state");

    let task = state_json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t-1")
        .expect("find task t-1");
    assert_eq!(task["status"], "RUNNING");
    assert_eq!(task["session_name"], "sessions/s-1");

    fs::remove_file(state_path).ok();
}

#[tokio::test]
async fn tick_updates_running_task_to_completed() {
    let server = MockServer::start().await;
    let state_path = temp_state_path();

    let _ = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("init failed");

    let state_content = fs::read_to_string(&state_path).expect("read state");
    let mut state_json: serde_json::Value =
        serde_json::from_str(&state_content).expect("parse state");

    if let Some(task) = state_json["tasks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["id"] == "t-1")
    {
        task["status"] = json!("RUNNING");
        task["session_name"] = json!("sessions/s-1");
    }
    fs::write(&state_path, serde_json::to_string(&state_json).unwrap()).expect("write state");

    Mock::given(method("GET"))
        .and(path("/v1alpha/sessions/s-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sessions/s-1",
            "state": "COMPLETED",
            "output": {
                "pullRequest": {
                    "url": "https://github.com/owner/repo/pull/123",
                    "title": "PR Title"
                }
            }
        })))
        .mount(&server)
        .await;

    let output = Command::new(director_exe())
        .arg("tick")
        .arg("--state")
        .arg(&state_path)
        .env("JULES_API_KEY", "dummy")
        .env("JULES_API_URL", format!("{}/v1alpha", server.uri()))
        .output()
        .expect("tick failed");

    if output.status.code() != Some(0) {
        eprintln!(
            "Tick failed with stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(output.status.code(), Some(0));

    let state_content = fs::read_to_string(&state_path).expect("read state");
    let state_json: serde_json::Value = serde_json::from_str(&state_content).expect("parse state");

    let task = state_json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t-1")
        .expect("find task t-1");
    assert_eq!(task["status"], "PR_CANDIDATE");
    assert_eq!(task["pr_url"], "https://github.com/owner/repo/pull/123");

    fs::remove_file(state_path).ok();
}

#[tokio::test]
async fn tick_handles_api_errors_gracefully() {
    let server = MockServer::start().await;
    let state_path = temp_state_path();

    Mock::given(method("GET"))
        .and(path("/v1alpha/sources/source"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "sources/source",
            "githubRepo": {
                "owner": "test",
                "repo": "test",
                "defaultBranch": { "displayName": "main" }
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1alpha/sessions"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let _ = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("init failed");

    let output = Command::new(director_exe())
        .arg("tick")
        .arg("--state")
        .arg(&state_path)
        .env("JULES_API_KEY", "dummy")
        .env("JULES_API_URL", format!("{}/v1alpha", server.uri()))
        .output()
        .expect("tick failed");

    assert_eq!(output.status.code(), Some(11));

    fs::remove_file(state_path).ok();
}
