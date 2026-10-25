# S013 A retry after failed checks uses a stronger model that knows what failed
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A task whose first round at fast committed work that fails an acceptance check with a specific message.

## Steps
1. `delegate run t001` again, without `--tier`.

## Passes when
- The second round runs at standard and says it stepped up after failed checks.
- The second worker is given what failed (the failing command and its output), without the user copying it in.

## If this fails
The stronger model starts blind on half-done, failing code, so retries waste quota and the user ends up explaining the failure by hand.

## Failure paths
- None beyond the above.

## Properties
- After failed checks, the next round's tier is never lower than the last.
