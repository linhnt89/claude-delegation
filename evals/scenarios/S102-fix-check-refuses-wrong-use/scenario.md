# S102 `fix-check` refuses a fix it can't ground in a real failure
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, calling `fix-check` with a mistake. Decision 025.

## Starting state
As S101: t003's last `CHECKS.txt` shows S001 failing and S002 passing; S001's check was written
by the DeepSeek family.

## Steps
1. `delegate fix-check S002 --from t003 --reason "x"` (S002 didn't fail).
2. `delegate fix-check S001 --from t003` (no reason).
3. `delegate fix-check S001 --from t003 --reason "x"` with no DeepSeek-family candidate in the tier.

## Contract
- Each step exits 1, creates no task (`delegate list` unchanged, no new `task/*` branch).
- Step 1's message names the scenarios that did fail in t003's last round.
- Step 2's message says to give `--reason`.
- Step 3's message names the family, the check and the tier and says to add a candidate in
  `workers.toml` (as S099).

## Passes when
- A mistaken call changes nothing and says what to do next.

## If this fails
The lead can start a check fix for a check that never failed, or one with no stated reason, so
checks get bent without a record of why.
