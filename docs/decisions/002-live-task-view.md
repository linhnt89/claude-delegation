# 002: A live view of running tasks

- Date: 2026-10-25
- Status: accepted

## Context
Runs go to the background with nothing to see: `list` only knows a round once it ends, and
`run.log` holds little more than the command line, because workers run headless. The user
and Claude (as lead) both need to know, at any moment, what each task is doing and whether
it has stalled or died. Scenarios: J8 in `evals/jobs.md` (S022–S031, P009–P011).

## Decision
- **Live state.** While a task runs, the runner keeps `<repo>.delegate/<id>/status.json`
  current: phase (`questions`, `implement`, `checks`, `review`, `done`, `interrupted`), outcome
  when done (`pass`, `fail`), worker, tier, candidate, pid, start time, heartbeat. A run whose
  process is gone before `done` is `interrupted`. Every view reads this; no view writes
  anything (P011).
- **Steps.** Workers run in their harness's JSON streaming mode. One adapter per harness
  turns the stream into common steps in `events.jsonl`: `read`, `edit`, `run` (with result),
  `think`, `message`, `error`. The raw stream is kept beside it. A worker without an adapter
  gets the basic view (changed files, log tail) and `steps_available: false`.
- **Views.** `delegate status [id] [--json]` prints once; `--json` is how Claude checks a run.
  `delegate watch [id]` refreshes by itself; when stdout isn't a terminal, each refresh starts
  with a line `delegate watch`. `delegate statusline` prints one line for Claude Code's status
  line. A Claude Code side pane mod shows tasks and their latest steps. `delegate init`
  installs the status line and the pane, and never replaces a status line the user already has.
- **`delegate run --background`** detaches into its own session and returns at once with the
  task id and `delegate status <id>`. A run that can't start fails in the foreground and leaves
  nothing behind.
- **Stalled** = no new step and no file change for 5 minutes (`DELEGATE_STALL_SECS`). A
  warning only: nothing is stopped.
- **Recorded streams** for checks live in `evals/lib/streams/`; their `.steps` files are
  written by the lead, never by whoever writes an adapter.

## Rejected options
- Basic view only (changed files and log tail) — the user wants to tell a worker going in
  circles from one making progress, which needs its steps.
- A local web dashboard — needs a server and new dependencies; the terminal and Claude Code
  cover where the user looks.
- Killing stalled workers automatically — a slow build looks the same as a hang; the user decides.

## Consequences
- Harness commands change to streaming flags; each harness's format can drift, so recordings
  are re-made when a harness changes, and S023/P010 catch the drift.
- `status.json` becomes the one source for phase: `list` must agree with it (P009).
- `init` now touches `.claude/settings.json`, so it must merge, not overwrite (S031).
