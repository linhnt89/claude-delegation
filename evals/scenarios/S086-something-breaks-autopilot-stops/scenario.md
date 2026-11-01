# S086 Autopilot stops when something breaks
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decisions 019, 026. A red gate is no longer an immediate stop: S114–S116 cover it.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls. A run where the lead's Claude account is below the quota floor.

## Steps
1. `delegate autopilot start --quota-floor <above current>`.

## Passes when
- No wave starts; the message says the quota floor stopped it and when the account resets.

## If this fails
Autopilot spends the user's whole Claude quota overnight.
