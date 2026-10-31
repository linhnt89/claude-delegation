# S114 A flaky red gate is rerun once before anything else happens
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away, with autopilot's gate failing once by chance. Decision 026.

## Starting state
A mandate whose gate fails on its first run after wave 1's merge and passes on the second.

## Steps
1. `delegate autopilot start`; let it stop.

## Contract
- The gate runs twice after wave 1; the log says it was rerun; no revert commit on `autopilot`.
- The run goes on to its goal; stop reason is the normal finish.

## Passes when
- One flaky failure doesn't cost a revert or the run.

## If this fails
Autopilot stops or reverts good work because of noise.
