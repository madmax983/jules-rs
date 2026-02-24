#![cfg(feature = "player")]

use jules_rs::{Activity, SessionPlayer};

fn mock_activity(name: &str, timestamp: Option<&str>) -> Activity {
    Activity {
        name: name.to_string(),
        timestamp: timestamp.map(ToString::to_string),
        status: None,
        stage_name: None,
        activity_type: None,
        detail: None,
        overview: None,
        plan: None,
        user_input_request: None,
        tool_use: None,
        view_diff: None,
        commit: None,
        create_pull_request: None,
        output: None,
    }
}

#[test]
fn test_sort_activities() {
    let player = SessionPlayer::new(1.0);

    let a1 = mock_activity("a1", Some("2023-01-01T10:00:00Z"));
    let a2 = mock_activity("a2", Some("2023-01-01T12:00:00Z"));
    let a3 = mock_activity("a3", Some("2023-01-01T09:00:00Z"));
    let a4 = mock_activity("a4", None); // No timestamp, should be first or last?
    // String comparison: "" < "2023..." so None (treated as "") comes first.

    let activities = vec![a1.clone(), a2.clone(), a3.clone(), a4.clone()];
    let sorted = player.sort_activities(&activities);

    assert_eq!(sorted[0].name, "a4");
    assert_eq!(sorted[1].name, "a3");
    assert_eq!(sorted[2].name, "a1");
    assert_eq!(sorted[3].name, "a2");
}
