# Jules Director

Director is an autonomous agent runtime that manages the lifecycle of product development tasks. It coordinates sessions, tracks state, and enforces quality gates.

## Usage

The `director` binary supports the following commands:

### `init`

Initialize a new director state file.

```bash
director init "<goal>" "<source>" [--state <path>] [--json]
```

- `<goal>`: The high-level product goal.
- `<source>`: The source repository context (e.g., `github/owner/repo`).

### `run`

Run the director loop until all tasks are settled (completed, failed, or blocked).

```bash
director run ["<goal>" "<source>"] [--max-cycles <n>] [--state <path>] [--json]
```

- If `goal` and `source` are provided, it ensures the state matches or initializes it if missing.
- `--max-cycles`: Maximum number of loop iterations (default: 600).

### `tick`

Execute a single iteration of the director loop.

```bash
director tick [--state <path>] [--json]
```

### `status`

Display the current status of the director state.

```bash
director status [--state <path>] [--json]
```

## Environment Variables

| Variable | Description |
|---|---|
| `JULES_API_KEY` | **Required**. API key for the Jules API. |
| `JULES_API_URL` | Optional. Override the base URL for the Jules API (useful for testing). |
| `JULES_DIRECTOR_STATE` | Path to the state file (default: `.jules-director-state.json`). |
| `JULES_DIRECTOR_AUTO_FEEDBACK_MESSAGE` | Message to send when auto-responding to user feedback requests. |
| `JULES_DIRECTOR_MAX_AUTO_FEEDBACK_MESSAGES` | Maximum number of auto-responses per task before escalating to user. |
| `JULES_DIRECTOR_MAX_POLLS_PER_TASK` | Maximum number of polls per task before blocking. |
| `JULES_DIRECTOR_QUALITY_GATES` | Semicolon-separated list of shell commands to run as quality gates before completing a task. |

## Exit Codes

The director binary uses stable exit codes to indicate the result of the operation:

- `0`: **Success**. The command completed successfully.
- `10`: **Usage Error**. Invalid arguments or configuration.
- `11`: **Transient Error**. Temporary failure (e.g., network timeout, 5xx API error). Retrying may succeed.
- `12`: **Permanent Error**. Use-case or configuration error (e.g., 4xx API error, invalid state). Retrying is unlikely to succeed.
- `13`: **Lock Error**. Failed to acquire the lock on the state file (another instance is running).
- `14`: **Internal Error**. Unexpected internal state or corruption.

## File Formats

### State File

The state file is a JSON document that persists the director's knowledge of the product goal, tasks, and their status. It includes a `version` field (e.g., `"version": 1`) to facilitate future migrations. It is safe to commit this file to version control if it does not contain sensitive secrets (it typically contains task descriptions and session references).

A lock file (`<state-path>.lock`) is created alongside the state file to prevent concurrent modifications.
