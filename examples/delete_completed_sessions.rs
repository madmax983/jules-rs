use std::env;

use jules_rs::{JulesClient, JulesError};

#[derive(Debug, thiserror::Error)]
enum ExampleError {
    #[error("missing environment variable `JULES_API_KEY`")]
    MissingApiKey,
    #[error(transparent)]
    Jules(#[from] JulesError),
}

#[tokio::main]
async fn main() -> Result<(), ExampleError> {
    let api_key = env::var("JULES_API_KEY").map_err(|_| ExampleError::MissingApiKey)?;
    let client = JulesClient::builder(api_key).build()?;

    let deleted_sessions = client.delete_completed_sessions().await?;
    if deleted_sessions.is_empty() {
        println!("No completed sessions found.");
    } else {
        println!("Deleted {} completed session(s):", deleted_sessions.len());
        for session_name in deleted_sessions {
            println!("- {session_name}");
        }
    }

    Ok(())
}
