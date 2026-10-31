# S104 A check fix reaches the waiting implementer's next round on its own
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, after merging a check fix while the implementer's task is between rounds.
Decision 025.

## Starting state
t003's branch was cut from main before t004 (a fix to S001's check) was merged into main. t003
is in review after a failed round.

## Steps
1. `delegate run t003`.
2. The same where t004 and t003 changed the same file in conflicting ways.

## Contract
- Step 1: t003's next round sees t004's version of S001's check in its worktree, and the run
  output names t004 (or its merge commit) as brought in from the base branch.
- Step 2: the run exits 1, starts no worker, leaves t003's branch unchanged, and names the
  conflicting files.

## Passes when
- The lead never merges a fix into a task branch by hand.

## If this fails
The implementer keeps failing against the old check after it was fixed, and burns rounds.
