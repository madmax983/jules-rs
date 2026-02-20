use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use fs2::FileExt;

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
    // using a simple counter or random if possible, but pid + timestamp is usually enough
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

    // Create and lock the lock file from the test process
    let lock_file = fs::File::create(&lock_path).expect("failed to create lock file");
    lock_file.lock_exclusive().expect("failed to lock file");

    // Try to run director (any command that requires lock)
    let output = Command::new(director_exe())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("failed to execute director");

    // Verify it failed with exit code 13
    assert_eq!(
        output.status.code(),
        Some(13),
        "Director should fail with lock error (13) when lock is held"
    );

    // Unlock
    lock_file.unlock().ok();

    // Clean up
    fs::remove_file(&state_path).ok();
    fs::remove_file(&lock_path).ok();
}
