use serde::{Deserialize, Serialize};

/// Request body for `POST /sessions`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// The initial prompt to start the session.
    pub prompt: String,
    /// Optional display title for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional source context to constrain repository scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_context: Option<SourceContext>,
    /// Whether Jules must ask for plan approval before executing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_plan_approval: Option<bool>,
    /// Controls how Jules should automate code application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_mode: Option<AutomationMode>,
}

/// Resource context used when creating a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceContext {
    /// Source resource name, for example `sources/my-repo`.
    pub source: String,
    /// Optional GitHub repository-specific session context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo_context: Option<SessionGithubRepoContext>,
}

/// Optional GitHub context used when starting a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGithubRepoContext {
    /// Starting branch that Jules should use for edits.
    pub starting_branch: String,
}

/// Session automation mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutomationMode {
    /// Apply changes manually in the local editor.
    None,
    /// Let Jules automatically create a pull request.
    AutoCreatePr,
    /// Future server enum values.
    #[serde(other)]
    Unknown,
}

/// Response from `GET /sessions`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    /// Session resources in this page.
    #[serde(default)]
    pub sessions: Vec<Session>,
    /// Cursor for the next page, if present.
    pub next_page_token: Option<String>,
}

/// Response from `GET /sources`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesResponse {
    /// Source resources in this page.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Cursor for the next page, if present.
    pub next_page_token: Option<String>,
}

/// Response from `GET /sessions/{session}/activities`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListActivitiesResponse {
    /// Activity resources in this page.
    #[serde(default)]
    pub activities: Vec<Activity>,
    /// Cursor for the next page, if present.
    pub next_page_token: Option<String>,
}

/// Session resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Resource name, for example `sessions/123`.
    pub name: String,
    /// Short session id, for example `123`.
    pub id: Option<String>,
    /// Original prompt used to create the session.
    pub prompt: Option<String>,
    /// Optional session title.
    pub title: Option<String>,
    /// Optional session description.
    pub description: Option<String>,
    /// Current high-level session state.
    pub state: Option<SessionState>,
    /// Source context selected for this session.
    pub source_context: Option<SourceContext>,
    /// Whether plan approval was required for this session.
    pub require_plan_approval: Option<bool>,
    /// Automation mode used by this session.
    pub automation_mode: Option<AutomationMode>,
    /// Session creation timestamp.
    pub create_time: Option<String>,
    /// Session update timestamp.
    pub update_time: Option<String>,
    /// URL for opening the session in Jules.
    pub url: Option<String>,
    /// Source associated with this session.
    pub source: Option<Source>,
    /// Current generated plan, if available.
    pub plan: Option<Plan>,
    /// Final output once a session completes.
    pub output: Option<SessionOutput>,
    /// Outputs emitted by the session.
    #[serde(default)]
    pub outputs: Vec<SessionOutput>,
}

/// Session states returned by the API.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionState {
    /// Default zero value.
    SessionStateUnspecified,
    /// Waiting in queue.
    Queued,
    /// Generating an execution plan.
    Planning,
    /// Waiting for plan approval.
    AwaitingPlanApproval,
    /// Waiting for follow-up user feedback.
    #[serde(alias = "AWAITING_USER_INPUT")]
    AwaitingUserFeedback,
    /// Actively executing work.
    #[serde(alias = "WORKING")]
    InProgress,
    /// Execution paused.
    Paused,
    /// Session finished successfully.
    #[serde(alias = "FINISHED")]
    Completed,
    /// Session ended with an error.
    #[serde(alias = "ERROR")]
    Failed,
    /// Future server enum values.
    #[serde(other)]
    Unknown,
}

/// Final or intermediate output payload for sessions and activities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutput {
    /// Files changed by the session.
    #[serde(default)]
    pub changed_files: Vec<ChangedFile>,
    /// Commit hash if a commit was generated.
    pub commit_hash: Option<String>,
    /// Pull request metadata when available.
    pub pull_request: Option<PullRequest>,
}

/// Single changed file entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedFile {
    /// File path.
    pub path: String,
    /// Unified diff patch content.
    pub diff: String,
}

/// Pull request metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    /// Pull request URL.
    pub url: String,
    /// Pull request title.
    pub title: Option<String>,
    /// Pull request number.
    pub number: Option<i64>,
}

/// Source resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Resource name, for example `sources/my-repo`.
    pub name: String,
    /// Short source id.
    pub id: Option<String>,
    /// Human-friendly source name.
    pub display_name: Option<String>,
    /// Optional source description.
    pub description: Option<String>,
    /// GitHub repository details when present.
    pub github_repo: Option<GithubRepoContext>,
}

/// GitHub repository context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepoContext {
    /// GitHub organization or user.
    pub owner: String,
    /// GitHub repository name.
    pub repo: String,
    /// Whether the repository is private.
    pub is_private: Option<bool>,
    /// Default branch information.
    pub default_branch: Option<Branch>,
    /// List of available branches.
    #[serde(default)]
    pub branches: Vec<Branch>,
}

/// Git branch metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    /// Branch display name.
    pub display_name: String,
}

/// Plan resource attached to a session or activity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    /// Ordered plan steps.
    #[serde(default)]
    pub steps: Vec<PlanStep>,
}

/// Single plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    /// Description of the step.
    pub description: String,
    /// Provider-defined status for the step.
    pub status: String,
}

/// Activity resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// Resource name, for example `sessions/123/activities/abc`.
    pub name: String,
    /// Activity status.
    pub status: Option<ActivityStatus>,
    /// High-level stage name.
    pub stage_name: Option<String>,
    /// Provider-defined activity type.
    pub activity_type: Option<String>,
    /// Human-readable detail text.
    pub detail: Option<String>,
    /// Event timestamp.
    pub timestamp: Option<String>,
    /// Structured overview payload.
    pub overview: Option<Overview>,
    /// Structured plan payload.
    pub plan: Option<Plan>,
    /// Structured user input request payload.
    pub user_input_request: Option<UserInputRequest>,
    /// Structured tool usage payload.
    pub tool_use: Option<ToolUse>,
    /// Structured diff payload.
    pub view_diff: Option<ViewDiff>,
    /// Structured commit payload.
    pub commit: Option<Commit>,
    /// Structured create PR payload.
    pub create_pull_request: Option<CreatePullRequest>,
    /// Activity-level output payload.
    pub output: Option<SessionOutput>,
}

/// Activity status values.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivityStatus {
    /// Default zero value.
    ActivityStatusUnspecified,
    /// Activity is still running.
    InProgress,
    /// Activity completed successfully.
    Success,
    /// Activity failed.
    Failed,
    /// Future server enum values.
    #[serde(other)]
    Unknown,
}

/// Activity overview payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Overview {
    /// Overall session state.
    pub state: Option<SessionState>,
    /// Raw thought text.
    pub thoughts: Option<String>,
    /// Human summary of progress.
    pub summary: Option<String>,
}

/// User input request payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserInputRequest {
    /// Question presented to the user.
    pub question: String,
    /// Suggested quick responses.
    #[serde(default)]
    pub suggested_responses: Vec<SuggestedResponse>,
}

/// Suggested response item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestedResponse {
    /// Suggested answer text.
    pub response: String,
}

/// Tool use payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUse {
    /// Tool name.
    pub tool_name: String,
    /// Tool input payload.
    pub input: String,
}

/// Diff view payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewDiff {
    /// Unified diff content.
    pub diff: String,
    /// Optional display title.
    pub title: Option<String>,
}

/// Commit payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    /// Commit message text.
    pub commit_message: String,
}

/// Create pull request payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreatePullRequest {
    /// Pull request title.
    pub title: String,
    /// Pull request body.
    pub body: String,
}

/// Query params for `GET /sources`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListSourcesParams {
    /// Maximum number of items in a page (1-100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// Page cursor returned by a previous call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// Query params for `GET /sessions`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsParams {
    /// Maximum number of items in a page (1-100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// Page cursor returned by a previous call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Optional filtering expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// Query params for `GET /sessions/{session}/activities`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListActivitiesParams {
    /// Maximum number of items in a page (1-100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// Page cursor returned by a previous call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Optional filtering expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Optional filter date, encoded by the API as `createTime`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
}

/// Request body for `POST /sessions/{session}:sendMessage`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageRequest {
    /// Prompt text to send to the session.
    pub prompt: String,
}
