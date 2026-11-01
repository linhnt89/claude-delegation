# S128 A check fix when its writers' family can't run
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A user whose GPT account is gone. Decision 029.

## Starting state
S001's checks were written by GPT. t004 has `fixes_checks: S001`; the implement tier lists only
Claude-family candidates.

## Steps
1. With `independence = "model"`: `delegate run t004`.
2. With no `independence` key: `delegate run t004`.

## Passes when
- With `"model"`, t004 runs on a Claude model, with a warning that S001's writer family (GPT)
  has no candidate.
- Without it, nothing starts; the message names GPT, S001 and the setting.

## If this fails
A check bug blocks a task for as long as the family that wrote the check is unavailable.

## Failure paths
- With `"model"` and a GPT candidate present, the fix still runs on GPT.
