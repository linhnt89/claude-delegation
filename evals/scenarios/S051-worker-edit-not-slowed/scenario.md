# S051 A worker's edits are never refused
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A Claude Code worker running a delegated task. It loads the same project settings as the lead
(`--setting-sources project`), so it gets the same hooks.

## Starting state
The shop project with the guard installed, a ready spec for a task that changes
`shop/pricing.py` and adds `tests/test_split.py`.

## Steps
1. `delegate run <id>`. The worker edits product files in its task worktree; Claude Code runs the
   project's hooks on each edit as declared in `.claude/settings.json`.

## Passes when
- None of the worker's edits are refused; the task's commit has its changes.
- The worker's turn isn't held at its end either.

## If this fails
The guard meant to stop the lead stops the workers, so nothing gets built at all.

## Failure paths
- A lead started inside a task worktree by hand (no runner): still a lead, still refused.
  Being a worker comes from the runner starting it, not from where the files are.
