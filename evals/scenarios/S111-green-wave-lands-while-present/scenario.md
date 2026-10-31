# S111 A green wave lands on the base branch on its own, only while the user is present
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, present, with autopilot running two waves. Decision 026.

## Starting state
A mandate of two tasks in two waves; both gates pass. main hasn't moved since `start`.

## Steps
1. `delegate autopilot start`; after wave 1, `delegate autopilot away`; let wave 2 finish.

## Contract
- After wave 1, main contains wave 1's merges (fast-forward or a clean merge, as `land`); the
  wave summary (`delegate wait` output, log) names the merged task and says it landed.
- After wave 2 (away), main is unchanged; `autopilot` holds wave 2's merge; the report says
  it waits for `land`.

## Passes when
- Present: no landing question; away: nothing lands without the user.

## If this fails
The user is stopped at every wave, or work lands on main while they're gone.
