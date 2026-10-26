# S055 The status line shows my session's tasks, other sessions as counts
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code, often with two or more sessions open on the same project (some in worktrees).

## Starting state
A project set up with `delegate init`. Session A (the main checkout) created and ran t021
(running) and t022 (ready to merge). Session B (a worktree on branch `pane-polish`) created and
ran t030 (running) and t032 (stalled). t031 was created and run from a plain terminal, outside
Claude Code (running).

## Steps
1. Run the status line command that `init` configured, the way Claude Code runs it, with
   session A's `session_id` in the stdin JSON.
2. The same with session B's `session_id`, from B's worktree.
3. The same with no `session_id` (an older Claude Code).

## Passes when
- In A it shows A's tasks as today (`t021 implement · 1 ready to merge`), then, dimmed, one
  count per other group: `│ pane-polish 2 (1 stalled) · by hand 1`. No other session's task id
  appears.
- In B it shows `t030` and `t032 stalled`, then A's group and `by hand 1` as dimmed counts;
  `t021` and `t022` don't appear.
- A group is named by the session's title when known, else its folder or branch.
- With no `session_id` it prints today's line for every task.
- Still one line, well under a second.

## If this fails
In every Claude Code session the user sees every session's tasks mixed together and can't
tell which are theirs.

## Failure paths
- A session with no tasks of its own still sees the other groups' counts, and nothing at all
  when no task anywhere is left to act on.

## Properties
- P018: every task is counted once.
