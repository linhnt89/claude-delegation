# 023: Checks stay delegated and apart from the code they judge

- Date: 2026-10-31
- Status: superseded by 030

## Context
Workers write the checks (J9; the lead guard refuses `check` and `evals/lib/` edits) and the
runner keeps implementers out of the check writers' families by `checks_by:` (decision 013).
Check bugs surfaced late, during implementation: t064's checks (DeepSeek) cost t070 five
rounds and needed fixes in t076–t079. Some fixes came from the family being judged: t077 and
t079 (Luna) fixed checks for work by Sol, both OpenAI. `checks_by:` names only the task that
first wrote a check, so the runner never saw it.

## Decision
- **The lead writes no checks.** The guard is unchanged; `check` files and `evals/lib/` stay
  product files.
- **Check tasks are reviewed for the reason they fail.** A check task's REPORT lists each
  check with the failure it gives on the current product. Before work that depends on it
  starts, the lead confirms each fails for the reason its `scenario.md` states: no setup
  error, no exit 3.
- **Implementers don't change their own checks.** `delegate merge` refuses an implementation
  task whose diff touches the `check` of a scenario named in its Acceptance; a check fix is
  its own round or task. Changes to `evals/lib/` warn only.
- **Check writers' families come from git.** The runner collects the `Worker:` families of
  every merge that touched the `check` of the spec's Acceptance scenarios, hidden ones
  included, and adds those of the `checks_by:` tasks. It skips them all when picking the
  implementer, as decision 013 does for `checks_by:` alone.

## Rejected options
- The lead writing checks — needs a guard exception, grows into implementation (decision 004)
  and spends the lead's context on harness and setup code.
- The lead writing only bug-repro checks — where it is most tempted to write the fix too.
- `checks_by:` alone — misses later fixes to a check (t077, t079).

## Consequences
A broken check costs a separate round instead of being fixed in passing by the implementer.
Families to avoid grow with each check fix, so fixes should come from families already on
that list. Every implement tier still needs at least two families.
