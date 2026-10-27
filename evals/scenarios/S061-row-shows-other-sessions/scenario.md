# S061 The row above the prompt shows other sessions too
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer leading from the Claude Code desktop app, where the CLI status line isn't shown,
with other sessions running tasks on the same project.

## Starting state
As S055: session A has t021 running and t022 ready to merge; session B (worktree
`pane-polish`) has t030 running and t032 stalled; t031 was started by hand.

## Steps
1. In session A, with the pane closed, look at the row above the prompt.
2. The same in session B, and in a third session with no tasks of its own.

## Passes when
- A's row shows A's counts first, in the pane's colours, then the other groups dimmed, the
  same groups and counts as A's status line: `delegate  ✓ 1 ready to merge · ● 1 running │
  pane-polish 2 (1 stalled) · by hand 1`.
- B's row is the mirror image: its own counts, then `│ shop 2 · by hand 1` dimmed.
- A session with no tasks of its own still shows the row, with only the dimmed part.
- When the row is too narrow, the other groups are cut before this session's counts.

## If this fails
In the desktop app the user can't see that another session has a stalled or finished task
without opening the pane.

## Failure paths
- With no session id known (older Claude Code), every task counts as this session's, as
  before.

## Properties
- None.
