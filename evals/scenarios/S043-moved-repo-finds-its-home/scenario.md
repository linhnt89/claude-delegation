# S043 Move or rename a project, and learn where its task state and hidden checks went
- Job: J7
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) who moved or renamed their project folder, for example
`mv ~/Work/trial/shop ~/Work/shop`, with `delegate` on PATH.

## Starting state
A project in use: one task with a ready spec and a finished round in its delegate home, and
two hidden scenarios in `_evals/hidden/scenarios/` there. Then the repo folder is moved; its
delegate home stays where it was.

## Steps
1. `delegate list` in the moved repo.
2. `evals/run fast` in the moved repo.
3. Do what the message says, then `delegate list` and `evals/run fast` again.

## Passes when
- `delegate list` doesn't just say "no tasks": it says the project's delegate home is still at
  the old path, and gives the one command that moves it beside the repo.
- `evals/run` says the same before running, instead of silently running without the hidden set.
- After that command, `delegate list` shows the task as it was and `evals/run` runs the hidden
  scenarios.
- Nothing is moved or deleted for the user.

## If this fails
Moving a project quietly cuts it off from its open tasks and hidden checks; the old folder is
easy to delete without knowing what's in it, as happened to this repo's first hidden set.

## Failure paths
- The old home is gone too: the message says the home at the old path is missing, so open tasks
  and hidden scenarios from before the move are lost, once; after that the new home is used
  without warnings.
- A copy, not a move (the old checkout still exists): no message, and nothing tells the user
  to move the original's home away from it.
