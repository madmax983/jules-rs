# Director Agent-Safe CLI Contract Plan

## Goal
Implement an agent-safe CLI contract for `src/bin/director.rs` so Claude/Codex/Gemini can invoke it reliably in CI and shared terminal workflows.

## Agreed Product Decisions
- `--json` emits exactly one final JSON object on `stdout`.
- Human/log output moves to `stderr` when `--json` is used.
- Every JSON response includes `schema_version`.
- Stable exit-code contract:
  - `0`: success/settled
  - `10`: blocked/needs_input
  - `11`: failed
  - `12`: lock/state conflict
  - `13`: usage/config error
  - `14`: transient API/system error
- Multi-agent safety uses lock file lease protocol with owner metadata.
- Lock acquisition waits with jitter + timeout, then exits with code `12` on timeout.

## Scope
### In scope
- JSON envelope for `init`, `status`, `tick`, `run`
- Exit code standardization
- Lock lease implementation (`<state>.lock`)
- CLI flags for lock behavior
- Unit tests + integration checks for contract behavior

### Out of scope (follow-up)
- NDJSON event streaming
- HTTP/gRPC wrapper around director
- multi-process stress harness beyond deterministic tests

## Contract Specification
### JSON envelope
```json
{
  "schema_version": 1,
  "command": "tick",
  "timestamp_unix": 1760000000,
  "state_path": ".jules-director-state.json",
  "result": "success",
  "code": 0,
  "message": "created session `sessions/s-123` for task `t-1`",
  "data": {}
}
```

### Result vocabulary
- `success`
- `blocked`
- `failed`
- `conflict`
- `usage_error`
- `transient_error`

### Lock file shape
```json
{
  "schema_version": 1,
  "lock_id": "uuid",
  "agent_id": "string",
  "pid": 12345,
  "hostname": "host",
  "command": "run",
  "created_at_unix": 1760000000,
  "lease_expires_unix": 1760000030
}
```

## Implementation Steps
1. Introduce CLI output mode
- Add `--json` flag for all commands.
- Add response model (`CliResponse<T>`) and typed payload structs per command.
- Route machine output to `stdout`, human logs to `stderr` when `--json`.

2. Normalize error mapping + exit codes
- Add top-level `run_cli() -> Result<..., DirectorError>` with final exit code mapping.
- Map usage/config errors to `13`.
- Map lock conflicts to `12`.
- Map transient API/timeouts to `14`.
- Map blocked terminal states to `10`.

3. Add lock lease subsystem
- Implement lock acquisition with atomic create/write.
- Add stale-lock detection and takeover by lease expiry.
- Add jittered wait loop until `--lock-timeout-ms`.
- Add lock renewal heartbeat for `run`.
- Add ownership checks before unlock.

4. Add new CLI flags and env wiring
- `--lock-timeout-ms` (default 30000)
- `--lock-lease-ms` (default 30000)
- `JULES_DIRECTOR_AGENT_ID` (override owner id)
- Keep defaults deterministic and documented in JSON output.

5. Update command payload builders
- `init`: include seeded task summary + policy snapshot.
- `status`: include compact task summaries + counts.
- `tick`: include `outcome_type`, `task_id`, optional `session/pr`.
- `run`: include cycles executed, terminal boolean, final counts.

6. Tests (TDD-first per slice)
- RED tests for JSON schema fields presence.
- RED tests for each exit code mapping.
- RED tests for lock held -> wait -> timeout -> code `12`.
- RED tests for stale lock takeover.
- RED tests ensuring only one JSON object is emitted.

7. Verification
- `cargo fmt --all`
- `cargo test --bin director`
- `cargo test`
- `cargo check --all-targets`

## Acceptance Criteria
- Agents can parse all command outputs with a stable JSON schema.
- Concurrent agents cannot corrupt state due to lock coordination.
- Exit codes are deterministic and documented.
- All new behavior is covered by tests and passes CI commands above.

## Director Execution Goal
Use this exact goal when invoking director:

> Implement the agent-safe CLI contract described in `docs/plans/2026-02-19-director-agent-safe-cli-contract-plan.md`, including JSON output contract, stable exit codes, lock lease protocol, and tests, then open/update PR with verification notes.

Suggested source:

> `sources/github/madmax983/jules-rs`
