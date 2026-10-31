# S103 The check's writers say the check is right, and the failure goes back to the implementer
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, after a `fix-check` task whose worker finds the check correct. Decision 025.

## Starting state
As S101, after `fix-check` started t004. t004's worker changes no files and writes
`.task/REPORT.md` with `Status: check is right` and a reason.

## Steps
1. `delegate merge t004`.
2. `delegate run t003` (its next round).

## Contract
- Step 1 exits 0, adds no commit to the base branch, and closes t004 (gone from `delegate list`,
  no `task/t004` branch, no task folder left). Its output says the check was judged right.
- In step 2, t003's `.task/CHECKS.txt` holds S001's failing output and t004's reason, marked as
  a product failure.

## Passes when
- "The check is right" is a normal outcome, not an error, and its reason reaches the implementer
  without the lead copying it.

## If this fails
A check that was right gets "fixed" anyway, or the implementer never learns its work was wrong.
