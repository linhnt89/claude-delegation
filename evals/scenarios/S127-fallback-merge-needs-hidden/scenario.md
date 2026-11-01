# S127 A same-family merge needs the hidden scenarios of its job
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, merging a task that used the fallback. Decision 029.

## Starting state
t003 ran on a same-family fallback, its checks pass, and its review has no high findings. A
hidden scenario of t003's job fails on its branch.

## Steps
1. `delegate merge t003`.
2. After the hidden scenario passes on a later round, `delegate merge t003` again.

## Passes when
- The first merge is refused: the message says hidden scenarios of the job must pass for a
  fallback task and names no hidden id or content.
- The second goes through, with the `Independence: model` trailer.

## If this fails
Work checked only within one family merges with nothing independent of it, and its blind
spots reach the user's branch.

## Failure paths
- A task that used no fallback merges as before, without that requirement.
