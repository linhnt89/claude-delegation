# S029 The Claude Code status line shows running tasks
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code (terminal or desktop app).

## Starting state
A project set up with `delegate init`; two tasks running, one ready to merge.

## Steps
1. Run the status line command that `init` configured, the way Claude Code runs it.

## Passes when
- It prints one short line: each task's id and phase (and stalled when it is), and how many are ready to merge.
- With no tasks it prints nothing delegate-related.
- It answers fast enough not to slow Claude Code down (well under a second).

## If this fails
The user has to leave Claude Code to see that a task finished or got stuck.

## Failure paths
- Run outside a delegate project: prints nothing and exits 0.

## Properties
- None.
