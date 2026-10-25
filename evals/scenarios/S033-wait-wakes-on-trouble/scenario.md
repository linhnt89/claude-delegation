# S033 Claude is woken when a run dies or stalls
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated
- Timeout: 180

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. Claude as lead is waiting on two background runs.

## Starting state
Two background runs: one whose runner gets killed, one whose worker goes quiet. The stall threshold is shortened for the check.

## Steps
1. `delegate wait t001` while t001's runner is killed.
2. `delegate wait t002` while t002's worker stays quiet past the threshold.
3. `delegate wait` (no ids) with both still in those states.

## Passes when
- A killed run: `wait` returns within 5 seconds, saying t001 was interrupted and what to run next.
- A stall: `wait` returns within 5 seconds of the task being flagged, saying t002 stalled, in which phase, for how long.
- Without ids it waits on every running task and returns on the first event.

## If this fails
A dead or hung worker goes unnoticed until someone looks, wasting time and quota.

## Failure paths
- None beyond the above.

## Properties
- None.
