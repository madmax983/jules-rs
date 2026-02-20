use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
    path.push(format!("director-integration-{}-{}.json", pid, timestamp));
    path
}

#[tokio::test]
async fn api_url_override_works() {
    // Start mock server
    let mock_server = MockServer::start().await;

    // Mock create session response
    // The director will try to create a session for the first pending task "t-1".
    Mock::given(method("POST"))
        .and(path("/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "sessions/mock-session",
            "state": "PLANNING",
            "createTime": "2024-01-01T00:00:00Z",
            "updateTime": "2024-01-01T00:00:00Z"
        })))
        .expect(1) // Expect exactly one call
        .mount(&mock_server)
        .await;

    let state_path = temp_state_path();

    // 1. Init
    let status = Command::new(director_exe())
        .arg("init")
        .arg("test goal")
        .arg("test source")
        .arg("--state")
        .arg(&state_path)
        .status()
        .expect("failed to run init");
    assert!(status.success(), "init command failed");

    // 2. Tick with override
    let output = Command::new(director_exe())
        .arg("tick")
        .arg("--state")
        .arg(&state_path)
        .env("JULES_API_KEY", "dummy-key")
        .env("JULES_API_URL", mock_server.uri())
        .output()
        .expect("failed to run tick");

    // Print stdout/stderr if it fails
    if !output.status.success() {
        println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(output.status.success(), "tick command failed");

    // Verify stdout contains summary
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("created session `sessions/mock-session` for task `t-1`"));

    // Cleanup
    fs::remove_file(&state_path).ok();
    fs::remove_file(format!("{}.lock", state_path.display())).ok();
}
