# 039: The side pane may use signal-hook, isolates unreadable tasks, and ships in two tasks

- Date: 2026-11-04
- Status: accepted; parked by 040

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Independent plan review of decision 038's specs (GPT-6.1 Sol, 0 high / 5 medium / 1 low)
found three things to decide:
- Std Rust can't catch SIGTERM/SIGHUP without `unsafe`, so the side pane couldn't restore
  the terminal or clear its sidebar rows on those signals.
- `status::task_ids` turns an unreadable task store into "no tasks", and one corrupt task
  fails the whole list.
- One task mixed the reporter with the interactive screen and launcher, and its checks
  didn't cover the screen or the launch.

## Decision
- **`signal-hook` is allowed** in `delegate`, for the side pane only, through its safe flag
  API. SIGINT, SIGTERM and SIGHUP each restore the terminal and send a final clearing
  report. No other crate is approved by this.
- **The side pane reads tasks fallibly and in isolation.** An unlistable store is an
  outage (shown stale, rows cleared after the TTL), never an empty list. One unreadable
  task becomes `unavailable · <id>` while the others stay live. Activity is read with
  byte and record budgets. Existing status callers keep their behaviour.
- **Two tasks.** t124: reporter, bounded fallible reader, plain frames, quit and signals
  (S148, hidden S150). t140: keys, `--beside`, `--sidebar-config`, skill text (S151),
  plus a lead-run walkthrough with real Pi/Claude leads in installed Sheprd.
- Hidden S150 was revised to match: a corrupt task is isolated, and the outage is an
  unlistable store, which keeps the running worker alive.

## Rejected options
- A shell wrapper plus TTL instead of signal handling — leaves rows up to a TTL after a
  stop and would change S148.
- Making the whole list stale on any task error — one corrupt record would blind the pane.
- One larger t124 — harder to review, and its screen and launch would go unchecked.

## Consequences
`delegate` gains its first signal dependency. The fallible reader is new code beside
`status::collect`; switching other views to it is a separate choice.
