# S069 The fix is written by a different kind of model than its checks
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, who had one task write the checks and now hands the fix to another. Decision 013.

## Starting state
Task t001 wrote checks, was done by a GPT-family worker and merged. Task t002's spec says its
checks were written by t001 (`checks_by: t001`). The implement tiers list a GPT-family model
first and a DeepSeek-family model second.

## Steps
1. Look at t001's merge commit.
2. `delegate run t002` (also once with `--dry-run`).
3. Make t002's round fail its checks so it steps up, and run it again.

## Passes when
- t001's merge commit has a `Worker: pi:<model>@<effort>` trailer for the worker that did it.
- t002 runs on the DeepSeek model, and the run says it skipped the GPT family because t001
  wrote its checks. `--dry-run` shows the same.
- After the step-up the GPT family is still skipped.
- A spec without `checks_by` runs exactly as before.

## If this fails
The same kind of model writes the checks and the code that passes them, so their shared blind
spots slip through, and the user believes it was independently checked.

## Failure paths
- `checks_by` names a task with no merge commit, or one merged before trailers existed: the
  run says it can't tell that task's worker and goes on without the rule.
