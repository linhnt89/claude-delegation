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
1. Open the delegate pane in Claude Code.
2. Watch it for a minute while the tasks go on.

## Passes when
- The pane lists each task with phase, worker, elapsed time and files changed, and the latest steps of running tasks.
- It updates by itself as steps and phases change; a stalled task stands out.
- It looks right in light and dark mode.

## If this fails
The user who lives in the desktop app has no live view of their workers there.

## Failure paths
- No tasks: the pane says so instead of looking broken.

## Properties
- None.
