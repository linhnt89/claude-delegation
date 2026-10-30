# S084 A question only I can answer is parked and other work goes on
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls. t001's worker asks a blocking question that neither the mandate nor the decisions answer.

## Steps
1. `delegate autopilot start`; wait until it stops.

## Passes when
- `PLAN.md` has a "Waiting for you" entry naming t001 and quoting its question.
- t002 is merged on `autopilot`; t003 (depends on t001) was never started; t004 is merged.
- The run stops with "waiting for you"; the notification counts one item waiting.

## If this fails
One question halts the whole night's work, or work is built on an answer the user never gave.

## Failure paths
- The lead must not answer the question itself or invent a decision for it.
