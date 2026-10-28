# S080 Tasks started together stay on the first account while its quota is healthy
- Job: J5
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead starting three ready tasks of a wave at once. Decisions 016, 017.

## Starting state
The standard tier has candidates on two accounts. The first account is healthy: at its pace
its runway covers three medium tasks at once. No task is running.

## Steps
1. `delegate run t001 --background`, `delegate run t002 --background`,
   `delegate run t003 --background`, one after another (all `size: medium`).
2. `delegate list`.

## Passes when
- All three run on the tier's first candidate; the second account is not touched.
- `delegate run --dry-run` for a fourth task shows the first account's runway shared by the
  three running tasks.

## If this fails
The user's preferred subscription is skipped for no reason, and quota on accounts they meant
as a fallback is spent early.

## Failure paths
- See S081: when the shared runway is too short, the next task moves on.
