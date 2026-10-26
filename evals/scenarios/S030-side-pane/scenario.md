# S030 A side pane in Claude Code shows each task's steps
- Job: J8
- Tier: full
- Judged by: human
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from the Claude Code desktop app.

## Starting state
A project set up with `delegate init`; two tasks running with steps, one stalled.

## Steps
1. Start the runs with `delegate run --background`; don't open anything.
2. Watch the pane for a minute while the tasks go on, in light mode, then in dark mode.
3. Merge or discard every task.

## Passes when
- The pane opens by itself when a task starts, without running `/delegate-pane`.
- Each task is a card that starts with its status in words (`⚠ STALLED · quiet 6m`, `✗ interrupted`, `● implementing`, `✓ ready to merge`), then the id and worker, then elapsed time, time since the last activity and the number of files changed; a running task shows up to its 3 latest steps.
- A stalled or interrupted task has a coloured bar down its left edge, in the warning colour for stalled and the error colour for interrupted, and a line saying what to do next.
- Tasks that need the user come first: stalled, interrupted or failed, then ready to merge, then running, then not started.
- It updates by itself as steps and phases change.
- It is easy to read in both light and dark mode: every colour stands out from the background, and dim text is still readable.
- It closes by itself once no task is left to act on (all merged or discarded).

## If this fails
The user who lives in the desktop app has no live view of their workers there.

## Failure paths
- No tasks: the pane says so instead of looking broken.

## Properties
- None.
