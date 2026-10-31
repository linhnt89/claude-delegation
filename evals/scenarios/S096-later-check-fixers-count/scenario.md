# S096 Everyone who changed a check counts as its writer, not just the task that first wrote it
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, handing an implementation to a worker after the checks were fixed by another
task. Decision 023.

## Starting state
t001 wrote the check for S001 and was merged with a DeepSeek-family `Worker:` trailer. t002
later fixed that check and was merged with a GPT-family trailer. t003's spec has
`evals/run S001` in its Acceptance and no `checks_by:` line. The implement tier lists a
GPT-family model, a DeepSeek-family model and a Claude-family model, in that order.

## Steps
1. `delegate run t003 --dry-run`, then `delegate run t003`.

## Passes when
- t003 runs on the Claude-family model.
- The run says it skipped the GPT and DeepSeek families because they wrote S001's check, and
  names t001 and t002 (or their merge commits). `--dry-run` shows the same.
- A `checks_by:` line adds its tasks' families to the ones found from history.

## If this fails
The family a check judges can quietly fix that check, and the user believes the work was
checked independently.

## Failure paths
- A merge that touched the check has no `Worker:` trailer (merged before decision 013): the run
  says it can't tell who made that change and goes on without it, as S069 does.
- No candidate is left at the tier: the run stops and names the families and the tier, as S070.
