use std::env;
use std::process::ExitCode;
use std::time::Duration;

use jules_rs::{JulesClient, ListActivitiesParams, RetryPolicy, SessionPlayer, TimeoutPolicy};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: jules-player <session_id> [--speed <n>]");
        return ExitCode::FAILURE;
    }

    let session_id = &args[1];
    let mut speed = 1.0;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--speed" {
            if i + 1 < args.len() {
                speed = args[i + 1].parse().unwrap_or(1.0);
                i += 2;
            } else {
                eprintln!("Missing value for --speed");
                return ExitCode::FAILURE;
            }
        } else {
            i += 1;
        }
    }

    let Ok(api_key) = env::var("JULES_API_KEY") else {
        eprintln!("Error: JULES_API_KEY not set");
        return ExitCode::FAILURE;
    };

    let mut builder = JulesClient::builder(api_key)
        .retry_policy(RetryPolicy {
            max_retries: 3,
            initial_backoff: Duration::from_millis(300),
            max_backoff: Duration::from_secs(3),
        })
        .timeout_policy(TimeoutPolicy {
            request_timeout: Duration::from_secs(45),
        });

    if let Ok(api_url) = env::var("JULES_API_URL") {
        if !api_url.trim().is_empty() {
            builder = builder.base_url(api_url);
        }
    }

    let client = builder.build().expect("Failed to build client");

    let session = match client.get_session(session_id).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to fetch session: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut activities = Vec::new();
    let mut page_token = None;
    loop {
        let response = match client
            .list_activities(
                &session.name,
                ListActivitiesParams {
                    page_size: Some(100),
                    page_token: page_token.clone(),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to list activities: {e}");
                return ExitCode::FAILURE;
            }
        };

        activities.extend(response.activities);
        match response.next_page_token {
            Some(token) if !token.is_empty() => page_token = Some(token),
            _ => break,
        }
    }

    let player = SessionPlayer::new(speed);
    if let Err(e) = player.play(&session, &activities).await {
        eprintln!("Player error: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
