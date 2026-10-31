# S110 While present, a parked question reaches the lead session at once
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead in the user's session, waiting on a running autopilot while the user is present.
Decision 026.

## Starting state
Autopilot is running, present, on two tasks; the worker of one asks a question, so the wave lead
parks it. A later wave ends normally.

## Steps
1. `delegate wait` (no id) while the wave runs; again until the wave ends.

## Contract
- The first `wait` returns (exit 0) when the item is parked, not when the run stops, and prints
  the parked item's full text and task id.
- The next `wait` returns when the wave ends and prints that wave's summary (merged tasks, gate
  result).

## Passes when
- The user is asked within one wait, not at the end of the run.

## If this fails
A present user only learns of a question when the whole run stops.
