use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jules_rs::{
    AutomationMode, CreateSessionRequest, JulesClient, JulesError, RetryPolicy, Session,
    SessionState, SourceContext, TimeoutPolicy,
};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

const DEFAULT_STATE_FILE: &str = ".jules-director-state.json";
const DEFAULT_MAX_CYCLES: u32 = 600;
const DEFAULT_MAX_POLLS_PER_TASK: u32 = 260;
const DEFAULT_MAX_AUTO_FEEDBACK_MESSAGES: u32 = 6;
const DEFAULT_AUTO_FEEDBACK_MESSAGE: &str = "Continue autonomously with the best safe assumption. Minimize questions and document assumptions in the PR.";
const INITIAL_POLL_INTERVAL: Duration = Duration::from_secs(3);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
enum DirectorError {
    #[error("missing environment variable `JULES_API_KEY`")]
    MissingApiKey,
    #[error("usage error: {0}")]
    Usage(String),
    #[error("state file not found at `{0}`")]
    MissingStateFile(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Jules(#[from] JulesError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cli {
    mode: CommandMode,
    state_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandMode {
    Init {
        goal: String,
        source: String,
    },
    Run {
        goal: Option<String>,
        source: Option<String>,
        max_cycles: u32,
    },
    Tick,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DirectorState {
    version: u32,
    product_goal: String,
    source: String,
    created_at_unix: u64,
    updated_at_unix: u64,
    policy: DirectorPolicy,
    tasks: Vec<TaskState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DirectorPolicy {
    auto_feedback_message: String,
    max_auto_feedback_messages: u32,
    max_polls_per_task: u32,
    quality_gate_commands: Vec<String>,
}

impl DirectorPolicy {
    fn from_env() -> Self {
        let auto_feedback_message = env::var("JULES_DIRECTOR_AUTO_FEEDBACK_MESSAGE")
            .ok()
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| DEFAULT_AUTO_FEEDBACK_MESSAGE.to_string());

        let max_auto_feedback_messages = env::var("JULES_DIRECTOR_MAX_AUTO_FEEDBACK_MESSAGES")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_AUTO_FEEDBACK_MESSAGES);

        let max_polls_per_task = env::var("JULES_DIRECTOR_MAX_POLLS_PER_TASK")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_POLLS_PER_TASK);

        let quality_gate_commands = env::var("JULES_DIRECTOR_QUALITY_GATES")
            .ok()
            .map(|value| {
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Self {
            auto_feedback_message,
            max_auto_feedback_messages,
            max_polls_per_task,
            quality_gate_commands,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskState {
    id: String,
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    status: TaskStatus,
    attempts: u32,
    poll_attempts: u32,
    plan_approved: bool,
    feedback_messages_sent: u32,
    session_name: Option<String>,
    session_url: Option<String>,
    last_session_state: Option<SessionState>,
    commit_hash: Option<String>,
    pr_url: Option<String>,
    last_error: Option<String>,
    remediation_for_task_id: Option<String>,
}

impl TaskState {
    fn pending(
        id: String,
        title: impl Into<String>,
        description: impl Into<String>,
        acceptance_criteria: Vec<String>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            description: description.into(),
            acceptance_criteria,
            status: TaskStatus::Pending,
            attempts: 0,
            poll_attempts: 0,
            plan_approved: false,
            feedback_messages_sent: 0,
            session_name: None,
            session_url: None,
            last_session_state: None,
            commit_hash: None,
            pr_url: None,
            last_error: None,
            remediation_for_task_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TaskStatus {
    Pending,
    Running,
    PrCandidate,
    Completed,
    NeedsRemediation,
    Failed,
    Blocked,
}

impl TaskStatus {
    fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedbackAction {
    AutoRespond,
    Escalate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TickOutcome {
    SessionCreated {
        task_id: String,
        session_name: String,
    },
    PlanApproved {
        task_id: String,
        session_name: String,
    },
    AutoFeedbackSent {
        task_id: String,
        session_name: String,
        sent_count: u32,
    },
    SessionInProgress {
        task_id: String,
        state: SessionState,
        poll_attempts: u32,
    },
    TaskCompleted {
        task_id: String,
        pr_url: Option<String>,
    },
    GateFailedQueuedRemediation {
        task_id: String,
        remediation_task_id: String,
    },
    TaskFailed {
        task_id: String,
        reason: String,
    },
    TaskBlocked {
        task_id: String,
        reason: String,
    },
    Idle,
}

impl TickOutcome {
    fn made_progress(&self) -> bool {
        !matches!(self, Self::SessionInProgress { .. } | Self::Idle)
    }

    fn summary(&self) -> String {
        match self {
            Self::SessionCreated {
                task_id,
                session_name,
            } => format!("created session `{session_name}` for task `{task_id}`"),
            Self::PlanApproved {
                task_id,
                session_name,
            } => format!("approved plan for task `{task_id}` in session `{session_name}`"),
            Self::AutoFeedbackSent {
                task_id,
                session_name,
                sent_count,
            } => format!(
                "sent automated feedback #{sent_count} for task `{task_id}` in session `{session_name}`"
            ),
            Self::SessionInProgress {
                task_id,
                state,
                poll_attempts,
            } => format!("task `{task_id}` is {state:?} (poll {poll_attempts})"),
            Self::TaskCompleted { task_id, pr_url } => {
                if let Some(pr_url) = pr_url {
                    format!("task `{task_id}` produced PR candidate: {pr_url}")
                } else {
                    format!("task `{task_id}` completed without PR metadata")
                }
            }
            Self::GateFailedQueuedRemediation {
                task_id,
                remediation_task_id,
            } => format!(
                "task `{task_id}` failed quality gates; queued remediation task `{remediation_task_id}`"
            ),
            Self::TaskFailed { task_id, reason } => {
                format!("task `{task_id}` failed: {reason}")
            }
            Self::TaskBlocked { task_id, reason } => {
                format!("task `{task_id}` blocked: {reason}")
            }
            Self::Idle => "no pending or running tasks".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GateCommandResult {
    command: String,
    success: bool,
    exit_code: Option<i32>,
    stderr: String,
}

#[tokio::main]
async fn main() -> Result<(), DirectorError> {
    let cli = parse_cli(env::args().skip(1))?;
    match cli.mode {
        CommandMode::Init { goal, source } => {
            let source = normalize_source_name(&source)?;
            let state = bootstrap_state(goal, source, DirectorPolicy::from_env());
            save_state(&cli.state_path, &state)?;
            println!("initialized director state at {}", cli.state_path.display());
            print_status(&state);
            Ok(())
        }
        CommandMode::Status => {
            let state = load_state(&cli.state_path)?;
            print_status(&state);
            Ok(())
        }
        CommandMode::Tick => {
            let api_key = env::var("JULES_API_KEY").map_err(|_| DirectorError::MissingApiKey)?;
            let client = build_client(api_key)?;
            let workspace_root = env::current_dir()?;
            let mut state = load_state(&cli.state_path)?;
            let outcome = tick_once(&client, &mut state, &workspace_root).await?;
            state.updated_at_unix = now_unix();
            save_state(&cli.state_path, &state)?;
            println!("{}", outcome.summary());
            print_status_counts(&state);
            Ok(())
        }
        CommandMode::Run {
            goal,
            source,
            max_cycles,
        } => {
            let api_key = env::var("JULES_API_KEY").map_err(|_| DirectorError::MissingApiKey)?;
            let client = build_client(api_key)?;
            let workspace_root = env::current_dir()?;
            let mut state = ensure_state(&cli.state_path, goal, source)?;
            run_until_settled(
                &client,
                &mut state,
                &cli.state_path,
                max_cycles,
                &workspace_root,
            )
            .await
        }
    }
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Cli, DirectorError> {
    let raw_args: Vec<String> = args.into_iter().collect();
    if raw_args.is_empty() {
        return Err(DirectorError::Usage(usage().to_string()));
    }

    let mut state_path = default_state_path();
    let mut max_cycles = DEFAULT_MAX_CYCLES;
    let mut positional = Vec::new();
    let mut index = 0_usize;
    while index < raw_args.len() {
        let token = &raw_args[index];
        match token.as_str() {
            "--state" => {
                let value = raw_args
                    .get(index + 1)
                    .ok_or_else(|| DirectorError::Usage("missing value for --state".to_string()))?;
                state_path = PathBuf::from(value);
                index += 2;
            }
            "--max-cycles" => {
                let value = raw_args.get(index + 1).ok_or_else(|| {
                    DirectorError::Usage("missing value for --max-cycles".to_string())
                })?;
                max_cycles = value.parse::<u32>().map_err(|_| {
                    DirectorError::Usage(format!(
                        "invalid `--max-cycles` value `{value}`: expected positive integer"
                    ))
                })?;
                index += 2;
            }
            _ if token.starts_with("--") => {
                return Err(DirectorError::Usage(format!("unknown option `{token}`")));
            }
            _ => {
                positional.push(token.clone());
                index += 1;
            }
        }
    }

    if positional.is_empty() {
        return Err(DirectorError::Usage(usage().to_string()));
    }

    let mode = match positional[0].as_str() {
        "init" => {
            if positional.len() != 3 {
                return Err(DirectorError::Usage(
                    "init requires: init \"<goal>\" \"<source>\"".to_string(),
                ));
            }
            CommandMode::Init {
                goal: positional[1].clone(),
                source: positional[2].clone(),
            }
        }
        "run" => match positional.len() {
            1 => CommandMode::Run {
                goal: None,
                source: None,
                max_cycles,
            },
            3 => CommandMode::Run {
                goal: Some(positional[1].clone()),
                source: Some(positional[2].clone()),
                max_cycles,
            },
            _ => {
                return Err(DirectorError::Usage(
                    "run supports either `run` (existing state) or `run \"<goal>\" \"<source>\"`"
                        .to_string(),
                ));
            }
        },
        "tick" => {
            if positional.len() != 1 {
                return Err(DirectorError::Usage(
                    "tick does not accept positional arguments".to_string(),
                ));
            }
            CommandMode::Tick
        }
        "status" => {
            if positional.len() != 1 {
                return Err(DirectorError::Usage(
                    "status does not accept positional arguments".to_string(),
                ));
            }
            CommandMode::Status
        }
        _ => return Err(DirectorError::Usage(usage().to_string())),
    };

    Ok(Cli { mode, state_path })
}

fn usage() -> &'static str {
    "director usage:
  director init \"<goal>\" \"<source>\" [--state <path>]
  director run [\"<goal>\" \"<source>\"] [--max-cycles <n>] [--state <path>]
  director tick [--state <path>]
  director status [--state <path>]"
}

fn default_state_path() -> PathBuf {
    env::var("JULES_DIRECTOR_STATE")
        .map_or_else(|_| PathBuf::from(DEFAULT_STATE_FILE), PathBuf::from)
}

fn build_client(api_key: String) -> Result<JulesClient, DirectorError> {
    JulesClient::builder(api_key)
        .retry_policy(RetryPolicy {
            max_retries: 3,
            initial_backoff: Duration::from_millis(300),
            max_backoff: Duration::from_secs(3),
        })
        .timeout_policy(TimeoutPolicy {
            request_timeout: Duration::from_secs(45),
        })
        .build()
        .map_err(DirectorError::from)
}

fn normalize_source_name(source: &str) -> Result<String, DirectorError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(DirectorError::Usage("source cannot be empty".to_string()));
    }
    if trimmed.starts_with("sources/") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("sources/{trimmed}"))
    }
}

fn ensure_state(
    state_path: &Path,
    goal: Option<String>,
    source: Option<String>,
) -> Result<DirectorState, DirectorError> {
    if state_path.exists() {
        let state = load_state(state_path)?;
        if let Some(goal) = goal {
            if goal != state.product_goal {
                return Err(DirectorError::InvalidState(format!(
                    "state goal `{}` does not match provided goal `{goal}`",
                    state.product_goal
                )));
            }
        }
        if let Some(source) = source {
            let normalized_source = normalize_source_name(&source)?;
            if normalized_source != state.source {
                return Err(DirectorError::InvalidState(format!(
                    "state source `{}` does not match provided source `{normalized_source}`",
                    state.source
                )));
            }
        }
        return Ok(state);
    }

    let goal = goal.ok_or_else(|| {
        DirectorError::Usage(
            "state file does not exist; provide `run \"<goal>\" \"<source>\"`".to_string(),
        )
    })?;
    let source = source.ok_or_else(|| {
        DirectorError::Usage(
            "state file does not exist; provide `run \"<goal>\" \"<source>\"`".to_string(),
        )
    })?;
    let normalized_source = normalize_source_name(&source)?;
    let state = bootstrap_state(goal, normalized_source, DirectorPolicy::from_env());
    save_state(state_path, &state)?;
    Ok(state)
}

fn bootstrap_state(goal: String, source: String, policy: DirectorPolicy) -> DirectorState {
    let now = now_unix();
    DirectorState {
        version: 1,
        product_goal: goal.clone(),
        source,
        created_at_unix: now,
        updated_at_unix: now,
        policy,
        tasks: build_seed_tasks(&goal),
    }
}

fn build_seed_tasks(goal: &str) -> Vec<TaskState> {
    vec![
        TaskState::pending(
            "t-1".to_string(),
            "Create product architecture slice",
            format!(
                "Design and implement the first vertical slice for `{goal}` with clear module boundaries."
            ),
            vec![
                "A vertical slice is implemented end-to-end in code.".to_string(),
                "Architecture decisions are documented in commit/PR context.".to_string(),
            ],
        ),
        TaskState::pending(
            "t-2".to_string(),
            "Implement core workflow",
            format!("Deliver the primary user workflow that moves `{goal}` forward."),
            vec![
                "Primary workflow executes successfully in the target repo.".to_string(),
                "Behavior is covered by automated tests.".to_string(),
            ],
        ),
        TaskState::pending(
            "t-3".to_string(),
            "Harden quality and docs",
            format!("Add tests, docs, and polish to make `{goal}` shippable."),
            vec![
                "Critical behavior is tested and passing.".to_string(),
                "User-facing docs are updated for the implemented changes.".to_string(),
            ],
        ),
    ]
}

fn choose_feedback_action(task: &TaskState, policy: &DirectorPolicy) -> FeedbackAction {
    if task.feedback_messages_sent < policy.max_auto_feedback_messages {
        FeedbackAction::AutoRespond
    } else {
        FeedbackAction::Escalate
    }
}

async fn run_until_settled(
    client: &JulesClient,
    state: &mut DirectorState,
    state_path: &Path,
    max_cycles: u32,
    workspace_root: &Path,
) -> Result<(), DirectorError> {
    let mut delay = INITIAL_POLL_INTERVAL;
    for cycle in 1..=max_cycles {
        let outcome = tick_once(client, state, workspace_root).await?;
        state.updated_at_unix = now_unix();
        save_state(state_path, state)?;
        println!("cycle {cycle}: {}", outcome.summary());
        print_status_counts(state);

        if all_tasks_terminal(state) {
            println!("all tasks reached terminal status");
            print_status(state);
            return Ok(());
        }

        delay = next_poll_delay(delay, outcome.made_progress());
        sleep(delay).await;
    }

    Err(DirectorError::InvalidState(format!(
        "run loop hit --max-cycles={max_cycles} before settling"
    )))
}

async fn tick_once(
    client: &JulesClient,
    state: &mut DirectorState,
    workspace_root: &Path,
) -> Result<TickOutcome, DirectorError> {
    if let Some(task_index) = state
        .tasks
        .iter()
        .position(|task| task.status == TaskStatus::Running)
    {
        return poll_running_task(client, state, task_index, workspace_root).await;
    }

    if let Some(task_index) = state
        .tasks
        .iter()
        .position(|task| task.status == TaskStatus::Pending)
    {
        return create_session_for_task(client, state, task_index).await;
    }

    Ok(TickOutcome::Idle)
}

async fn create_session_for_task(
    client: &JulesClient,
    state: &mut DirectorState,
    task_index: usize,
) -> Result<TickOutcome, DirectorError> {
    let prompt = {
        let task = &state.tasks[task_index];
        build_task_prompt(state, task)
    };
    let title = format!("director: {}", state.tasks[task_index].title);
    let source = state.source.clone();
    let session = client
        .create_session(CreateSessionRequest {
            prompt,
            title: Some(title),
            source_context: Some(SourceContext {
                source,
                github_repo_context: None,
            }),
            require_plan_approval: Some(false),
            automation_mode: Some(AutomationMode::AutoCreatePr),
        })
        .await?;

    let task = &mut state.tasks[task_index];
    task.status = TaskStatus::Running;
    task.attempts = task.attempts.saturating_add(1);
    task.poll_attempts = 0;
    task.plan_approved = false;
    task.feedback_messages_sent = 0;
    task.session_name = Some(session.name.clone());
    task.session_url = session.url.clone();
    task.last_session_state = session.state;
    task.last_error = None;

    Ok(TickOutcome::SessionCreated {
        task_id: task.id.clone(),
        session_name: session.name,
    })
}

async fn poll_running_task(
    client: &JulesClient,
    state: &mut DirectorState,
    task_index: usize,
    workspace_root: &Path,
) -> Result<TickOutcome, DirectorError> {
    let (task_id, session_name, auto_feedback_message, max_polls_per_task) = {
        let task = &state.tasks[task_index];
        (
            task.id.clone(),
            task.session_name.clone().ok_or_else(|| {
                DirectorError::InvalidState(format!(
                    "task `{}` is running without a session_name",
                    task.id
                ))
            })?,
            state.policy.auto_feedback_message.clone(),
            state.policy.max_polls_per_task,
        )
    };

    let session = client.get_session(&session_name).await?;
    let session_state = session.state.unwrap_or(SessionState::Unknown);

    {
        let task = &mut state.tasks[task_index];
        task.poll_attempts = task.poll_attempts.saturating_add(1);
        task.last_session_state = Some(session_state);
        if task.session_url.is_none() {
            task.session_url = session.url.clone();
        }
    }

    match session_state {
        SessionState::AwaitingPlanApproval => {
            client.approve_plan(&session_name).await?;
            let task = &mut state.tasks[task_index];
            task.plan_approved = true;
            return Ok(TickOutcome::PlanApproved {
                task_id,
                session_name,
            });
        }
        SessionState::AwaitingUserFeedback => {
            match choose_feedback_action(&state.tasks[task_index], &state.policy) {
                FeedbackAction::AutoRespond => {
                    client
                        .send_message(&session_name, &auto_feedback_message)
                        .await?;
                    let sent_count = {
                        let task = &mut state.tasks[task_index];
                        task.feedback_messages_sent = task.feedback_messages_sent.saturating_add(1);
                        task.feedback_messages_sent
                    };
                    return Ok(TickOutcome::AutoFeedbackSent {
                        task_id,
                        session_name,
                        sent_count,
                    });
                }
                FeedbackAction::Escalate => {
                    let reason = format!(
                        "awaiting_user_feedback exceeded automatic limit ({})",
                        state.policy.max_auto_feedback_messages
                    );
                    let task = &mut state.tasks[task_index];
                    task.status = TaskStatus::Blocked;
                    task.last_error = Some(reason.clone());
                    return Ok(TickOutcome::TaskBlocked { task_id, reason });
                }
            }
        }
        SessionState::Completed => {
            return complete_task_from_session(state, task_index, &session, workspace_root);
        }
        SessionState::Failed => {
            let reason = format!("session `{session_name}` reached FAILED");
            let task = &mut state.tasks[task_index];
            task.status = TaskStatus::Failed;
            task.last_error = Some(reason.clone());
            return Ok(TickOutcome::TaskFailed { task_id, reason });
        }
        SessionState::SessionStateUnspecified
        | SessionState::Queued
        | SessionState::Planning
        | SessionState::InProgress
        | SessionState::Paused
        | SessionState::Unknown => {}
    }

    let poll_attempts = state.tasks[task_index].poll_attempts;
    if poll_attempts >= max_polls_per_task {
        let reason = format!("poll limit reached at {poll_attempts} polls");
        let task = &mut state.tasks[task_index];
        task.status = TaskStatus::Blocked;
        task.last_error = Some(reason.clone());
        return Ok(TickOutcome::TaskBlocked { task_id, reason });
    }

    Ok(TickOutcome::SessionInProgress {
        task_id,
        state: session_state,
        poll_attempts,
    })
}

fn complete_task_from_session(
    state: &mut DirectorState,
    task_index: usize,
    session: &Session,
    workspace_root: &Path,
) -> Result<TickOutcome, DirectorError> {
    let (commit_hash, pr_url) = extract_session_artifacts(session);
    let gate_results = run_quality_gates(&state.policy.quality_gate_commands, workspace_root)?;
    let gate_failures = gate_results
        .iter()
        .filter(|result| !result.success)
        .cloned()
        .collect::<Vec<_>>();

    if gate_failures.is_empty() {
        let task = &mut state.tasks[task_index];
        task.commit_hash = commit_hash;
        task.pr_url = pr_url.clone();
        task.status = if pr_url.is_some() {
            TaskStatus::PrCandidate
        } else {
            TaskStatus::Completed
        };
        task.last_error = None;
        return Ok(TickOutcome::TaskCompleted {
            task_id: task.id.clone(),
            pr_url,
        });
    }

    let failure_summary = summarize_gate_failures(&gate_failures);
    let (task_id, task_title) = {
        let task = &mut state.tasks[task_index];
        task.commit_hash = commit_hash;
        task.pr_url = pr_url;
        task.status = TaskStatus::NeedsRemediation;
        task.last_error = Some(failure_summary.clone());
        (task.id.clone(), task.title.clone())
    };

    let remediation_task = build_remediation_task(state, &task_id, &task_title, &failure_summary);
    let remediation_task_id = remediation_task.id.clone();
    state.tasks.push(remediation_task);
    Ok(TickOutcome::GateFailedQueuedRemediation {
        task_id,
        remediation_task_id,
    })
}

fn build_remediation_task(
    state: &DirectorState,
    parent_task_id: &str,
    parent_task_title: &str,
    failure_summary: &str,
) -> TaskState {
    let task_id = format!("t-{}", state.tasks.len() + 1);
    let mut task = TaskState::pending(
        task_id,
        format!("Remediate quality gates for {parent_task_title}"),
        format!(
            "Resolve quality gate failures for completed task `{parent_task_id}`.\n\
             Failure summary: {failure_summary}"
        ),
        vec![
            "All configured quality gate commands pass.".to_string(),
            "Changes are published in a PR with remediation notes.".to_string(),
        ],
    );
    task.remediation_for_task_id = Some(parent_task_id.to_string());
    task
}

fn run_quality_gates(
    commands: &[String],
    workspace_root: &Path,
) -> Result<Vec<GateCommandResult>, DirectorError> {
    let mut reports = Vec::new();
    for command in commands {
        let output = execute_shell(command, workspace_root)?;
        reports.push(GateCommandResult {
            command: command.clone(),
            success: output.status.success(),
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(reports)
}

fn execute_shell(command: &str, workspace_root: &Path) -> Result<Output, DirectorError> {
    let mut shell = if cfg!(target_os = "windows") {
        let mut shell = Command::new("powershell");
        shell.arg("-NoProfile").arg("-Command").arg(command);
        shell
    } else {
        let mut shell = Command::new("sh");
        shell.arg("-lc").arg(command);
        shell
    };
    shell.current_dir(workspace_root);
    shell.output().map_err(DirectorError::from)
}

fn summarize_gate_failures(failures: &[GateCommandResult]) -> String {
    failures
        .iter()
        .map(|failure| match failure.exit_code {
            Some(exit_code) => format!("`{}` failed with exit code {exit_code}", failure.command),
            None => format!("`{}` failed without an exit code", failure.command),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn extract_session_artifacts(session: &Session) -> (Option<String>, Option<String>) {
    let output = session.outputs.first().or(session.output.as_ref());
    let commit_hash = output.and_then(|value| value.commit_hash.clone());
    let pr_url = output
        .and_then(|value| value.pull_request.as_ref())
        .map(|pull_request| pull_request.url.clone());
    (commit_hash, pr_url)
}

fn build_task_prompt(state: &DirectorState, task: &TaskState) -> String {
    let acceptance_criteria = task
        .acceptance_criteria
        .iter()
        .map(|criterion| format!("- {criterion}"))
        .collect::<Vec<_>>()
        .join("\n");
    let remediation_note = task
        .remediation_for_task_id
        .as_ref()
        .map_or_else(String::new, |parent_id| {
            format!(
                "\nRemediation note:\n- This task remediates quality gate failures from `{parent_id}`."
            )
        });

    format!(
        "Product goal:
{product_goal}

Task ID: {task_id}
Task title: {task_title}
Task description:
{task_description}

Acceptance criteria:
{acceptance_criteria}

Execution policy:
- Minimize requests for user feedback.
- When blocked, choose the safest reasonable assumption and proceed.
- Document assumptions in commit or PR descriptions.
- Run relevant tests and include verification notes in the PR.
- Prefer publishing a pull request when work is ready.{remediation_note}",
        product_goal = state.product_goal,
        task_id = task.id,
        task_title = task.title,
        task_description = task.description,
        acceptance_criteria = acceptance_criteria,
        remediation_note = remediation_note,
    )
}

fn print_status(state: &DirectorState) {
    println!("goal: {}", state.product_goal);
    println!("source: {}", state.source);
    println!("state file version: {}", state.version);
    print_status_counts(state);
    for task in &state.tasks {
        println!("[{:?}] {} ({})", task.status, task.title, task.id);
        if let Some(session_name) = &task.session_name {
            println!("  session: {session_name}");
        }
        if let Some(pr_url) = &task.pr_url {
            println!("  pr: {pr_url}");
        }
        if let Some(last_error) = &task.last_error {
            println!("  note: {last_error}");
        }
    }
}

fn print_status_counts(state: &DirectorState) {
    let pending = state
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .count();
    let running = state
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Running)
        .count();
    let completed_like = state
        .tasks
        .iter()
        .filter(|task| matches!(task.status, TaskStatus::PrCandidate | TaskStatus::Completed))
        .count();
    let blocked = state
        .tasks
        .iter()
        .filter(|task| matches!(task.status, TaskStatus::Blocked | TaskStatus::Failed))
        .count();
    println!(
        "counts: pending={pending} running={running} completed_or_pr={completed_like} blocked_or_failed={blocked}"
    );
}

fn all_tasks_terminal(state: &DirectorState) -> bool {
    state.tasks.iter().all(|task| !task.status.is_active())
}

fn next_poll_delay(current: Duration, made_progress: bool) -> Duration {
    if made_progress {
        INITIAL_POLL_INTERVAL
    } else {
        current.saturating_mul(2).min(MAX_POLL_INTERVAL)
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn load_state(state_path: &Path) -> Result<DirectorState, DirectorError> {
    if !state_path.exists() {
        return Err(DirectorError::MissingStateFile(
            state_path.display().to_string(),
        ));
    }
    let bytes = fs::read(state_path)?;
    serde_json::from_slice(&bytes).map_err(DirectorError::from)
}

fn save_state(state_path: &Path, state: &DirectorState) -> Result<(), DirectorError> {
    if let Some(parent) = state_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let serialized = serde_json::to_vec_pretty(state)?;
    fs::write(state_path, serialized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_seed_tasks_creates_multiple_pending_tasks() {
        let tasks = build_seed_tasks("build a project management SaaS");
        assert!(
            tasks.len() >= 3,
            "seed tasks should create at least three product-building steps"
        );
        assert!(
            tasks.iter().all(|task| task.status == TaskStatus::Pending),
            "all seed tasks should start pending"
        );
    }

    #[test]
    fn choose_feedback_action_prefers_automation_until_limit() {
        let policy = DirectorPolicy {
            auto_feedback_message: "Proceed autonomously.".to_string(),
            max_auto_feedback_messages: 2,
            max_polls_per_task: 120,
            quality_gate_commands: Vec::new(),
        };
        let task = TaskState {
            id: "t-1".to_string(),
            title: "Implement auth".to_string(),
            description: "desc".to_string(),
            acceptance_criteria: vec![],
            status: TaskStatus::Running,
            attempts: 1,
            poll_attempts: 5,
            plan_approved: false,
            feedback_messages_sent: 1,
            session_name: Some("sessions/s-1".to_string()),
            session_url: None,
            last_session_state: Some(SessionState::AwaitingUserFeedback),
            commit_hash: None,
            pr_url: None,
            last_error: None,
            remediation_for_task_id: None,
        };
        assert_eq!(
            choose_feedback_action(&task, &policy),
            FeedbackAction::AutoRespond
        );

        let saturated_task = TaskState {
            feedback_messages_sent: 2,
            ..task
        };
        assert_eq!(
            choose_feedback_action(&saturated_task, &policy),
            FeedbackAction::Escalate
        );
    }

    #[test]
    fn next_poll_delay_resets_when_progress_is_made() {
        let current = Duration::from_secs(24);
        assert_eq!(next_poll_delay(current, true), INITIAL_POLL_INTERVAL);
    }

    #[test]
    fn next_poll_delay_grows_with_cap() {
        let next = next_poll_delay(Duration::from_secs(3), false);
        let capped = next_poll_delay(Duration::from_secs(90), false);
        assert_eq!(next, Duration::from_secs(6));
        assert_eq!(capped, MAX_POLL_INTERVAL);
    }

    #[test]
    fn normalize_source_name_adds_prefix() {
        assert_eq!(
            normalize_source_name("github/acme/widgets").expect("source normalization"),
            "sources/github/acme/widgets".to_string()
        );
        assert_eq!(
            normalize_source_name("sources/github/acme/widgets").expect("source normalization"),
            "sources/github/acme/widgets".to_string()
        );
    }

    #[test]
    fn parse_cli_accepts_run_without_goal_when_state_exists() {
        let cli = parse_cli(vec!["run".to_string()]).expect("parse run");
        assert_eq!(
            cli.mode,
            CommandMode::Run {
                goal: None,
                source: None,
                max_cycles: DEFAULT_MAX_CYCLES
            }
        );
    }
}
