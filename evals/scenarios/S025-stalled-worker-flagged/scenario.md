# S025 A worker that has gone quiet is flagged
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated
- Timeout: 120

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A run whose worker makes one edit, then does nothing (no steps, no file changes) while its process stays alive. The stall threshold is shortened for the check.

## Steps
1. `delegate status t001` before the threshold, after it, and after the worker becomes active again.

## Passes when
- Past the threshold the task shows as stalled, with how long it has been quiet and its last step.
- Nothing is stopped or killed by the flag.
- When the worker does something again, the flag goes away.

## If this fails
A hung worker burns time and quota and nobody notices, one of the reasons users give up.

## Failure paths
- A long-running command (a slow build) counts as quiet too, but the flag names that command as the last step so the user can judge.

## Properties
- None.
