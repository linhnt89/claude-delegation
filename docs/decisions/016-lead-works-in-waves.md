# 016: The lead plans and runs work in waves, not one task at a time

- Date: 2026-10-28
- Status: accepted

## Context
`delegate` can run many tasks at once and `delegate wait` with no ids watches every running
task, but the delegate skill describes a loop for one task (spec, run, `wait <id>`, review,
merge, next), with parallel runs as a single aside. Leads follow it in sequence: the user
waits on each worker, and the lead sits idle while workers run. Parallel runs also expose a
quota flaw: each run's runway check (decision 014) reads the same snapshot and ignores other
running tasks, so a batch piles onto the first account of the tier and overruns its runway.

## Decision
- **Waves.** A breakdown puts contracts and checks first, then tasks that touch disjoint
  files, each listing them in `Scope In`. `PLAN.md` has a `wave` column; plan review runs
  once per wave.
- **Batch loop.** The lead starts every ready task of the wave with `run --background`, then
  one `delegate wait --timeout 25m` with no ids, acts on whichever task it reports, and waits
  again. While workers run it writes the next wave's specs and scenarios or reviews finished
  tasks. A merge that unblocks a task is followed by starting that task at once.
- **One merge question per batch** of finished tasks, naming the ids; tasks that need another
  round are named in the same message.
- **Overlap warning.** `delegate run` warns when the new task's `Scope In` overlaps a running
  task's. It does not refuse.
- **Quota spreads across accounts.** When picking a candidate, an account's runway is divided
  by (tasks running on it + 1), and an account with no running task is preferred over a busy
  one in the same tier.

## Rejected options
- Refusing to run on Scope In overlap — some overlap (a shared test file) is fine and the
  lead can judge it.
- The runner starting tasks itself in dependency order — the lead decides what runs.
- One merge question per task — it pauses the user once per task.

## Consequences
Plans must be split for parallelism up front, so a poor split shows early as an overlap
warning instead of late as merge conflicts. Batches use more accounts at once and finish
sooner; merges later in a batch more often need a conflict round.
