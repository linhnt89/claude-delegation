# S123 With two families, work goes on with another model
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A user with only Claude and GPT accounts, `independence = "model"` in workers.toml. Decision 029.

## Starting state
S001's checks were written by GPT (`gpt-6.1-sol`) and later fixed by Claude Opus. t003 has
`evals/run S001`; the implement tier lists `pi:openai-codex/gpt-6.1-sol` and `claude:sonnet`.

## Steps
1. `delegate run t003`, then `delegate merge t003` once it passes.
2. `delegate list` before the merge.

## Passes when
- t003 runs on `claude:sonnet`; the run says no other family was left and it fell back to
  another model of the Claude family.
- `delegate list` shows `same-family fallback` on t003's row.
- The merge commit carries `Independence: model` after its `Worker:` line.

## If this fails
Users with one or two model families can't delegate tasks judged by checks both of them
touched, and the kit stops working for them.

## Failure paths
- A tier that also has a DeepSeek-family candidate picks it and records no fallback.
