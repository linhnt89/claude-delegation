# Task t124: Side-pane reporter: a lead's tasks as Herdr sidebar tokens, from a bounded, fallible read

size: long

## Goal
Let the Herdr sidebar show real delegated tasks under the Pi/Claude lead that created
them, with no Sheprd changes (decision 038, variant B). Add a `delegate watch
--lead-pane` mode that reads task state with budgets, never mistakes a failed read for
an empty list, and reports up to four `<state> · <id>` rows onto the lead's pane. The
interactive screen, launcher and config snippet are t140.

## Readiness
- t139 merged (origin, instance, Herdr client, workers without `HERDR_*`).
- Lead: `signal-hook` fetched into the cargo cache (`cargo fetch` in the task worktree;
  the sandbox's `~/.cargo` is read-only).
- S148 automated (fails until built). Hidden S150 runs at review.

## Context (read these, nothing more)
- `AGENTS.md`; decisions `035`, `037`, `038`, `039`
- `docs/research/herdr-sidebar-tokens-probe.md` (observed token behaviour; required)
- `docs/task-specs/t139-herdr-lead-origin.md`, section "Contract"
- `delegate/src/watch.rs`, `status.rs` (`TaskView`, `collect`, `task_ids`), `events.rs`
  (`collect`), t139's Herdr client module, `main.rs` (`Cmd::Watch`)
- `evals/scenarios/S148-*/scenario.md`, `S144-*/scenario.md`

## Scope
- In: `delegate/src/watch.rs`, new `delegate/src/herdr_view.rs`, `delegate/src/status.rs`
  (a new fallible reader; existing functions unchanged), `delegate/src/events.rs` (a
  bounded tail reader beside `collect`), the t139 Herdr client module (add
  `pane.report_metadata`), `delegate/src/main.rs` (CLI args), `delegate/Cargo.toml` and
  `Cargo.lock` (`signal-hook` only), unit tests
- Out: `--beside`, `--sidebar-config`, arrow keys/expand/group navigation (t140); skills
  and templates; Sheprd/Herdr sources; the user's Herdr `config.toml`; `status --json`
  schema; task state; worker launching

## Contract
### `delegate watch --lead-pane <PANE> [--interval S]` (default 2 s)
- Every interval, it reads only:
  - `pane.get` for PANE
  - the new fallible task reader below, for this checkout
- It runs no other delegate action, sends no input anywhere, and writes nothing but its
  own Herdr reports.
- **This lead** is PANE's current `agent_session` when it is a supported Pi/Claude
  identity, validated exactly like t139. Otherwise it is unknown. It follows the pane: a
  new conversation in PANE is a new lead.
- Plain frames (any stdout):
  - a header `This lead · <agent> · <6-hex digest of the identity value>` or why it's
    unknown ("no conversation in this pane", "Codex: attribution unsupported",
    "lead unavailable")
  - the health: `live · read 2s ago`, `stale since 12s` or `unavailable`
  - this lead's tasks in the same attention order as the tokens: id, title,
    phase/outcome, stalled
  - counts for other leads, Codex (unsupported) and unknown lead
  - On a terminal it redraws in place, as `watch` does now.
- Text: replace Unicode category C, Zl and Zp characters with U+FFFD, keep it single-line,
  bound it to the width and mark truncation.
- Quit:
  - `q` on a terminal (raw mode via `stty`; Ctrl-C becomes a key, which also quits)
  - SIGINT, SIGTERM or SIGHUP, caught with `signal-hook`'s safe flag API
  - Each sends one final clearing report, restores the terminal and exits 0.

### Fallible, bounded task reader (`status.rs`, `events.rs`)
- A public function, roughly `read_tasks(ctx, Budget { steps, .. }) -> Result<Vec<TaskRead>>`.
  `TaskRead` is a task's view or `Unavailable { id, reason }`. The step count is a
  parameter: t124 uses 5 per task; t140 asks for 50 for the selected task, with no
  reader changes.
- Listing the task store fails (unreadable or unlistable folder) → `Err`, never an empty
  list. Existing `task_ids`/`collect` keep their behaviour for other callers.
- One task that fails to load → `Unavailable` for that task alone; the rest are read
  normally.
- `events.jsonl`: read the last 256 KiB, at most 200 records. Skip records over 16 KiB and
  count them. Show what was dropped.
- **Whole-refresh budget.** No recursive worktree scan and no unbounded subprocess:
  - The worktree activity scan stops after 5,000 entries; the time is then "unknown".
  - Each git call (merge readiness) is killed after 2 s. That task's readiness becomes
    unknown, not ready, and its other fields still show.
  - The reader runs on its own thread. The report/quit loop never waits on it: while a
    refresh is overdue it keeps the last good rows (stale rules below). `q` and signals
    are handled within 200 ms even during a stuck refresh.

### Unreadable tasks and attribution
An unreadable task's origin can't be read, so the side pane never guesses it:
- During its own run, the side pane remembers the last successful read of each task:
  (id, instance, origin).
- A task that becomes unreadable is shown as `unavailable · <id>` under this lead only
  if that memory says it is this lead's task. It is ranked with failed/blocked.
- With no memory of it (unreadable since startup), it isn't attributed. On screen it
  counts under "unknown lead · unavailable", and it gets no token.
- Memory is kept while the id is still listed. It is dropped when the id leaves the
  listing, and replaced when a later read shows a different instance (id reuse). It is
  never written to disk.

### Tests
- a 50 MiB events file
- oversized and malformed records
- an unlistable store
- one corrupt `STATE.json`, both after a good read (shown under this lead) and from
  startup (unknown lead, no token)
- id reuse with a new instance
- a worktree with 100,000 files (the refresh stays within budget)
- a stalled `git` on `PATH`: readiness unknown, reports continue, `q` within 200 ms

### Sidebar tokens
- Each cycle sends one `pane.report_metadata` on PANE with `source = "delegate:watch"`,
  a rising `seq` and `ttl_ms = 3 × interval`, setting all five keys:
  - `dlg_t1`…`dlg_t4`: this lead's first four tasks by attention, then id. The order
    is: failed, blocked, interrupted or unavailable; stalled; ready to merge; running
    or checking; others.
  - Value: `<state> · <id>`, with state one of `failed`, `blocked`, `interrupted`,
    `unavailable`, `stalled`, `ready`, `checking`, `running`, `waiting`, `done`.
    Never a title.
  - `dlg_more`: `+N more`, or null
  - unused keys: null
- All five are null when this lead is unknown or unsupported, or when the task list has
  been unreadable for longer than the TTL. Until then it repeats the last good rows,
  labelled stale on screen.
- If Herdr is unreachable, it says "lead unavailable", keeps running and retries every
  interval. It reports again once Herdr is back.
- Values are sanitized and at most 80 chars.

## Acceptance
- [ ] `scripts/check`
- [ ] `evals/run S148 S055 S056 S057 P009`
- [ ] Unit tests with a fixture Herdr socket cover:
  - four rows plus `+N more`, attention order and nulls
  - clearing for Codex, a missing identity, an identity change and on every exit path
    (`q`, Ctrl-C, SIGINT, SIGTERM, SIGHUP)
  - TTL and seq on every report
  - state-first values without titles
  - sanitizing
  - stale versus empty
  - one unavailable task among live ones
  - the bounded reader budgets
- [ ] Review: only `pane.get`/`pane.report_metadata` are sent, no Herdr config is
      written, and `signal-hook` is the only new dependency.

## Constraints
- `signal-hook` is the one dependency this spec allows (decision 039). No `unsafe`, no
  `unwrap()` outside tests, `anyhow`, clippy pedantic clean.
- Never infer a lead from pane position, focus, checkout name or model.
- Tokens are display-only: never read them back as data.

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
