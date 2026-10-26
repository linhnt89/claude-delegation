# 004: The lead never commits on a task branch

- Date: 2026-10-26
- Status: accepted

## Context
The delegate skill let Claude, as lead, "commit a small fix in the worktree yourself" and fix
docs "in the same branch before merging". In S006 that turned review into rework: the worker
passed every check, then Claude moved its tests to the file it preferred, rewrote a docstring
and marked the task done on the task branch. The work stops being the worker's, the user's
other models look unused, and nobody can tell which model did what.

## Decision
Only the runner commits on a task branch (`task <id> round N`). The lead judges the work by the
spec's Acceptance and contract, not taste. A choice that works and keeps the contract is
accepted and mentioned to the user. Anything that breaks the spec, however small, goes back
as another round, and `REVIEW.patch` tests go to the worker through `.task/`. A merge conflict
with the base branch too: the lead only starts `git merge --no-commit <base>` in the worktree,
a round tells the worker to resolve it, and the runner's round commit concludes the merge
(`delegate merge` says so when it refuses). Doc fixes and
the `PLAN.md` status go in the lead's own commit on the base branch right after the merge.

## Rejected options
- Lead commits small fixes — the reason S006 failed: small fixes grow into the implementation.
- An exception letting the lead commit a merge of the base branch — conflict resolution is
  code, and it is where the lead most often ended up writing implementation (t010, t014).
- Doc and `PLAN.md` fixes made by the worker in a final round — costs a worker round for
  bookkeeping the lead can do in seconds after the merge.

## Consequences
A trivial fix or a merge conflict costs a worker round. The squash commit holds only the worker's change, and the
doc update lands as a separate commit just after it.
