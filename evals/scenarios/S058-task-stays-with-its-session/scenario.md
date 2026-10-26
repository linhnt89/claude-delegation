# S058 A task stays with the session that created it
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code, often with two or more sessions open on the same project (some in worktrees).

## Starting state
A project set up with `delegate init`. Session A created t021 with `delegate new`.
t040 was created from a plain terminal.

## Steps
1. Session A runs t021; then, from session B, run another round of it, so it's ready to merge.
2. From session A, run t040.
3. Read `delegate status --json` and the status line as A and as B.

## Passes when
- t021 still belongs to A: its `session` is A's, it is counted in A's part of the status line
  (`1 ready to merge`), and B sees it only in A's dimmed count.
- t040, which had no owner, now belongs to A, the first session that ran it.
- A resumed session (same id) sees its tasks as its own; a fork has a new id and sees them under
  the parent's group.

## If this fails
Tasks jump between sessions whenever another session touches them, so the grouping can't be
trusted.

## Failure paths
- A session that only checks, reviews or merges a task never takes it over.

## Properties
- P018: every task is counted once.
