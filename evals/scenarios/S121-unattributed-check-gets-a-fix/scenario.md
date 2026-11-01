# S121 A check with no recorded writer can still be fixed
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, after a worker reported a bug in a lead-written check. Decision 028.

## Starting state
S001's folder was added and changed only by commits with no `Worker:` trailer. t004's spec has
`fixes_checks: S001` and `evals/run S001`. The implement tier lists a GPT-family model first.
t005, later, has `evals/run S001` in its Acceptance; its tier lists GPT first and DeepSeek
second.

## Steps
1. `delegate run t004`, then `delegate merge t004`.
2. `delegate run t005`.

## Passes when
- t004 starts on the GPT-family model, with a warning naming S001 as having no recorded writer.
- t004's merge carries its `Worker:` trailer; t005 then skips GPT and runs on DeepSeek.

## If this fails
Most checks were written by the lead, and a bug in one of them could never be fixed through
delegate, so the task it blocks stays stuck.

## Failure paths
- With `--worker` naming any family, t004 runs on it with the same warning.
