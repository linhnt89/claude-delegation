# S044 A project with an old home beside it moves into the central place on first use
- Job: J7
- Tier: full
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) on a machine set up with `delegate setup --vault <url>` (a private remote), working on a project set up by the kit. The project was set up before decision 007, like every project today.

## Starting state
`shop/` with `shop.delegate/` beside it holding an open task with a finished round, eval
results, and two hidden scenarios. `.delegate-kit.toml` has no project id yet.

## Steps
1. `delegate list`.
2. `delegate list` again, then `evals/run fast`.

## Passes when
- The first `delegate list` shows the task as before and says, once, that it moved the task
  state and the hidden set and where to: under `~/.local/share/delegate/`.
- `shop.delegate/` is gone; the hidden scenarios are committed in the vault and pushed.
- The project id is written to `.delegate-kit.toml`, and the output says to commit it.
- The second `delegate list` shows the task with no message; `evals/run` runs the hidden scenarios.

## If this fails
Upgrading to the new home layout would strand today's open tasks and hidden sets in the old
folders, or ask the user to move them by hand.

## Failure paths
- This machine has no vault remote yet: the hidden set goes into a local vault delegate creates,
  and the output says to connect it with `delegate setup --vault <url>`; nothing is lost.
- Migration interrupted halfway (killed): the next command finishes it; no task or hidden
  scenario is lost or duplicated.
