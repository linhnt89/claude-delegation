# S089 Notification is safe and never breaks the run
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user. A stand-in `claude` on PATH plays each wave's headless lead from a script (runs `delegate` commands, edits `PLAN.md`); stand-in workers do the tasks. Decision 019.

## Starting state
A project after `delegate init` with a `## Mandate` in `PLAN.md` (goal: t001–t004; merge rule: checks and scenarios pass; gate command named) and specs for two waves: t001, t002 in wave 1, t003, t004 (depending on t001) in wave 2. A `notify` command is configured that records its calls.

## Steps
1. Run with a `notify` command that exits 1.
2. Run with `notify` unset.
3. Run with a recording `notify`.

## Passes when
- A failing notify is noted in the autopilot log; the run still ends with its report.
- With no `notify`, nothing is sent and the report is written.
- The recorded message has counts, ids and the stop reason; no diff, file content or spec text.

## If this fails
A broken notifier loses the run's report, or the notification leaks code to an outside service.
