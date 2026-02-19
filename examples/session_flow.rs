use std::env;
use std::time::Duration;

use jules_rs::{
    CreateSessionRequest, JulesClient, JulesError, ListSourcesParams, RetryPolicy, Session,
    SessionGithubRepoContext, SessionState, SourceContext, TimeoutPolicy,
};
use tokio::time::sleep;

const DEFAULT_PROMPT: &str = "Inspect the repository and summarize the first safe improvement.";
const INITIAL_POLL_INTERVAL: Duration = Duration::from_secs(3);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_POLLS: u32 = 120;

#[derive(Debug, thiserror::Error)]
enum ExampleError {
    #[error("missing environment variable `JULES_API_KEY`")]
    MissingApiKey,
    #[error("source argument cannot be empty")]
    EmptySource,
    #[error("source does not expose a usable default branch: {0}")]
    MissingStartingBranch(String),
    #[error("no Jules sources were returned; pass a source id as the second argument")]
    NoSources,
    #[error("session did not finish after {0} polls")]
    PollLimitReached(u32),
    #[error("session failed: {0}")]
    SessionFailed(String),
    #[error(transparent)]
    Jules(#[from] JulesError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalOutcome {
    Success,
    Failure,
}

#[tokio::main]
async fn main() -> Result<(), ExampleError> {
    let api_key = env::var("JULES_API_KEY").map_err(|_| ExampleError::MissingApiKey)?;
    let mut args = env::args().skip(1);
    let prompt = args.next().unwrap_or_else(|| DEFAULT_PROMPT.to_string());
    let source_arg = args.next();

    let client = JulesClient::builder(api_key)
        .retry_policy(RetryPolicy {
            max_retries: 3,
            initial_backoff: Duration::from_millis(300),
            max_backoff: Duration::from_secs(3),
        })
        .timeout_policy(TimeoutPolicy {
            request_timeout: Duration::from_secs(45),
        })
        .build()?;

    let normalized_source_name = if let Some(source_arg) = source_arg {
        normalize_source_name(&source_arg)?
    } else {
        let response = client
            .list_sources(ListSourcesParams {
                page_size: Some(1),
                page_token: None,
            })
            .await?;
        response
            .sources
            .into_iter()
            .next()
            .map(|source| source.name)
            .ok_or(ExampleError::NoSources)?
    };
    let source_context = resolve_source_context(&client, &normalized_source_name).await?;
    let source_name = source_context.source.clone();
    if let Some(github_repo_context) = &source_context.github_repo_context {
        println!(
            "Using starting branch: {}",
            github_repo_context.starting_branch
        );
    }

    println!("Using source: {source_name}");
    let session = client
        .create_session(CreateSessionRequest {
            prompt,
            title: Some("jules-rs example session".to_string()),
            source_context: Some(source_context),
            require_plan_approval: Some(true),
            automation_mode: None,
        })
        .await?;
    println!("Created session: {}", session.name);

    wait_for_completion(&client, &session.name).await
}

fn normalize_source_name(source_arg: &str) -> Result<String, ExampleError> {
    let trimmed = source_arg.trim();
    if trimmed.is_empty() {
        return Err(ExampleError::EmptySource);
    }

    if trimmed.starts_with("sources/") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("sources/{trimmed}"))
    }
}

async fn resolve_source_context(
    client: &JulesClient,
    normalized_source_name: &str,
) -> Result<SourceContext, ExampleError> {
    let source = client.get_source(normalized_source_name).await?;
    let github_repo_context = source
        .github_repo
        .as_ref()
        .map(|github_repo| {
            let default_branch = github_repo
                .default_branch
                .as_ref()
                .map(|branch| branch.display_name.clone())
                .or_else(|| {
                    github_repo
                        .branches
                        .first()
                        .map(|branch| branch.display_name.clone())
                });
            default_branch
                .map(|starting_branch| SessionGithubRepoContext { starting_branch })
                .ok_or_else(|| ExampleError::MissingStartingBranch(source.name.clone()))
        })
        .transpose()?;

    Ok(SourceContext {
        source: source.name,
        github_repo_context,
    })
}

async fn wait_for_completion(client: &JulesClient, session_name: &str) -> Result<(), ExampleError> {
    let mut approved_plan = false;
    let mut previous_state: Option<SessionState> = None;
    let mut poll_delay = INITIAL_POLL_INTERVAL;
    for poll_count in 1..=MAX_POLLS {
        let session = client.get_session(session_name).await?;
        let state = session.state.unwrap_or(SessionState::Unknown);
        println!("poll {poll_count}: state={state:?}");

        if let Some(outcome) = terminal_outcome(state) {
            print_output(&session);
            return match outcome {
                TerminalOutcome::Success => Ok(()),
                TerminalOutcome::Failure => Err(ExampleError::SessionFailed(session.name)),
            };
        }

        match state {
            SessionState::AwaitingPlanApproval if !approved_plan => {
                println!("Approving generated plan...");
                client.approve_plan(session_name).await?;
                approved_plan = true;
            }
            SessionState::AwaitingUserFeedback if should_send_feedback(previous_state, state) => {
                println!("Session requested input, sending a short follow-up message...");
                client
                    .send_message(session_name, "Continue with the best next step.")
                    .await?;
            }
            SessionState::AwaitingUserFeedback
            | SessionState::AwaitingPlanApproval
            | SessionState::SessionStateUnspecified
            | SessionState::Queued
            | SessionState::Planning
            | SessionState::InProgress
            | SessionState::Paused
            | SessionState::Unknown => {}
            SessionState::Completed | SessionState::Failed => unreachable!(),
        }

        if poll_count < MAX_POLLS {
            poll_delay = next_poll_delay(previous_state, state, poll_delay);
            previous_state = Some(state);
            println!("next poll in {}s", poll_delay.as_secs());
            sleep(poll_delay).await;
        }
    }

    Err(ExampleError::PollLimitReached(MAX_POLLS))
}

fn terminal_outcome(state: SessionState) -> Option<TerminalOutcome> {
    match state {
        SessionState::Completed => Some(TerminalOutcome::Success),
        SessionState::Failed => Some(TerminalOutcome::Failure),
        SessionState::SessionStateUnspecified
        | SessionState::Queued
        | SessionState::Planning
        | SessionState::AwaitingPlanApproval
        | SessionState::AwaitingUserFeedback
        | SessionState::InProgress
        | SessionState::Paused
        | SessionState::Unknown => None,
    }
}

fn should_send_feedback(previous_state: Option<SessionState>, current_state: SessionState) -> bool {
    current_state == SessionState::AwaitingUserFeedback
        && previous_state != Some(SessionState::AwaitingUserFeedback)
}

fn next_poll_delay(
    previous_state: Option<SessionState>,
    current_state: SessionState,
    current_delay: Duration,
) -> Duration {
    if previous_state == Some(current_state) {
        current_delay.saturating_mul(2).min(MAX_POLL_INTERVAL)
    } else {
        INITIAL_POLL_INTERVAL
    }
}

fn print_output(session: &Session) {
    if let Some(output) = session.outputs.first().or(session.output.as_ref()) {
        println!("changed files: {}", output.changed_files.len());
        if let Some(commit_hash) = &output.commit_hash {
            println!("commit: {commit_hash}");
        }
        if let Some(pull_request) = &output.pull_request {
            println!("pull request: {}", pull_request.url);
        }
    } else {
        println!("no output payload was returned");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_poll_delay_resets_when_state_changes() {
        let delay = next_poll_delay(
            Some(SessionState::InProgress),
            SessionState::Planning,
            Duration::from_secs(24),
        );
        assert_eq!(delay, INITIAL_POLL_INTERVAL);
    }

    #[test]
    fn next_poll_delay_grows_exponentially_until_cap() {
        let first = next_poll_delay(
            Some(SessionState::InProgress),
            SessionState::InProgress,
            Duration::from_secs(3),
        );
        let second = next_poll_delay(
            Some(SessionState::InProgress),
            SessionState::InProgress,
            first,
        );
        let capped = next_poll_delay(
            Some(SessionState::InProgress),
            SessionState::InProgress,
            Duration::from_secs(30),
        );

        assert_eq!(first, Duration::from_secs(6));
        assert_eq!(second, Duration::from_secs(12));
        assert_eq!(capped, MAX_POLL_INTERVAL);
    }

    #[test]
    fn terminal_outcome_treats_failed_as_failure() {
        assert_eq!(
            terminal_outcome(SessionState::Completed),
            Some(TerminalOutcome::Success)
        );
        assert_eq!(
            terminal_outcome(SessionState::Failed),
            Some(TerminalOutcome::Failure)
        );
        assert_eq!(terminal_outcome(SessionState::InProgress), None);
    }

    #[test]
    fn should_send_feedback_only_when_entering_feedback_state() {
        assert!(should_send_feedback(
            Some(SessionState::InProgress),
            SessionState::AwaitingUserFeedback
        ));
        assert!(!should_send_feedback(
            Some(SessionState::AwaitingUserFeedback),
            SessionState::AwaitingUserFeedback
        ));
        assert!(!should_send_feedback(
            Some(SessionState::Queued),
            SessionState::Planning
        ));
    }
}
