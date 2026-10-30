# S083 Autopilot finishes a two-wave plan and I land it with one command
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, leaving after a prep session. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls.

## Steps
1. `delegate autopilot start`, then wait until `delegate autopilot status` says it stopped.
2. Look at the user's checkout, the run report and the notify calls.
3. `delegate autopilot land`.

## Passes when
- All four tasks are merged on the `autopilot` branch, wave 1 before wave 2; each wave had its own lead session.
- Until `land`, the user's checkout and base branch are unchanged.
- Exactly one notification, saying the goal is met; the run report lists the four merges and why it stopped.
- After `land`, the base branch has all four merges and `delegate list` shows no task state.

## If this fails
Work the user left to run overnight isn't done, or lands somewhere they didn't expect.

## Failure paths
- A task fails its checks: another round runs as today; only the round limit parks it (S084).
