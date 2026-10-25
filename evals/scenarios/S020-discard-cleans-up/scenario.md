# S020 Discard removes the task and leaves main untouched
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A task with a committed but unwanted change.

## Steps
1. `delegate discard t001`.
2. `delegate list`.

## Passes when
- Task folder, worktree and branch are gone.
- Main is exactly as before.

## If this fails
Rejected work lingers or leaks into main.

## Failure paths
- None beyond the above.

## Properties
- Merge or discard leaves no task state anywhere.
