# S120 Whoever changed any file of a scenario counts as its writer
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, starting an implementer. Decision 028.

## Starting state
S001's `check` was written by t001 with a Claude-family `Worker:` trailer. Later t002, with a
GPT-family trailer, changed only `S001-*/fixture.json`. t003's spec has `evals/run S001` in its
Acceptance. The implement tier lists a GPT-family model first, a DeepSeek-family model second
and a Claude-family model third.

## Steps
1. `delegate run t003`.

## Passes when
- t003 runs on the DeepSeek-family model; the run says it skipped the GPT and Claude families
  for S001's checks.

## If this fails
A model family that shaped a check's data could implement the work that check judges, so the
check is no longer independent of it.

## Failure paths
- A commit with no `Worker:` trailer (the lead's) adds no family.
