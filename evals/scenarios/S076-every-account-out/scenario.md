# S076 Every candidate's account is out: the run stops and says when to come back
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, late in a heavy day. Decision 014.

## Starting state
Every candidate in the tier is on an account the stand-ins report as exhausted, resetting at
different times.

## Steps
1. `delegate run t001`.

## Passes when
- No worker starts and no round is recorded; the exit status is non-zero.
- The message names each account and its reset, and the earliest reset first.
- It doesn't move to another tier on its own.

## If this fails
The runner starts a model that fails at once, or the user can't tell when work can resume.

## Failure paths
- None beyond the above.
