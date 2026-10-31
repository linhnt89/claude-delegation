# S113 The user's answer unparks a task in the next wave
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, answering a question autopilot parked. Decision 026.

## Starting state
Autopilot is running; t002 is parked with a question in wave 1.

## Steps
1. `delegate autopilot answer t002 "use the second option"`; let the next wave run.

## Contract
- The command exits 0 and leaves the `autopilot` branch head unchanged; the answer is queued
  in the delegate home (`autopilot status --json` lists it).
- The next wave lead's prompt contains the answer and t002; the queue is empty once that wave
  starts; t002 runs in that wave.
- The run report lists the answer under "Answers received".
- `answer` for an id that isn't parked exits 1 and names the parked ones.

## Passes when
- One command answers; the user never edits the autopilot branch.

## If this fails
Parked tasks wait for the next run, or the user's edit races the wave lead's `PLAN.md`.
