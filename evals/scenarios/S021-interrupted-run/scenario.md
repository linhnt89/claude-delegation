# S021 A run killed halfway shows up and can be cleaned up
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A run in progress with a slow worker.

## Steps
1. Kill `delegate run t001` partway.
2. `delegate list`.
3. `delegate discard t001`.

## Passes when
- `list` shows the task as interrupted or not finished, never as passed.
- `discard` removes everything; a fresh `run` works.

## If this fails
A crash or Ctrl-C leaves a task the user can neither see nor clean up.

## Failure paths
- None beyond the above.

## Properties
- None.
