//! `jules-pr-triage` — scan open Jules-authored pull requests across repos and
//! produce a merge / close / needs-human report.
//!
//! **Dry-run only.** This binary evaluates PRs and writes a report. It never
//! closes, merges, or otherwise mutates any pull request or branch — the GitHub
//! mutation methods (`close_pull`, `delete_branch`) are deliberately unused
//! here.
//!
//! Configuration (all via the environment / flags, in the repo's manual style):
//! * `GITHUB_TOKEN` — required; without it the tool exits non-zero.
//! * `JULES_API_KEY` — optional; enables Jules-side session correlation and
//!   repo discovery. Missing is a warning, not an error.
//! * `ANTHROPIC_API_KEY` — optional; enables the LLM evaluator.
//! * `--repo owner/repo` — repeatable, and/or a comma-separated list. When
//!   omitted, the repo set is derived from Jules sources.
//! * `--mode heuristic|llm|both` — default `both`.
//! * `--limit N` — cap PRs triaged per repo.
//! * `--out <path>` — report file (default `jules-pr-triage-report.md`).

use std::collections::BTreeSet;
use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use jules_rs::github::{CheckSummary, GithubClient, JulesMatcher, Pr};
use jules_rs::triage::{
    HeuristicEvaluator, LlmEvaluator, PrContext, Recommendation, SessionIndex, TriageMode,
    TriageResult, evaluate_pr, render_report, task_id_from_body,
};
use jules_rs::{
    AnthropicClient, JulesClient, ListSessionsParams, ListSourcesParams, Session, SessionReviewer,
    Source, TimeoutPolicy,
};
use tokio::task::JoinSet;

const EXIT_SUCCESS: u8 = 0;
const EXIT_USAGE: u8 = 2;
const EXIT_MISSING_TOKEN: u8 = 3;

/// Default report path when `--out` is not supplied.
const DEFAULT_OUT: &str = "jules-pr-triage-report.md";

/// Max concurrent per-repo triage tasks.
const REPO_CONCURRENCY: usize = 4;

/// Parsed command-line configuration.
#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    repos: Vec<(String, String)>,
    mode: Option<String>,
    limit: Option<usize>,
    out: String,
    help: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cli = match parse_cli(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("Error: {message}\n\n{}", usage());
            return ExitCode::from(EXIT_USAGE);
        }
    };

    if cli.help {
        println!("{}", usage());
        return ExitCode::from(EXIT_SUCCESS);
    }

    match run(cli).await {
        Ok(()) => ExitCode::from(EXIT_SUCCESS),
        Err(RunError::MissingToken(message)) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_MISSING_TOKEN)
        }
        Err(RunError::Usage(message)) => {
            eprintln!("Error: {message}\n\n{}", usage());
            ExitCode::from(EXIT_USAGE)
        }
        Err(RunError::Io(error)) => {
            eprintln!("Error: {error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Fatal errors from the main flow.
enum RunError {
    MissingToken(String),
    Usage(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for RunError {
    fn from(error: std::io::Error) -> Self {
        RunError::Io(error)
    }
}

async fn run(cli: Cli) -> Result<(), RunError> {
    // GitHub is mandatory — it is the only source of PR signals.
    let Some(github) = GithubClient::from_env() else {
        return Err(RunError::MissingToken(
            "GITHUB_TOKEN is not set (or is empty). Set it to a token with repo read access and \
             re-run. jules-pr-triage only reads GitHub — it never closes or merges PRs."
                .to_string(),
        ));
    };
    let github = Arc::new(github);

    // Jules is optional: it supplies session correlation and repo discovery.
    let jules = build_jules_client();
    if jules.is_none() {
        eprintln!(
            "warning: JULES_API_KEY is not set — skipping Jules-side session correlation; \
             reviewer findings will be unavailable and repos must be given with --repo."
        );
    }

    // Correlate Jules sessions to PRs when we have a Jules client.
    let (session_index, sources) = if let Some(client) = &jules {
        let sessions = fetch_all_sessions(client).await;
        let sources = fetch_all_sources(client).await;
        (SessionIndex::from_sessions(&sessions), sources)
    } else {
        (SessionIndex::default(), Vec::new())
    };
    let session_index = Arc::new(session_index);

    // Resolve the repo set: explicit --repo flags win; otherwise derive from
    // Jules sources.
    let repos = if cli.repos.is_empty() {
        derive_repos_from_sources(&sources)
    } else {
        cli.repos.clone()
    };
    if repos.is_empty() {
        return Err(RunError::Usage(
            "no repositories to triage. Pass --repo owner/repo (repeatable or comma-separated), \
             or set JULES_API_KEY so repos can be derived from Jules sources."
                .to_string(),
        ));
    }

    let mode = match cli.mode.as_deref() {
        Some(raw) => TriageMode::parse(raw)
            .ok_or_else(|| RunError::Usage(format!("invalid --mode `{raw}`")))?,
        None => TriageMode::default(),
    };

    // Optional LLM evaluator.
    let llm = AnthropicClient::from_env().map(|client| {
        eprintln!("info: LLM evaluator enabled (model {})", client.model());
        Arc::new(LlmEvaluator::new(client))
    });
    if llm.is_none() && matches!(mode, TriageMode::Llm | TriageMode::Both) {
        eprintln!(
            "info: ANTHROPIC_API_KEY not set — LLM evaluator unavailable; using heuristic only."
        );
    }

    let matcher = JulesMatcher::from_env();
    let heuristic = HeuristicEvaluator::new();

    // Triage repos in parallel, bounded.
    let mut results: Vec<TriageResult> = Vec::new();
    for chunk in repos.chunks(REPO_CONCURRENCY) {
        let mut set: JoinSet<Vec<TriageResult>> = JoinSet::new();
        for (owner, repo) in chunk {
            let github = Arc::clone(&github);
            let index = Arc::clone(&session_index);
            let matcher = matcher.clone();
            let llm = llm.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let limit = cli.limit;
            set.spawn(async move {
                triage_repo(
                    &github, &owner, &repo, &matcher, &index, heuristic, llm.as_deref(), mode,
                    limit,
                )
                .await
            });
        }
        while let Some(joined) = set.join_next().await {
            if let Ok(mut repo_results) = joined {
                results.append(&mut repo_results);
            }
        }
    }

    let report = render_report(&results);
    std::fs::write(&cli.out, &report)?;
    println!("{}", stdout_summary(&results, &cli.out));

    Ok(())
}

/// Triage every open Jules PR in one repository. Read-only: gathers signals and
/// evaluates, never mutates GitHub.
#[allow(clippy::too_many_arguments)]
async fn triage_repo(
    github: &GithubClient,
    owner: &str,
    repo: &str,
    matcher: &JulesMatcher,
    index: &SessionIndex,
    heuristic: HeuristicEvaluator,
    llm: Option<&LlmEvaluator>,
    mode: TriageMode,
    limit: Option<usize>,
) -> Vec<TriageResult> {
    let label = format!("{owner}/{repo}");
    let pulls = match github.list_open_pulls(owner, repo).await {
        Ok(pulls) => pulls,
        Err(error) => {
            eprintln!("warning: failed to list open pulls for {label}: {error}");
            return Vec::new();
        }
    };

    let mut jules_prs: Vec<Pr> = pulls.into_iter().filter(|pr| matcher.is_jules(pr)).collect();
    if let Some(cap) = limit {
        jules_prs.truncate(cap);
    }

    let reviewer = SessionReviewer::new();
    let mut results = Vec::new();
    for pr in jules_prs {
        let Some(result) =
            triage_pull(github, &label, index, &reviewer, heuristic, llm, mode, pr).await
        else {
            continue;
        };
        results.push(result);
    }
    results
}

/// Gather the signals for one PR and evaluate it. Returns `None` (with a
/// warning) when the detailed PR view cannot be fetched.
#[allow(clippy::too_many_arguments)]
async fn triage_pull(
    github: &GithubClient,
    repo_label: &str,
    index: &SessionIndex,
    reviewer: &SessionReviewer,
    heuristic: HeuristicEvaluator,
    llm: Option<&LlmEvaluator>,
    mode: TriageMode,
    pr: Pr,
) -> Option<TriageResult> {
    let (owner, repo) = repo_label.split_once('/')?;

    let detail = match github.get_pull(owner, repo, pr.number).await {
        Ok(detail) => detail,
        Err(error) => {
            eprintln!("warning: failed to fetch {repo_label}#{}: {error}", pr.number);
            return None;
        }
    };

    // Check runs on the head commit. A failure here degrades to an empty
    // summary (→ needs_human), rather than dropping the PR.
    let head_sha = detail.head_sha().to_string();
    let checks = match github.list_check_runs(owner, repo, &head_sha).await {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!(
                "warning: failed to list check runs for {repo_label}#{}: {error}",
                pr.number
            );
            CheckSummary::default()
        }
    };

    // Best-effort Jules correlation.
    let outputs = index.find(&pr);
    let session_matched = !outputs.is_empty();
    let review = reviewer.review_session(outputs);
    let mut changed_files: Vec<String> = outputs
        .iter()
        .flat_map(|output| output.changed_files.iter().map(|file| file.path.clone()))
        .collect();
    changed_files.sort_unstable();
    changed_files.dedup();

    let jules_task_id = pr.body.as_deref().and_then(task_id_from_body);

    let ctx = PrContext {
        repo: repo_label.to_string(),
        pr: pr.clone(),
        detail,
        checks: checks.clone(),
        findings: review.findings,
        changed_files,
        session_matched,
        jules_task_id,
    };

    let verdict = evaluate_pr(mode, &heuristic, llm, &ctx).await;

    Some(TriageResult {
        repo: repo_label.to_string(),
        number: pr.number,
        title: pr.title,
        url: pr.html_url,
        verdict,
        checks,
        mergeable_state: ctx.detail.mergeable_state,
        additions: ctx.detail.additions,
        deletions: ctx.detail.deletions,
        changed_files: ctx.detail.changed_files,
        findings: ctx.findings.len(),
    })
}

/// Build a Jules client from the environment, mirroring the other bins.
fn build_jules_client() -> Option<JulesClient> {
    let api_key = env::var("JULES_API_KEY").ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    let mut builder = JulesClient::builder(api_key).timeout_policy(TimeoutPolicy {
        request_timeout: Duration::from_secs(45),
    });
    if let Ok(api_url) = env::var("JULES_API_URL") {
        if !api_url.trim().is_empty() {
            builder = builder.base_url(api_url);
        }
    }
    builder.build().ok()
}

/// Paginate all sessions; on error, return what was gathered so far (empty).
async fn fetch_all_sessions(client: &JulesClient) -> Vec<Session> {
    let mut all = Vec::new();
    let mut page_token = None;
    loop {
        let response = client
            .list_sessions(ListSessionsParams {
                page_size: Some(100),
                page_token: page_token.clone(),
                filter: None,
            })
            .await;
        match response {
            Ok(page) => {
                all.extend(page.sessions);
                match page.next_page_token {
                    Some(token) if !token.is_empty() => page_token = Some(token),
                    _ => break,
                }
            }
            Err(error) => {
                eprintln!("warning: failed to list Jules sessions: {error}");
                break;
            }
        }
    }
    all
}

/// Paginate all sources; on error, return what was gathered so far.
async fn fetch_all_sources(client: &JulesClient) -> Vec<Source> {
    let mut all = Vec::new();
    let mut page_token = None;
    loop {
        let response = client
            .list_sources(ListSourcesParams {
                page_size: Some(100),
                page_token: page_token.clone(),
            })
            .await;
        match response {
            Ok(page) => {
                all.extend(page.sources);
                match page.next_page_token {
                    Some(token) if !token.is_empty() => page_token = Some(token),
                    _ => break,
                }
            }
            Err(error) => {
                eprintln!("warning: failed to list Jules sources: {error}");
                break;
            }
        }
    }
    all
}

/// Distinct `(owner, repo)` pairs derived from Jules GitHub sources.
fn derive_repos_from_sources(sources: &[Source]) -> Vec<(String, String)> {
    let mut set: BTreeSet<(String, String)> = BTreeSet::new();
    for source in sources {
        if let Some(github) = &source.github_repo {
            if !github.owner.is_empty() && !github.repo.is_empty() {
                set.insert((github.owner.clone(), github.repo.clone()));
            }
        }
    }
    set.into_iter().collect()
}

/// A concise stdout summary table plus the report location.
fn stdout_summary(results: &[TriageResult], out_path: &str) -> String {
    use std::fmt::Write as _;

    let mut merge = 0;
    let mut close = 0;
    let mut needs_human = 0;
    for result in results {
        match result.verdict.recommendation {
            Recommendation::Merge => merge += 1,
            Recommendation::Close => close += 1,
            Recommendation::NeedsHuman => needs_human += 1,
        }
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Triaged {} Jules PR(s): {merge} merge / {close} close / {needs_human} needs_human  (dry-run — nothing was closed or merged)",
        results.len()
    );
    if !results.is_empty() {
        let _ = writeln!(out, "{:<28} {:>6}  {:<12} {:>5}  SOURCE", "REPO", "PR", "REC", "CONF");
        for result in results {
            let _ = writeln!(
                out,
                "{:<28} {:>6}  {:<12} {:>5.2}  {}",
                truncate(&result.repo, 28),
                format!("#{}", result.number),
                result.verdict.recommendation.label(),
                result.verdict.confidence,
                result.verdict.source
            );
        }
    }
    let _ = write!(out, "Report written to {out_path}");
    out
}

/// Truncate a string to `max` characters for the summary table.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Parse a `owner/repo` string into its two parts, rejecting malformed input.
fn parse_repo(value: &str) -> Result<(String, String), String> {
    let trimmed = value.trim();
    let (owner, repo) = trimmed
        .split_once('/')
        .ok_or_else(|| format!("invalid --repo `{value}` (expected owner/repo)"))?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(format!("invalid --repo `{value}` (expected owner/repo)"));
    }
    Ok((owner.to_string(), repo.to_string()))
}

/// Manual arg parsing in the repo's existing style.
fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli {
        out: DEFAULT_OUT.to_string(),
        ..Cli::default()
    };
    let mut index = 0usize;
    while index < args.len() {
        let token = &args[index];
        match token.as_str() {
            "--help" | "-h" => {
                cli.help = true;
                index += 1;
            }
            "--repo" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --repo".to_string())?;
                for part in value.split(',') {
                    if part.trim().is_empty() {
                        continue;
                    }
                    cli.repos.push(parse_repo(part)?);
                }
                index += 2;
            }
            "--mode" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --mode".to_string())?;
                cli.mode = Some(value.clone());
                index += 2;
            }
            "--limit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --limit".to_string())?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit `{value}` (expected a positive integer)"))?;
                cli.limit = Some(parsed);
                index += 2;
            }
            "--out" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --out".to_string())?;
                cli.out.clone_from(value);
                index += 2;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(cli)
}

fn usage() -> String {
    "jules-pr-triage — dry-run merge/close/needs-human report for open Jules PRs

USAGE:
  jules-pr-triage [--repo owner/repo]... [--mode heuristic|llm|both] [--limit N] [--out <path>]

OPTIONS:
  --repo owner/repo   Repository to triage. Repeatable, and/or a comma-separated
                      list. When omitted, repos are derived from Jules sources
                      (requires JULES_API_KEY).
  --mode MODE         heuristic | llm | both   (default: both)
  --limit N           Cap the number of PRs triaged per repo.
  --out PATH          Report output path (default: jules-pr-triage-report.md).
  --help, -h          Show this help.

ENVIRONMENT:
  GITHUB_TOKEN        Required. Read access to the target repositories.
  JULES_API_KEY       Optional. Enables Jules session correlation + repo
                      discovery. Missing is a warning, not an error.
  ANTHROPIC_API_KEY   Optional. Enables the LLM evaluator.
  ANTHROPIC_MODEL     Optional. Overrides the default Claude model.

Dry-run only: this tool never closes or merges any pull request."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_defaults() {
        let cli = parse_cli(&[]).expect("empty args parse");
        assert_eq!(cli.out, DEFAULT_OUT);
        assert!(cli.repos.is_empty());
        assert!(cli.mode.is_none());
        assert!(cli.limit.is_none());
        assert!(!cli.help);
    }

    #[test]
    fn parse_cli_repeatable_and_comma_repos() {
        let args = [
            "--repo".to_string(),
            "acme/widgets".to_string(),
            "--repo".to_string(),
            "acme/gadgets,acme/sprockets".to_string(),
        ];
        let cli = parse_cli(&args).expect("repo args parse");
        assert_eq!(
            cli.repos,
            vec![
                ("acme".to_string(), "widgets".to_string()),
                ("acme".to_string(), "gadgets".to_string()),
                ("acme".to_string(), "sprockets".to_string()),
            ]
        );
    }

    #[test]
    fn parse_cli_rejects_bad_repo() {
        assert!(parse_cli(&["--repo".to_string(), "not-a-repo".to_string()]).is_err());
        assert!(parse_cli(&["--repo".to_string(), "a/b/c".to_string()]).is_err());
    }

    #[test]
    fn parse_cli_flags() {
        let args = [
            "--mode".to_string(),
            "heuristic".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--out".to_string(),
            "report.md".to_string(),
        ];
        let cli = parse_cli(&args).expect("flags parse");
        assert_eq!(cli.mode.as_deref(), Some("heuristic"));
        assert_eq!(cli.limit, Some(5));
        assert_eq!(cli.out, "report.md");
    }

    #[test]
    fn parse_cli_help_flag() {
        assert!(parse_cli(&["--help".to_string()]).expect("help parse").help);
        assert!(parse_cli(&["-h".to_string()]).expect("help parse").help);
    }

    #[test]
    fn parse_cli_unknown_arg_errors() {
        assert!(parse_cli(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn derive_repos_dedupes_github_sources() {
        // Build via JSON to avoid depending on every Source field.
        let source: Source = serde_json::from_str(
            r#"{"name":"sources/github/acme/widgets","githubRepo":{"owner":"acme","repo":"widgets"}}"#,
        )
        .expect("valid source");
        let repos = derive_repos_from_sources(std::slice::from_ref(&source));
        assert_eq!(repos, vec![("acme".to_string(), "widgets".to_string())]);
    }
}
