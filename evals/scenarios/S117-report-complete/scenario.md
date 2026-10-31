# S117 The run report shows every waiting item in full
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, back from away, reading the run report. A bug: the 2026-10-31 report cut off its
"Waiting for you" item mid-sentence. Decision 026.

## Starting state
A run that parked a task with a multi-line question longer than 500 characters, received one
answer, and reverted one merge.

## Steps
1. Read `report.md` of the run (path from `delegate autopilot status`).

## Contract
- "Waiting for you" holds each parked item's whole text, every line.
- The report has "Answers received" and "Reverts" sections listing each with its reason, and
  "Proposed decisions to confirm".

## Passes when
- The user acts on the report without opening logs.

## If this fails
The user has to dig through logs to learn what waits for them.
