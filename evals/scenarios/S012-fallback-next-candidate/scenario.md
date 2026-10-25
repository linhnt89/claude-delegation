# S012 A model that can't start is skipped for the next one
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A tier whose first candidate fails to start with an auth or quota error; the second works.

## Steps
1. `delegate run t001`.
2. `delegate list`.

## Passes when
- The run says the first candidate couldn't start and why, then uses the second.
- `list` shows the model that actually did the work.

## If this fails
One expired login or empty quota stops all work, or the user is misled about which model did it.

## Failure paths
- None beyond the above.

## Properties
- None.
