# S081 A long task skips an account whose runway is already shared with running tasks
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead starting a second `size: long` task while one runs. Decisions 014, 016, 017.

## Starting state
The tier's first candidate's account has runway for one long task but not for two (at its
pace it lasts 1.5× the long-task runway). t001 (long) is running on it. The second candidate's
account is healthy.

## Steps
1. `delegate run t002 --dry-run`, then `delegate run t002 --background` (t002 is `size: long`).

## Passes when
- Both say they skip the first candidate because its runway is shared with t001, naming the
  account and the runway left per task.
- t002 runs on the second candidate.
- With t001 not running, t002 would run on the first candidate.

## If this fails
Two long tasks share an account that can carry only one, and both stop partway.

## Failure paths
- Every candidate is busy and short: the run stops with the resets and the running tasks,
  as S076 does when every account is out.
