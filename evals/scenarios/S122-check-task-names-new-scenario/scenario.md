# S122 A check task may name the scenario it creates
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, merging a check-writing task. Decision 028.

## Starting state
S005's folder has only `scenario.md` (Status: agreed) on the base branch. t006's spec has
`evals/run S005` in its Acceptance; its branch adds `S005-*/check`. t007's spec has
`evals/run S001 S005`; its branch adds S005's check and changes S001's existing `check`.

## Steps
1. `delegate merge t006`.
2. `delegate merge t007`.

## Passes when
- t006 merges: a check that doesn't exist in the base isn't the task's own.
- t007 is refused, naming S001's check only.

## If this fails
Check tasks can't list the scenarios they write in Acceptance, so their checks are never run
by the runner before merge.

## Failure paths
- A check that exists in the base and is deleted by the task counts as changed.
