# S010 Review always comes from a different model family
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A task implemented by a GPT model; review candidates that include GPT and other families.

## Steps
1. `delegate run t001`.
2. `delegate review t001`.

## Passes when
- The reviewer is from a different family than the implementer, and `list` shows which model reviewed.
- With no other-family reviewer configured, review refuses with a clear message instead of using the same family.

## If this fails
A model reviews its own family's work, so the second opinion the user relies on isn't one.

## Failure paths
- None beyond the above.

## Properties
- For any worker config, the reviewer's family differs from the implementer's.
