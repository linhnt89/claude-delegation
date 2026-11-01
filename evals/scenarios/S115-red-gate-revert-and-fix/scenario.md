# S115 A merge that turns the gate red is reverted and gets a fix task
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away, when a merge breaks the gate. Decision 026.

## Starting state
Three tasks: t001 (its merge makes the gate fail every time), t002 depends on t001, t003 is
independent.

## Steps
1. `delegate autopilot start`; let it stop.

## Contract
- After t001's wave, `autopilot` has a `Revert "t001: …"` commit and the gate passes again.
- The next wave lead's prompt names a fix task for t001, with the reverted commit and the
  gate output.
- t002 is not started while t001 is reverted; t003 runs and merges.
- While reverted, t001 doesn't count as merged: the goal isn't met, its dependents wait, and
  the report and status don't list it as merged. It counts again when its fix task merges.
- The report lists the revert with the gate output's reason.

## Passes when
- A bad merge costs one revert and one fix task, not the run.

## If this fails
Every later merge fails the same gate and the run is stuck, as on 2026-10-31.
