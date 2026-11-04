# Task t139: Record the Herdr lead origin and task instance; strip Herdr hints from workers

size: medium

## Goal
Let a native dashboard group real tasks under the Pi/Claude conversation that created
them (decision 038). `delegate new` records the launching lead's conversation identity
from the pane's own Herdr API and gives each task a fresh instance id. Worker processes
stop inheriting Herdr hints, so a worker's hooks can't overwrite the lead pane's identity.

## Context (read these, nothing more)
- `AGENTS.md`
- `docs/decisions/009-views-scoped-to-session.md`, `031-any-model-can-lead.md`,
  `037-pi-claude-native-observer-first.md`, `038-delegate-records-herdr-lead-origin.md`
- `prototypes/herdr-observer/README.md`, sections "Attribution transaction and
  authority" (the Origin rules) and "Pinned public transport and fixture evidence"
- `prototypes/herdr-observer/fixtures/contract.json`, `fixtures/panes.json`
- `delegate/src/commands.rs` (`claim_owner`, `new`, the process spawn near `Command::new(program)`)
- `delegate/src/task.rs` (`State`, `Owner`), `delegate/src/status.rs` (`TaskView`, `collect`)
- `delegate/src/harness.rs` (worker/check spawns that set `DELEGATE_SANDBOXED`)

## Scope
- In: `delegate/src/origin.rs` (new), `delegate/src/main.rs` (module line only),
  `delegate/src/commands.rs`, `delegate/src/task.rs`, `delegate/src/status.rs`,
  `delegate/src/harness.rs`, and their unit tests
- Out: `prototypes/**`, `evals/**`, `.claude/**`, `.agents/**`, `delegate/templates/**`,
  `statusline.rs`, `watch.rs`, the pane mod, `Owner` and `session`/`session_name`
  semantics, `workers.toml` handling, dependencies

## Contract
### Task state (`STATE.json`), additive and optional
```rust
pub struct State { /* existing fields */
    #[serde(default, skip_serializing_if = "Option::is_none")] pub origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub instance: Option<String>,
}
#[serde(tag = "support", rename_all = "snake_case")]
pub enum Origin {
    Supported { source: String, agent: String, kind: RefKind, value: String },
    Unsupported { source: String, agent: String },
}
#[serde(rename_all = "snake_case")] pub enum RefKind { Id, Path }
```
A STATE.json written before this task (no fields) still loads. A loaded origin that
fails validation (below) is treated as `None`, never repaired.

### `delegate new <id>`
1. Unchanged: create the spec, `claim_owner`.
2. Set `instance` to a fresh `uuid::Uuid::new_v4()` string. Always, Herdr or not.
3. Capture origin only when **all** hold, else leave `origin` absent:
   `DELEGATE_SANDBOXED` unset; `HERDR_SOCKET_PATH` is an absolute path to a UNIX socket;
   `HERDR_PANE_ID` is nonempty and free of Unicode category C characters.
4. Request, on one `UnixStream` with a 3-second total deadline:
   `{"id":"<uuid>","method":"pane.get","params":{"pane_id":"<HERDR_PANE_ID>"}}\n`.
   Read one newline-terminated line of at most 2 MiB. Accept only
   `{"id":<same>,"result":{"type":"pane_info","pane":{"pane_id":<same pane>,...}}}`.
   An `error` envelope, mismatched id or pane, timeout or oversize records nothing.
5. From `pane.agent_session` (absent or null records nothing):
   - `source`, `agent` and `value` are strings and `kind` is `"id"` or `"path"`. Every
     string is nonempty, ≤4,096 chars, with no Unicode category C, Zl or Zp characters.
     A `path` value must be absolute. If any check fails, record nothing (no sanitizing).
   - (`herdr:pi`,`pi`) or (`herdr:claude`,`claude`) → `Supported`.
   - (`herdr:codex`,`codex`) → `Unsupported` (the value is not stored).
   - Any other pair → nothing.
   - If `CLAUDE_CODE_SESSION_ID` is set and nonempty, the result must be `Supported` with
     (`herdr:claude`,`claude`,`id`) and value equal to it. Otherwise record nothing.
6. When the Herdr variables are present but nothing was recorded, print one stderr line:
   `note: no lead origin recorded for <id> (<short reason>); the task runs normally`.
   `new` still exits 0. The note never includes the identity value or the raw response.
7. Origin and instance are written in the same save as the owner. Nothing else ever
   sets, changes or clears them (run, retry, check, review, merge don't touch them).

### `delegate status [id] --json`
Every task object, at every depth, gains two always-present keys:
- `origin`: `null`, `{"support":"supported","source","agent","kind","value"}` or
  `{"support":"unsupported","source","agent"}`
- `instance`: UUID string or `null`

Text output of `status`/`watch` is unchanged.

### Worker environment
Every process delegate starts with `DELEGATE_SANDBOXED=1` (worker, check, review,
plan review) runs with all variables whose names start with `HERDR_` removed. The
lead-side process environment is unchanged, and so is `CLAUDE_CODE_SESSION_ID` handling.

## Acceptance
- [ ] `scripts/check`
- [ ] `evals/run S055 S056 S057 S061 P009`
- [ ] `evals/run S147`
- [ ] Unit tests use a fixture UNIX socket server (in a temp dir, one response per
      connection) and cover: Pi path and Claude id recorded; Codex is unsupported without
      its value; unknown pair; error envelope; mismatched id; mismatched pane; slow
      server past the deadline; oversize line; control/bidi/Zl characters in each field;
      relative path; Claude id mismatch with `CLAUDE_CODE_SESSION_ID`; `DELEGATE_SANDBOXED`
      set; missing or relative socket path; each failure still exits 0 with the note.
- [ ] A test shows `run`/`retry` leave origin and instance unchanged, and a recreated id
      gets a new instance.
- [ ] A test shows a worker spawn's environment has no `HERDR_*` variable even when the
      parent has several.
- [ ] Old STATE.json without the new keys loads, and status JSON gives `null` for both.

## Constraints
- No new dependencies (`uuid`, `serde_json` and std's `UnixStream` are enough).
- `anyhow` errors; no `unwrap()` outside tests; clippy pedantic clean.
- Capture failures are never errors for `new`. Never log the identity value or the
  socket response.
- Don't change `Owner`, `claim_owner`'s rules or anything that reads `session`.

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
