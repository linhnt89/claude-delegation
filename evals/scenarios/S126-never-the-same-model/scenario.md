# S126 A fallback never picks the same model, by any route
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A user with `independence = "model"`. Decision 029.

## Starting state
S001's checks were written by `claude:opus`. t003 has `evals/run S001`; the implement tier
lists only `claude:opus@xhigh` and `pi:github-copilot/claude-opus-5.5@high`.

## Steps
1. `delegate run t003`.

## Passes when
- No worker starts. The message says every candidate is the model that wrote S001's checks
  (`opus`), and to add another model.

## If this fails
The same model would grade its own work, under a setting the user trusted to prevent that.

## Failure paths
- Effort and route don't make a different model: `@xhigh`, `claude-opus-5.5` and `opus` are
  one model.
