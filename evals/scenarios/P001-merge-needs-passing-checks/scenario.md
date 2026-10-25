# P001 Merge never lands work whose checks didn't pass
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Anyone using `delegate`, directly or through Claude as lead. From: S004, S008.

## Property
For any sequence of runs, fixes, `check`s and merges, `delegate merge` succeeds only if every acceptance command passed on the task's final commit. Otherwise main is unchanged.

## How it is checked
Random sequences of user actions on a task (run with a worker that passes or fails, commit a fix in the worktree, `check`, `merge`), compared after each step with a simple model of whether the final commit is checked and passing. Seeded; prints the seed and the smallest failing case.

## If this fails
Broken work can reach main while the kit says the gates passed.
