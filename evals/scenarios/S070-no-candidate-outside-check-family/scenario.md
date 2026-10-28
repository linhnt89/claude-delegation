# S070 No model outside the check writers' family: the run stops and says so
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, with a `workers.toml` whose tier has only one family. Decision 013.

## Starting state
t001 (checks) was done by a GPT-family worker and merged. t002 has `checks_by: t001`. Every
candidate in its tier is GPT-family.

## Steps
1. `delegate run t002`.

## Passes when
- No worker is started and no round is recorded.
- The message names the family, the task that wrote the checks and the tier, and says to add a
  candidate of another family to that tier in `workers.toml` (with its path).
- The run does not move to another tier on its own.

## If this fails
Either the rule is silently broken, or the task fails in a way that doesn't say how to fix it.

## Failure paths
- None beyond the above.
