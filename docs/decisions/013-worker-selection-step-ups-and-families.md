# 013: Step up only on product failures; keep implementers apart from check writers

- Date: 2026-10-28
- Status: accepted

## Context
A task steps up one tier after any round whose checks fail, and never steps down. Broken
checks (a property generator bug in secsw-arch) and a wrong acceptance line in a spec moved
tasks to `deep`, where the first candidate is Claude Opus, so most rounds ran on the lead's own
account. The lead tried to keep check writers and implementers in different families by
pinning `--worker`; those pins went stale once the implementer stepped up, and in secsw-arch
both the checks and the product ended up written by Claude.

## Decision
- **Check errors don't step up.** A check exits **3** when the check itself is broken (not a
  product verdict). `evals/run` reports it as its own outcome and exits 3 when check errors
  are the only failures. The runner records that round's checks as `CheckError` and does not
  count it toward a step-up; the next round stays at the same tier.
- **Step-ups are configurable.** `step_up_after` in `workers.toml` `[defaults]` is the number
  of consecutive product failures at the current tier before the next round moves one tier
  up. Default 1 (the earlier behaviour).
- **A task never steps down**, not even for a small correction round after deep passes.
- **The runner keeps the implementer's family apart from the check writers'.** A spec names
  the tasks that wrote its checks (`checks_by: t009`). Every merge commit carries a
  `Worker: harness:model@effort` trailer; at run time the runner finds those tasks' merge
  trailers and skips candidates from their families, as review skips the implementer's.
  If no candidate is left at the tier, the run stops and names the family; it never picks
  another tier on its own.
- **The lead doesn't pin workers for family reasons.** `--worker` is for debugging and demos.

## Rejected options
- Lead-side `--worker` pins — stale as soon as the tier changes.
- Stepping down after a passing deep round — brings back family mixing for little saving.
- Guessing a check error from output text — exit status is the contract, as for pass/fail/skip.

## Consequences
- Check authors must exit 3 for their own bugs; `evals/lib` helpers should make that easy.
- Merge commits gain a trailer; tasks merged before it have none and impose no family rule.
- Every `implement` tier needs at least two families for `checks_by` to be satisfiable.
