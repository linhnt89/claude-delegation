# S014 No model in the tier can start: clear message, no mess
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A tier where every candidate fails to start.

## Steps
1. `delegate run t001`.
2. `delegate list`.

## Passes when
- The run fails with a message listing each candidate and why it couldn't start.
- `list` shows the round as failed to start; no commit, no false success.

## If this fails
The user can't tell whether work happened, and has to clean up by hand before trying again.

## Failure paths
- None beyond the above.

## Properties
- None.
