# S019 After merge, nothing of the task is left behind
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A finished task with checks passed, a review and a report.

## Steps
1. `delegate merge t001`.
2. `delegate list`.

## Passes when
- The task folder, worktree and `task/t001` branch are gone; `list` doesn't show it.
- Git history has one squashed commit, with no spec, report, state or check files in it.

## If this fails
Clutter piles up after every task, or review-time paperwork leaks into the project's history.

## Failure paths
- None beyond the above.

## Properties
- Merge or discard leaves no task state anywhere.
