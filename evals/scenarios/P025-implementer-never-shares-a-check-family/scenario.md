# P025 An implementer is never from a family that wrote or changed its checks
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The runner, picking implementers over many merge histories. Decision 023.

## Property
For any history of merges with `Worker:` trailers that touched the checks a task's Acceptance
names (and hidden checks of its jobs), any `checks_by:` tasks, and any tier of candidates:
either the chosen implementer's family is none of those families, or no worker starts and the
message names the families. A check-fix task (`fixes_checks:`) instead runs only in the family
that wrote the check, or not at all. Adding a merge to the history never makes a family
allowed that was skipped before.

## If this fails
Some history lets the judged family write or fix its own checks.
