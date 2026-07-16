//! Pluggable PR-triage evaluators and report rendering.
//!
//! The triage agent gathers read-only signals about an open, Jules-authored
//! pull request — CI check outcomes, mergeability, diff size, and (when a Jules
//! session can be correlated to the PR) [`SessionReviewer`](crate::SessionReviewer)
//! findings — bundles them into a [`PrContext`], and asks one or more
//! [`Evaluator`]s for a [`Verdict`]: **merge**, **close**, or **needs human**.
//!
//! Everything here is advisory. This module never mutates GitHub — it only
//! evaluates and renders. Two evaluators are provided:
//!
//! * [`HeuristicEvaluator`] — pure, deterministic rules over the signals.
//! * [`LlmEvaluator`] — asks Claude to judge the PR (available only when an
//!   [`AnthropicClient`](crate::AnthropicClient) is configured).
//!
//! [`TriageMode`] selects which run; in [`TriageMode::Both`] the two verdicts
//! are reconciled, preferring `needs_human` whenever they disagree.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::anthropic::{AnthropicClient, RawJudgment, parse_judgment};
use crate::github::{CheckSummary, Pr, PrDetail};
use crate::reviewer::{FindingSeverity, ReviewFinding};
use crate::{Session, SessionOutput};

// ── Heuristic thresholds (documented, deterministic) ─────────────

/// A diff is "small" when it touches at most this many files.
const SMALL_DIFF_FILES: u64 = 10;

/// …and adds at most this many lines.
const SMALL_DIFF_ADDITIONS: u64 = 200;

/// A diff is "very large" (human review territory) at or above this file count.
const LARGE_DIFF_FILES: u64 = 40;

/// …or this many total changed lines (additions + deletions).
const LARGE_DIFF_LINES: u64 = 1000;

// ── Core types ───────────────────────────────────────────────────

/// The triage recommendation for a single PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recommendation {
    /// Signals point to a clean, safe merge.
    Merge,
    /// Signals point to closing (CI failing, conflicts, …).
    Close,
    /// Signals are inconclusive or high-risk; a human should decide.
    NeedsHuman,
}

impl Recommendation {
    /// Sort weight: `close` (0) first, then `needs_human` (1), then `merge` (2).
    /// Used to order the per-PR rows within a repo so the most actionable
    /// negatives surface at the top.
    #[must_use]
    pub fn sort_key(self) -> u8 {
        match self {
            Recommendation::Close => 0,
            Recommendation::NeedsHuman => 1,
            Recommendation::Merge => 2,
        }
    }

    /// Stable lowercase label for display.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Recommendation::Merge => "merge",
            Recommendation::Close => "close",
            Recommendation::NeedsHuman => "needs_human",
        }
    }
}

/// A single evaluator's verdict about a PR.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// The recommended action.
    pub recommendation: Recommendation,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Human-readable justification, naming the signals that drove it.
    pub rationale: String,
    /// Which evaluator produced this: `"heuristic"`, `"llm"`, or
    /// `"heuristic+llm"` for a reconciled verdict.
    pub source: &'static str,
}

/// All gathered signals for one PR, handed to the evaluators.
pub struct PrContext {
    /// `owner/repo` label.
    pub repo: String,
    /// The PR as returned by the pulls-list endpoint (number/title/body/…).
    pub pr: Pr,
    /// The detailed single-PR view (mergeability + diffstat).
    pub detail: PrDetail,
    /// Summary of the head commit's check runs.
    pub checks: CheckSummary,
    /// Reviewer findings from a correlated Jules session, if any.
    pub findings: Vec<ReviewFinding>,
    /// Paths changed by the correlated Jules session(s), for the LLM prompt.
    pub changed_files: Vec<String>,
    /// Whether a Jules session was correlated to this PR at all.
    pub session_matched: bool,
    /// Jules task id parsed from the PR body, when present.
    pub jules_task_id: Option<String>,
}

impl PrContext {
    /// Count of reviewer findings at [`FindingSeverity::Error`] (high severity).
    #[must_use]
    pub fn high_severity_findings(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .count()
    }
}

// ── Evaluator trait (async, static dispatch — no `dyn`) ──────────

/// A pluggable PR evaluator. Native `async fn` in traits (edition 2024) lets us
/// use static dispatch for both the pure heuristic and the network-backed LLM
/// evaluator without boxing.
// `async_fn_in_trait` warns that callers can't add `Send` bounds; triage only
// ever awaits these on one task, so the warning does not apply.
#[allow(async_fn_in_trait)]
pub trait Evaluator {
    /// Evaluate `ctx` and return a [`Verdict`].
    async fn evaluate(&self, ctx: &PrContext) -> Verdict;
}

// ── Heuristic evaluator ──────────────────────────────────────────

/// Pure, deterministic evaluator over the gathered signals. The rules, in
/// priority order:
///
/// 1. High-severity reviewer finding, or a very large diff → **`needs_human`**.
/// 2. CI has a failing check, or `mergeable_state` is dirty/conflicting →
///    **close** (lower confidence when a second signal disagrees).
/// 3. Checks still running, or mergeability unknown → **`needs_human`**.
/// 4. All checks green, mergeable & clean, and a small diff → **merge**.
/// 5. Otherwise → **`needs_human`** (inconclusive).
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicEvaluator;

impl HeuristicEvaluator {
    /// Build a new heuristic evaluator.
    #[must_use]
    pub fn new() -> Self {
        HeuristicEvaluator
    }

    /// Pure assessment, factored out of the trait method so it can be unit
    /// tested directly.
    #[must_use]
    pub fn assess(ctx: &PrContext) -> Verdict {
        let detail = &ctx.detail;
        let checks = &ctx.checks;

        let state = detail.mergeable_state.to_ascii_lowercase();
        let conflicting = matches!(state.as_str(), "dirty" | "conflicting");
        let total_lines = detail.additions.saturating_add(detail.deletions);
        let very_large =
            detail.changed_files >= LARGE_DIFF_FILES || total_lines >= LARGE_DIFF_LINES;
        let small_diff =
            detail.changed_files <= SMALL_DIFF_FILES && detail.additions <= SMALL_DIFF_ADDITIONS;
        let mergeable_clean = detail.mergeable == Some(true) && state == "clean";
        let running_or_unknown =
            checks.is_running() || detail.mergeable.is_none() || state.is_empty() || state == "unknown";
        let high_severity = ctx.high_severity_findings();

        // 1. High-severity findings or an oversized diff need a human.
        if high_severity > 0 {
            return Verdict {
                recommendation: Recommendation::NeedsHuman,
                confidence: 0.7,
                rationale: format!(
                    "{high_severity} high-severity reviewer finding(s) — needs human review"
                ),
                source: "heuristic",
            };
        }
        if very_large {
            return Verdict {
                recommendation: Recommendation::NeedsHuman,
                confidence: 0.65,
                rationale: format!(
                    "large diff ({} files, +{}/-{}) — too big to auto-triage",
                    detail.changed_files, detail.additions, detail.deletions
                ),
                source: "heuristic",
            };
        }

        // 2. Clear negatives: failing CI or merge conflicts → lean close.
        if checks.has_failure() || conflicting {
            let mut reasons: Vec<String> = Vec::new();
            if checks.has_failure() {
                reasons.push(format!("{} failing check(s)", checks.failure));
            }
            if conflicting {
                reasons.push(format!("mergeable_state={state}"));
            }
            // Lower confidence when a positive signal partly disagrees.
            let conflict_signal = mergeable_clean || checks.all_success();
            let confidence = if checks.has_failure() && conflicting {
                0.85
            } else if conflict_signal {
                0.5
            } else {
                0.65
            };
            return Verdict {
                recommendation: Recommendation::Close,
                confidence,
                rationale: format!("lean close: {}", reasons.join(", ")),
                source: "heuristic",
            };
        }

        // 3. Not enough information yet.
        if running_or_unknown {
            return Verdict {
                recommendation: Recommendation::NeedsHuman,
                confidence: 0.5,
                rationale: format!(
                    "checks/mergeability not settled (running={}, mergeable_state={})",
                    checks.running,
                    if state.is_empty() { "unknown" } else { &state }
                ),
                source: "heuristic",
            };
        }

        // 4. Clean positive: green CI, mergeable, small diff.
        if checks.all_success() && mergeable_clean && small_diff {
            let confidence = if ctx.findings.is_empty() { 0.8 } else { 0.7 };
            return Verdict {
                recommendation: Recommendation::Merge,
                confidence,
                rationale: format!(
                    "lean merge: {} checks green, mergeable, small diff ({} files, +{})",
                    checks.total, detail.changed_files, detail.additions
                ),
                source: "heuristic",
            };
        }

        // 5. Inconclusive.
        Verdict {
            recommendation: Recommendation::NeedsHuman,
            confidence: 0.4,
            rationale: "signals inconclusive (no clear merge or close case)".to_string(),
            source: "heuristic",
        }
    }
}

impl Evaluator for HeuristicEvaluator {
    async fn evaluate(&self, ctx: &PrContext) -> Verdict {
        HeuristicEvaluator::assess(ctx)
    }
}

// ── LLM evaluator ────────────────────────────────────────────────

/// Evaluator backed by the Anthropic Messages API. Builds a compact prompt from
/// the [`PrContext`] (title/body/diffstat/CI/findings — never the full diff) and
/// asks the model for a strict-JSON verdict.
pub struct LlmEvaluator {
    client: AnthropicClient,
}

impl LlmEvaluator {
    /// Wrap an [`AnthropicClient`].
    #[must_use]
    pub fn new(client: AnthropicClient) -> Self {
        LlmEvaluator { client }
    }

    /// The model this evaluator will query.
    #[must_use]
    pub fn model(&self) -> &str {
        self.client.model()
    }
}

/// Map a raw recommendation label to the typed enum. Anything unrecognized is
/// conservative (`needs_human`).
fn recommendation_from_label(label: &str) -> Recommendation {
    match label {
        "merge" => Recommendation::Merge,
        "close" => Recommendation::Close,
        _ => Recommendation::NeedsHuman,
    }
}

/// Turn a parsed [`RawJudgment`] into a `source = "llm"` [`Verdict`].
fn verdict_from_judgment(judgment: RawJudgment) -> Verdict {
    Verdict {
        recommendation: recommendation_from_label(&judgment.recommendation),
        confidence: judgment.confidence,
        rationale: judgment.rationale,
        source: "llm",
    }
}

/// Build the LLM prompt from a context. Bounds token use by truncating the body
/// and the changed-file list rather than including the full diff.
#[must_use]
pub fn build_prompt(ctx: &PrContext) -> String {
    const BODY_MAX: usize = 1200;
    const FILES_MAX: usize = 30;

    let body = ctx.pr.body.as_deref().unwrap_or("");
    let body: String = body.chars().take(BODY_MAX).collect();

    let files: Vec<&String> = ctx.changed_files.iter().take(FILES_MAX).collect();
    let file_list = if files.is_empty() {
        "(no correlated file list)".to_string()
    } else {
        let extra = ctx.changed_files.len().saturating_sub(files.len());
        let mut listed = files
            .iter()
            .map(|path| format!("- {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        if extra > 0 {
            let _ = write!(listed, "\n- …and {extra} more");
        }
        listed
    };

    let findings = if ctx.findings.is_empty() {
        "(none)".to_string()
    } else {
        ctx.findings
            .iter()
            .take(15)
            .map(|f| format!("- [{}] {}: {}", f.severity, f.file_path, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mergeable = ctx
        .detail
        .mergeable
        .map_or_else(|| "unknown".to_string(), |m| m.to_string());

    format!(
        "You are triaging an open pull request authored by the Jules coding agent. \
Decide whether it should be merged, closed, or escalated to a human. \
Consider CI results, mergeability, diff size, and code-review findings. \
Be conservative: prefer \"needs_human\" when signals conflict or information is missing.\n\n\
Respond with STRICT JSON and nothing else, of exactly this shape:\n\
{{\"recommendation\": \"merge|close|needs_human\", \"confidence\": 0.0-1.0, \"rationale\": \"one sentence\"}}\n\n\
Repository: {repo}\n\
PR #{number}: {title}\n\
Mergeable: {mergeable} (mergeable_state={state})\n\
Diffstat: {files_changed} files, +{additions}/-{deletions}\n\
CI checks: total={ci_total}, passing={ci_pass}, failing={ci_fail}, neutral={ci_neutral}, running={ci_running}\n\
Reviewer findings:\n{findings}\n\
Changed files:\n{file_list}\n\n\
PR body (truncated):\n{body}\n",
        repo = ctx.repo,
        number = ctx.pr.number,
        title = ctx.pr.title,
        state = ctx.detail.mergeable_state,
        files_changed = ctx.detail.changed_files,
        additions = ctx.detail.additions,
        deletions = ctx.detail.deletions,
        ci_total = ctx.checks.total,
        ci_pass = ctx.checks.success,
        ci_fail = ctx.checks.failure,
        ci_neutral = ctx.checks.neutral,
        ci_running = ctx.checks.running,
    )
}

impl Evaluator for LlmEvaluator {
    async fn evaluate(&self, ctx: &PrContext) -> Verdict {
        let prompt = build_prompt(ctx);
        match self.client.judge(&prompt).await {
            Ok(text) => verdict_from_judgment(parse_judgment(&text)),
            Err(error) => Verdict {
                recommendation: Recommendation::NeedsHuman,
                confidence: 0.0,
                rationale: format!("LLM evaluation unavailable: {error}"),
                source: "llm",
            },
        }
    }
}

// ── Mode + reconciliation ────────────────────────────────────────

/// Which evaluator(s) to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriageMode {
    /// Heuristic rules only.
    Heuristic,
    /// LLM only (falls back to heuristic when no client is configured).
    Llm,
    /// Run both and reconcile; the default.
    #[default]
    Both,
}

impl TriageMode {
    /// Parse a `--mode` flag value.
    #[must_use]
    pub fn parse(value: &str) -> Option<TriageMode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "heuristic" => Some(TriageMode::Heuristic),
            "llm" => Some(TriageMode::Llm),
            "both" => Some(TriageMode::Both),
            _ => None,
        }
    }
}

/// Reconcile a heuristic verdict with an LLM verdict. On agreement, the merged
/// verdict keeps the recommendation and the higher confidence and folds both
/// rationales; on disagreement it downgrades to `needs_human` and surfaces both
/// rationales so a human sees the split.
#[must_use]
pub fn reconcile(heuristic: &Verdict, llm: &Verdict) -> Verdict {
    if heuristic.recommendation == llm.recommendation {
        Verdict {
            recommendation: heuristic.recommendation,
            confidence: heuristic.confidence.max(llm.confidence),
            rationale: format!(
                "heuristic: {} | llm: {}",
                heuristic.rationale, llm.rationale
            ),
            source: "heuristic+llm",
        }
    } else {
        Verdict {
            recommendation: Recommendation::NeedsHuman,
            // Disagreement is itself low-confidence; average the two.
            confidence: f64::midpoint(heuristic.confidence, llm.confidence),
            rationale: format!(
                "evaluators disagree — heuristic={} ({}), llm={} ({})",
                heuristic.recommendation.label(),
                heuristic.rationale,
                llm.recommendation.label(),
                llm.rationale
            ),
            source: "heuristic+llm",
        }
    }
}

/// Evaluate one PR under `mode`, using the heuristic evaluator and an optional
/// LLM evaluator. In [`TriageMode::Llm`] with no LLM available, falls back to
/// the heuristic; in [`TriageMode::Both`] with no LLM available, returns the
/// heuristic verdict alone.
pub async fn evaluate_pr(
    mode: TriageMode,
    heuristic: &HeuristicEvaluator,
    llm: Option<&LlmEvaluator>,
    ctx: &PrContext,
) -> Verdict {
    match mode {
        TriageMode::Heuristic => heuristic.evaluate(ctx).await,
        TriageMode::Llm => match llm {
            Some(evaluator) => evaluator.evaluate(ctx).await,
            None => heuristic.evaluate(ctx).await,
        },
        TriageMode::Both => {
            let heuristic_verdict = heuristic.evaluate(ctx).await;
            match llm {
                Some(evaluator) => {
                    let llm_verdict = evaluator.evaluate(ctx).await;
                    reconcile(&heuristic_verdict, &llm_verdict)
                }
                None => heuristic_verdict,
            }
        }
    }
}

// ── Jules-side correlation (best-effort) ─────────────────────────

/// Extract a Jules task id from a PR body, looking for the
/// `jules.google.com/task/<id>` permalink. The id is taken up to the first
/// non-alphanumeric/`-`/`_` character. Case-insensitive on the host.
#[must_use]
pub fn task_id_from_body(body: &str) -> Option<String> {
    const MARKER: &str = "jules.google.com/task/";
    let lower = body.to_ascii_lowercase();
    let start = lower.find(MARKER)? + MARKER.len();
    let tail = &body[start..];
    let id: String = tail
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Index of Jules session outputs, keyed by the pull request they produced, so
/// a triaged PR can be correlated back to its originating session's outputs.
#[derive(Debug, Default)]
pub struct SessionIndex {
    by_url: HashMap<String, Vec<SessionOutput>>,
    by_number: HashMap<i64, Vec<SessionOutput>>,
}

impl SessionIndex {
    /// Build an index from every output attached to every session (both the
    /// terminal `output` and any intermediate `outputs`).
    #[must_use]
    pub fn from_sessions(sessions: &[Session]) -> SessionIndex {
        let mut index = SessionIndex::default();
        for session in sessions {
            if let Some(output) = &session.output {
                index.insert(output.clone());
            }
            for output in &session.outputs {
                index.insert(output.clone());
            }
        }
        index
    }

    fn insert(&mut self, output: SessionOutput) {
        let Some(pull_request) = output.pull_request.clone() else {
            return;
        };
        if let Some(number) = pull_request.number {
            self.by_number.entry(number).or_default().push(output.clone());
        }
        if !pull_request.url.is_empty() {
            self.by_url.entry(pull_request.url).or_default().push(output);
        }
    }

    /// Find the outputs correlated to `pr`, matching by PR html URL first, then
    /// by PR number. Returns an empty slice when there is no match.
    #[must_use]
    pub fn find(&self, pr: &Pr) -> &[SessionOutput] {
        if let Some(outputs) = self.by_url.get(&pr.html_url) {
            return outputs;
        }
        if let Ok(number) = i64::try_from(pr.number) {
            if let Some(outputs) = self.by_number.get(&number) {
                return outputs;
            }
        }
        &[]
    }
}

// ── Report rendering ─────────────────────────────────────────────

/// One finished triage result, the unit of the rendered report.
pub struct TriageResult {
    /// `owner/repo` label.
    pub repo: String,
    /// PR number.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// PR web URL.
    pub url: String,
    /// The final verdict for this PR.
    pub verdict: Verdict,
    /// Check-run summary (for the compact signals line).
    pub checks: CheckSummary,
    /// GitHub `mergeable_state`.
    pub mergeable_state: String,
    /// Lines added across the PR.
    pub additions: u64,
    /// Lines deleted across the PR.
    pub deletions: u64,
    /// Files changed by the PR.
    pub changed_files: u64,
    /// Count of correlated reviewer findings.
    pub findings: usize,
}

/// Totals over a set of results: `(merge, close, needs_human)` counts.
#[must_use]
pub fn tally(results: &[TriageResult]) -> (usize, usize, usize) {
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
    (merge, close, needs_human)
}

/// Render a Markdown triage report. Pure and unit-testable.
///
/// Structure: a header with totals, then one section per repo (repos sorted by
/// label), and within each repo the PRs sorted by recommendation
/// (close → `needs_human` → merge) then by descending confidence. **This report
/// is advisory only — no PR is ever closed or merged.**
#[must_use]
pub fn render_report(results: &[TriageResult]) -> String {
    let (merge, close, needs_human) = tally(results);

    let mut repos: Vec<&str> = results
        .iter()
        .map(|result| result.repo.as_str())
        .collect();
    repos.sort_unstable();
    repos.dedup();

    let mut out = String::new();
    let _ = writeln!(out, "# Jules PR triage report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "**Dry-run only — no PR is closed or merged by this report.**"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} open Jules PR(s) across {} repo(s): {} merge / {} close / {} needs_human",
        results.len(),
        repos.len(),
        merge,
        close,
        needs_human
    );

    for repo in repos {
        let mut rows: Vec<&TriageResult> = results
            .iter()
            .filter(|result| result.repo == repo)
            .collect();
        rows.sort_by(|a, b| {
            a.verdict
                .recommendation
                .sort_key()
                .cmp(&b.verdict.recommendation.sort_key())
                .then_with(|| {
                    b.verdict
                        .confidence
                        .partial_cmp(&a.verdict.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.number.cmp(&b.number))
        });

        let _ = writeln!(out);
        let _ = writeln!(out, "## {repo} ({} PR(s))", rows.len());
        for row in rows {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "- [#{} {}]({})",
                row.number,
                row.title.trim(),
                row.url
            );
            let _ = writeln!(
                out,
                "  - **{}** · confidence {:.2} · source: {}",
                row.verdict.recommendation.label(),
                row.verdict.confidence,
                row.verdict.source
            );
            let _ = writeln!(out, "  - {}", row.verdict.rationale.trim());
            let _ = writeln!(
                out,
                "  - signals: CI pass {} / fail {} / running {} · mergeable_state={} · +{}/-{} across {} files · {} reviewer finding(s)",
                row.checks.success,
                row.checks.failure,
                row.checks.running,
                if row.mergeable_state.is_empty() {
                    "unknown"
                } else {
                    &row.mergeable_state
                },
                row.additions,
                row.deletions,
                row.changed_files,
                row.findings
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PullRequest;

    fn pr(number: u64, body: Option<&str>) -> Pr {
        Pr {
            number,
            title: format!("PR {number}"),
            body: body.map(ToString::to_string),
            html_url: format!("https://github.com/acme/widgets/pull/{number}"),
            created_at: "2026-07-10T08:00:00Z".to_string(),
            author_login: "google-labs-jules[bot]".to_string(),
            head_ref: "nova/x".to_string(),
            head_sha: "abc".to_string(),
        }
    }

    fn detail(mergeable: Option<bool>, state: &str, files: u64, add: u64, del: u64) -> PrDetail {
        // Round-trip through JSON so the private head field is populated too.
        let json = format!(
            r#"{{"number":1,"mergeable":{},"mergeable_state":"{}","additions":{},"deletions":{},"changed_files":{},"head":{{"ref":"x","sha":"s"}}}}"#,
            mergeable.map_or_else(|| "null".to_string(), |m| m.to_string()),
            state,
            add,
            del,
            files
        );
        serde_json::from_str(&json).expect("valid detail")
    }

    fn checks(total: usize, success: usize, failure: usize, neutral: usize, running: usize) -> CheckSummary {
        CheckSummary {
            total,
            success,
            failure,
            neutral,
            running,
        }
    }

    fn ctx(
        detail: PrDetail,
        checks: CheckSummary,
        findings: Vec<ReviewFinding>,
    ) -> PrContext {
        PrContext {
            repo: "acme/widgets".to_string(),
            pr: pr(1, None),
            detail,
            checks,
            findings,
            changed_files: Vec::new(),
            session_matched: false,
            jules_task_id: None,
        }
    }

    fn error_finding() -> ReviewFinding {
        ReviewFinding {
            file_path: "src/lib.rs".to_string(),
            line_number: Some(3),
            severity: FindingSeverity::Error,
            message: "boom".to_string(),
        }
    }

    #[test]
    fn heuristic_leans_merge_on_clean_green_small_diff() {
        let context = ctx(
            detail(Some(true), "clean", 2, 30, 5),
            checks(3, 3, 0, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::Merge);
        assert!(verdict.confidence >= 0.7);
    }

    #[test]
    fn heuristic_leans_close_on_failing_ci() {
        let context = ctx(
            detail(Some(true), "clean", 2, 30, 5),
            checks(3, 2, 1, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::Close);
        assert!(verdict.rationale.contains("failing"));
    }

    #[test]
    fn heuristic_leans_close_on_dirty_mergeable_state() {
        let context = ctx(
            detail(Some(false), "dirty", 2, 30, 5),
            checks(3, 3, 0, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::Close);
    }

    #[test]
    fn heuristic_lowers_confidence_on_conflicting_signals() {
        // CI failing but the branch is otherwise clean/mergeable → conflict.
        let context = ctx(
            detail(Some(true), "clean", 2, 30, 5),
            checks(3, 2, 1, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::Close);
        assert!(verdict.confidence <= 0.5);
    }

    #[test]
    fn heuristic_needs_human_on_high_severity_finding() {
        let context = ctx(
            detail(Some(true), "clean", 2, 30, 5),
            checks(3, 3, 0, 0, 0),
            vec![error_finding()],
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::NeedsHuman);
    }

    #[test]
    fn heuristic_needs_human_on_large_diff() {
        let context = ctx(
            detail(Some(true), "clean", 60, 2000, 100),
            checks(3, 3, 0, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::NeedsHuman);
        assert!(verdict.rationale.contains("large diff"));
    }

    #[test]
    fn heuristic_needs_human_when_checks_running() {
        let context = ctx(
            detail(Some(true), "clean", 2, 30, 5),
            checks(3, 1, 0, 0, 2),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::NeedsHuman);
    }

    #[test]
    fn heuristic_needs_human_when_mergeable_unknown() {
        let context = ctx(
            detail(None, "unknown", 2, 30, 5),
            checks(3, 3, 0, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::NeedsHuman);
    }

    #[test]
    fn heuristic_needs_human_when_inconclusive() {
        // Green + mergeable but a large-ish (non-small) diff below the very-large
        // threshold: none of the clear branches fire.
        let context = ctx(
            detail(Some(true), "clean", 20, 500, 100),
            checks(3, 3, 0, 0, 0),
            Vec::new(),
        );
        let verdict = HeuristicEvaluator::assess(&context);
        assert_eq!(verdict.recommendation, Recommendation::NeedsHuman);
    }

    #[test]
    fn reconcile_agreement_keeps_recommendation_and_max_confidence() {
        let h = Verdict {
            recommendation: Recommendation::Merge,
            confidence: 0.7,
            rationale: "clean".to_string(),
            source: "heuristic",
        };
        let l = Verdict {
            recommendation: Recommendation::Merge,
            confidence: 0.9,
            rationale: "looks good".to_string(),
            source: "llm",
        };
        let merged = reconcile(&h, &l);
        assert_eq!(merged.recommendation, Recommendation::Merge);
        assert!((merged.confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(merged.source, "heuristic+llm");
        assert!(merged.rationale.contains("heuristic:"));
        assert!(merged.rationale.contains("llm:"));
    }

    #[test]
    fn reconcile_disagreement_prefers_needs_human() {
        let h = Verdict {
            recommendation: Recommendation::Merge,
            confidence: 0.8,
            rationale: "clean".to_string(),
            source: "heuristic",
        };
        let l = Verdict {
            recommendation: Recommendation::Close,
            confidence: 0.6,
            rationale: "risky".to_string(),
            source: "llm",
        };
        let merged = reconcile(&h, &l);
        assert_eq!(merged.recommendation, Recommendation::NeedsHuman);
        assert!(merged.rationale.contains("disagree"));
    }

    #[test]
    fn task_id_extraction() {
        assert_eq!(
            task_id_from_body("see https://jules.google.com/task/12345 for details"),
            Some("12345".to_string())
        );
        assert_eq!(
            task_id_from_body("HTTPS://JULES.GOOGLE.COM/task/ab-9_x)"),
            Some("ab-9_x".to_string())
        );
        assert_eq!(task_id_from_body("no task link here"), None);
    }

    #[test]
    fn session_index_correlates_by_number_and_url() {
        let output = SessionOutput {
            changed_files: Vec::new(),
            commit_hash: None,
            pull_request: Some(PullRequest {
                url: "https://github.com/acme/widgets/pull/7".to_string(),
                title: None,
                number: Some(7),
            }),
        };
        let session = Session {
            name: "sessions/s-1".to_string(),
            id: None,
            prompt: None,
            title: None,
            description: None,
            state: None,
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: None,
            output: Some(output),
            outputs: Vec::new(),
        };
        let index = SessionIndex::from_sessions(std::slice::from_ref(&session));

        // Match by URL.
        assert_eq!(index.find(&pr(7, None)).len(), 1);
        // A different PR number with no matching URL does not correlate.
        assert_eq!(index.find(&pr(9, None)).len(), 0);
    }

    #[test]
    fn render_report_orders_close_first_and_shows_totals() {
        let make = |number: u64, rec: Recommendation, conf: f64| TriageResult {
            repo: "acme/widgets".to_string(),
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/acme/widgets/pull/{number}"),
            verdict: Verdict {
                recommendation: rec,
                confidence: conf,
                rationale: "because".to_string(),
                source: "heuristic",
            },
            checks: checks(2, 2, 0, 0, 0),
            mergeable_state: "clean".to_string(),
            additions: 10,
            deletions: 2,
            changed_files: 1,
            findings: 0,
        };
        let results = vec![
            make(1, Recommendation::Merge, 0.8),
            make(2, Recommendation::Close, 0.9),
            make(3, Recommendation::NeedsHuman, 0.5),
        ];
        let report = render_report(&results);

        assert!(report.contains("3 open Jules PR(s) across 1 repo(s): 1 merge / 1 close / 1 needs_human"));
        assert!(report.contains("Dry-run only"));
        // Close must appear before merge in the rendered order.
        let close_pos = report.find("**close**").expect("close present");
        let merge_pos = report.find("**merge**").expect("merge present");
        let needs_pos = report.find("**needs_human**").expect("needs_human present");
        assert!(close_pos < needs_pos);
        assert!(needs_pos < merge_pos);
    }

    #[test]
    fn build_prompt_includes_key_signals() {
        let mut context = ctx(
            detail(Some(false), "dirty", 3, 40, 7),
            checks(4, 2, 1, 0, 1),
            vec![error_finding()],
        );
        context.changed_files = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let prompt = build_prompt(&context);
        assert!(prompt.contains("mergeable_state=dirty"));
        assert!(prompt.contains("failing=1"));
        assert!(prompt.contains("src/a.rs"));
        assert!(prompt.contains("STRICT JSON"));
    }
}
