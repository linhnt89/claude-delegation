# S026 A worker whose output isn't understood still shows progress
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A run with a worker whose output format `delegate` has no adapter for (plain text).

## Steps
1. `delegate status t001` while it runs.

## Passes when
- `status` shows phase, elapsed time, files changed so far and the last lines of the worker's log.
- It says step details aren't available for this worker, instead of showing nothing or garbage.

## If this fails
Adding a new or updated worker breaks the view for that worker, so the user is blind again.

## Failure paths
- None beyond the above.

## Properties
- Covered by P010.
