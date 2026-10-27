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
1. Start the runs with `delegate run --background`, then open the pane (S060).
2. Watch the pane for a minute while the tasks go on, in light mode, then in dark mode.
3. Close the pane by hand, look at the row above the prompt, then click it.
4. Merge or discard every task.

## Passes when
- Each task is a card that starts with its status in words (`⚠ STALLED · quiet 6m`, `✗ interrupted`, `● implementing`, `✓ ready to merge`), then `◆` (this session's) or `○` (another session's), the id, the spec's title and the worker, then the first line of its Goal, dimmed, then elapsed time, time since the last activity and the number of files changed; a running task shows up to its 3 latest steps.
- A stalled or interrupted task has a coloured bar down its left edge, in the warning colour for stalled and the error colour for interrupted, and a line saying what to do next.
- Within each session's group (S057), tasks that need the user come first: stalled, interrupted or failed, then ready to merge, then running, then not started.
- It updates by itself as steps and phases change.
- It is easy to read in both light and dark mode: every colour stands out from the background, and dim text is still readable.
- While the pane is closed and a task is left to act on, one row above the prompt sums up the tasks, trouble first, with a count per status in the same colours as the pane: `delegate  ⚠ 1 stalled · ✗ 1 failed · ✓ 1 ready to merge · ● 2 running  [Open tasks]` (stalled in the warning colour, interrupted or failed in the error colour, ready to merge in the success colour), in the desktop app and in the terminal.
- The row's `Open tasks` button opens the side pane with a click in the desktop app and in the fullscreen terminal, at any window width. On the terminal's normal screen, where clicks don't reach it, the row ends with `/delegate-pane or ctrl+x tab, Enter to open`, and both ways open the pane.
- The row is hidden while the pane is open, comes back when the pane is closed with tasks left, and is gone once no task is left to act on.

## If this fails
The user who lives in the desktop app has no live view of their workers there.

## Failure paths
- No tasks: the pane says so instead of looking broken.
- `delegate status` can't be read: the row says `delegate: status unavailable` with the same way to open the pane instead of disappearing.

## Properties
- None.
