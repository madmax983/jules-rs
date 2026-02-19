use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;

fn director_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_director"))
}

fn unique_state_path() -> PathBuf {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut path = env::temp_dir();
    path.push(format!("director-test-{}.json", now));
    path
}

#[test]
fn test_usage_error_exit_code() {
    let output = Command::new(director_bin())
        .arg("invalid-command")
        .output()
        .expect("failed to execute process");

    // Expecting 10 for usage error
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(10), "Exit code should be 10 (Usage)");
}

#[test]
fn test_init_success() {
    let state_path = unique_state_path();
    let output = Command::new(director_bin())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("failed to execute process");

    assert!(output.status.success(), "init failed: {:?}", output);
    assert!(state_path.exists());
    let _ = fs::remove_file(state_path);
}

#[test]
fn test_locking() {
    let state_path = unique_state_path();

    // Create state file first
    let _ = Command::new(director_bin())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .status()
        .expect("init failed");

    // Create lock file explicitly
    let lock_path = PathBuf::from(format!("{}.lock", state_path.display()));
    fs::File::create(&lock_path).expect("create lock file");

    // Spawn `flock` to hold the lock for 5 seconds
    // This simulates another process holding the lock
    let mut child = Command::new("flock")
        .arg(&lock_path)
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .expect("failed to spawn flock");

    // Give it time to start and acquire lock
    thread::sleep(Duration::from_secs(1));

    // Check if flock is still running
    if let Ok(Some(status)) = child.try_wait() {
        panic!("Background flock process exited early with status: {:?}", status);
    }

    // Now try to run director status
    // It should fail to acquire lock immediately
    let output = Command::new(director_bin())
        .arg("status")
        .arg("--state")
        .arg(&state_path)
        .output()
        .expect("failed to run director");

    if !output.status.success() {
         let stderr = String::from_utf8_lossy(&output.stderr);
         println!("Director stderr: {}", stderr);
    }

    assert!(!output.status.success(), "Director should fail due to lock");
    assert_eq!(output.status.code(), Some(13), "Exit code should be 13 (Lock)");

    let _ = child.kill();
    let _ = fs::remove_file(&state_path);
    // Cleanup lock file if exists
    let lock_path = PathBuf::from(format!("{}.lock", state_path.display()));
    if lock_path.exists() {
        let _ = fs::remove_file(lock_path);
    }
}

#[test]
fn test_json_output() {
    let state_path = unique_state_path();
    let _ = Command::new(director_bin())
        .arg("init")
        .arg("goal")
        .arg("source")
        .arg("--state")
        .arg(&state_path)
        .status();

    let output = Command::new(director_bin())
        .arg("status")
        .arg("--state")
        .arg(&state_path)
        .arg("--json")
        .output()
        .expect("failed to execute process");

    // Once implemented, check JSON.
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert_eq!(v["schema_version"], 1);

    let _ = fs::remove_file(state_path);
}
