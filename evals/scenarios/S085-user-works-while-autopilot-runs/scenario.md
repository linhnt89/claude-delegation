# S085 I keep working while autopilot runs
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, present, in another lead session in the main checkout. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls. Autopilot is running wave 1 (the stand-in workers are slow).

## Steps
1. In the main checkout: edit a file, `delegate new t005` and fill its spec.
2. `delegate run t005 --background`.
3. Wait for autopilot to stop.

## Passes when
- The user's edits and files in the main checkout are exactly as they left them; nothing from autopilot appears there.
- Writing the t005 spec works.
- `delegate run t005` is refused with a message that names `delegate autopilot stop`; no task starts.

## If this fails
Autopilot changes files under the user's hands, or two leads merge at once and clobber each other.

## Failure paths
- After `delegate autopilot stop`, `delegate run t005` works.
