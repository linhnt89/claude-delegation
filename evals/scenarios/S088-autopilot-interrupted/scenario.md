# S088 Autopilot is stopped or killed and started again
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls.

## Steps
1. Start autopilot; during wave 1 run `delegate autopilot stop`; check `status`; start again.
2. Start autopilot; during wave 1 kill the driver process; check `status`; `delegate run t005` from the main checkout; start autopilot again.

## Passes when
- After stop or kill, `status` says autopilot isn't running.
- A killed driver's lock doesn't block the user: `delegate run` works (or says how to clear the lock in one command).
- Restarted runs reach the goal without rerunning merged tasks or starting a second copy of a running one.

## If this fails
An interruption leaves the user locked out or doubles the work.
