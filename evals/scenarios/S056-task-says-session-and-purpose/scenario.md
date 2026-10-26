# S056 Each task says which session it belongs to and what it does
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code, often with two or more sessions open on the same project (some in worktrees).

## Starting state
As in S055: t021 and t022 from session A (main checkout), t030 from session B (worktree on
branch `pane-polish`), t031 from a plain terminal. Each spec has a title line and a Goal.

## Steps
1. Run `delegate status --json`.

## Passes when
- Every task has a `title` (the spec's first line, without `# Task <id>:`) and a `goal` (the
  Goal section's first line, never a template placeholder).
- Every task has `session` (the id of the Claude Code session that created it) and
  `session_name`: the session's title when known, else its folder or branch (`pane-polish`
  for B).
- t031 has no `session` and its `session_name` is `by hand`.

## If this fails
The pane and the status line can't say what a task is for or whose it is, so the user is back
to reading spec files to find out.

## Failure paths
- A spec with no title line or an empty Goal: `title` falls back to the id, `goal` is empty;
  nothing fails.

## Properties
- None.
