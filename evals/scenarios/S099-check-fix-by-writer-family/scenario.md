# S099 A broken check is fixed by the family that wrote it
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, after an implementer reported a check bug. Decision 023.

## Starting state
t001 wrote S001's check and was merged with a DeepSeek-family trailer. t005's spec is a check
fix: it names S001 as the check it fixes (`fixes_checks: S001`). The tier lists a GPT-family
model first and a DeepSeek-family model second.

## Steps
1. `delegate run t005`, then `delegate merge t005` after it changes S001's check.
2. The same with no DeepSeek-family candidate in the tier.

## Passes when
- t005 runs on the DeepSeek-family model, and the run says it picked the family that wrote
  S001's check.
- Its merge is not refused for changing S001's check.
- With no DeepSeek-family candidate, no worker starts; the message names the family, the check
  and the tier and says to add a candidate in `workers.toml`. It does not pick another family
  or tier.

## If this fails
Check fixes bring in new families, so each fix shrinks the models left to implement, or the
family being judged fixes its own judge.

## Failure paths
- `fixes_checks:` names a scenario with no check: the run refuses and says which.
