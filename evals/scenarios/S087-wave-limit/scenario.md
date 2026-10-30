# S087 The wave limit stops a long run
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A mandate and specs for five waves, one task each.

## Steps
1. `delegate autopilot start --max-waves 2`; wait until it stops.
2. `delegate autopilot start` again; wait until it stops.

## Passes when
- The first run merges waves 1–2 and says it stopped at the wave limit.
- The second run continues from wave 3 and reaches the goal; no task runs twice.

## If this fails
A confused loop runs without end, or a restart redoes finished work.
