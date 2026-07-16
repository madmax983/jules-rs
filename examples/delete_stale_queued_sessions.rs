use std::env;
use std::time::Duration;

use jules_rs::{JulesClient, JulesError};

const DEFAULT_MAX_AGE_HOURS: u64 = 24;

#[derive(Debug, thiserror::Error)]
enum ExampleError {
    #[error("missing environment variable `JULES_API_KEY`")]
    MissingApiKey,
    #[error("invalid `JULES_STALE_MAX_AGE_HOURS`: {0}")]
    InvalidMaxAge(String),
    #[error(transparent)]
    Jules(#[from] JulesError),
}

#[tokio::main]
async fn main() -> Result<(), ExampleError> {
    let api_key = env::var("JULES_API_KEY").map_err(|_| ExampleError::MissingApiKey)?;
    let max_age_hours = match env::var("JULES_STALE_MAX_AGE_HOURS") {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| ExampleError::InvalidMaxAge(value))?,
        Err(_) => DEFAULT_MAX_AGE_HOURS,
    };
    let max_age = Duration::from_secs(max_age_hours * 60 * 60);

    let client = JulesClient::builder(api_key).build()?;

    let deleted_sessions = client.delete_stale_queued_sessions(max_age).await?;
    if deleted_sessions.is_empty() {
        println!("No stale queued sessions older than {max_age_hours}h found.");
    } else {
        println!(
            "Deleted {} stale queued session(s) older than {max_age_hours}h:",
            deleted_sessions.len()
        );
        for session_name in deleted_sessions {
            println!("- {session_name}");
        }
    }

    Ok(())
}
