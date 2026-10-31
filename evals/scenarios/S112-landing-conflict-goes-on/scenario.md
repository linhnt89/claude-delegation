# S112 A landing conflict lands nothing and the run goes on
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, present, editing main while autopilot runs. Decision 026.

## Starting state
Autopilot is running, present. The user commits a change on main that conflicts with wave 1's
merge.

## Steps
1. Let wave 1 finish, then wave 2.

## Contract
- After wave 1, main and `autopilot` are unchanged by landing; the wave summary, `status` and
  the report name the conflicting files.
- The run is not stopped by the conflict: wave 2 runs.

## Passes when
- A conflict never leaves main half-merged and never stops the run.

## If this fails
The user's own edits get clobbered, or one conflict halts all work.
