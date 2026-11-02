# S082 A freshly set-up project tells the lead to run a wave's tasks together
- Job: J9
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A developer who just ran `delegate init`; the lead (any model) reads the installed kit.
Decision 016; managed blocks from decisions 032, 033.

## Starting state
A new git repo with a small app; `delegate init` has run.

## Steps
1. Read the installed `AGENTS.md`, `PLAN.md` and the delegate skill.

## Passes when
- `PLAN.md` has a `wave` column.
- The delegate skill says to start every ready task of a wave in the background, then wait
  on all of them with `delegate wait` and no ids, and act on whichever finishes.
- It says to write the next specs or review finished tasks while workers run, and to start a
  task as soon as the merge it depends on lands.
- It asks one merge question per batch, naming the ids.
- The `lead` block in `AGENTS.md` says plans are split into waves of tasks that touch
  different files.
- `delegate upgrade` on a project with the kit before the wave text replaces the delegate
  skill, brings the wave line in through the `lead` block (as S133), and reports the
  `PLAN.md` template change (the wave column) for the user to take in; it never edits
  `PLAN.md`.

## If this fails
The lead keeps running one task at a time, and the user waits on each worker in turn.

## Failure paths
- The delegate skill still shows `delegate wait <id>` as the way to wait on a batch.
