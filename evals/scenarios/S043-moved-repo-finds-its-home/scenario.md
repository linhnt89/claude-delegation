# S043 Move or rename a project, and everything is still there
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) on a machine set up with `delegate setup --vault <url>` (a private remote), working on a project set up by the kit.

## Starting state
A project in use: one task with a finished round, and two hidden scenarios in the vault. Then
the repo folder is moved to another parent folder and renamed (`mv ~/Work/trial/shop ~/Work/shop2`).

## Steps
1. `delegate list` in the moved repo.
2. `evals/run fast` in the moved repo.
3. `delegate merge` the task.

## Passes when
- `delegate list` shows the task exactly as before the move, with no message about homes.
- `evals/run` runs both hidden scenarios.
- The merge works as it would have before the move, and leaves no task state behind.
- The task state's folder now carries the new name (`…-shop2`).

## If this fails
Moving or renaming a project cuts it off from its open tasks and hidden checks, as happened to
this repo's first hidden set.

## Failure paths
- The repo is moved while a task runs in the background: the run either finishes and is found,
  or shows as interrupted with a hint that works; nothing is lost silently.
