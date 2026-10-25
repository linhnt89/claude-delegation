# S008 Merge is refused while acceptance checks fail
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A task whose worker committed a change that fails one of the acceptance commands.

## Steps
1. `delegate run t001`.
2. `delegate merge t001`.

## Passes when
- Merge is refused and names the failing check.
- Main is unchanged; the task stays open for another round or discard.

## If this fails
Broken work can land on main while the kit says the gates passed.

## Failure paths
- Fix committed in the worktree, then `delegate check t001`: merge now allowed.

## Properties
- Merge never succeeds unless acceptance checks passed on the final commit.
