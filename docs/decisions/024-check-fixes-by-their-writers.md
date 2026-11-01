# 024: A check fix comes from the family that wrote the check

- Date: 2026-10-31
- Status: superseded by 030

## Context
Decision 023 makes a check fix its own task and counts every family that changed a check as
its writer. Left open: which family may fix a broken check, and how the runner and merge know a
task is a check fix. Choosing a new family per fix shrinks the families left to implement.

## Decision
- **Marker.** A check-fix spec names the scenarios whose checks it may change on a
  `fixes_checks: S001, S002` line, like `checks_by:`. Merge allows that task's changes to those
  checks and still refuses changes to other checks in its Acceptance (decision 023).
- **Family.** The runner picks a check-fix task's worker only from the families that wrote
  those checks (their merges' `Worker:` trailers). If no candidate of those families is in the
  tier, no worker starts and the message names the families, the checks and the tier; it never
  picks another family or tier on its own (as decision 013).
- **Trailers stay in commit messages.** `Worker:` trailers remain the record of who wrote what,
  so the family rules work from git alone, on any machine.

## Rejected options
- Any family but the implementers a check judges — each fix adds a family to avoid, and the
  runner would have to work out which tasks a check judged.
- A `--fixes-checks` run flag — not in the spec, so lost on retries and in autopilot.
- Keeping trailers in the delegate home instead — the family rules would depend on local task
  state surviving moves and machines.

## Consequences
The spec format gains `fixes_checks:`. A check written by a family with no candidate in the
tier can't be fixed until the user adds one. Commits keep naming each change's provider and
model.
