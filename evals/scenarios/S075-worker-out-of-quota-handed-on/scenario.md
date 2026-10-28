# S075 A worker that runs out of quota mid-task hands over to the next model
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead, with a task running when its worker's subscription runs out. Decision 014.

## Starting state
The tier's first candidate makes some edits, then fails with a usage-limit error. The second
candidate is on another account.

## Steps
1. `delegate run t001` (once, without retrying by hand).
2. After it finishes, merge it.

## Passes when
- The first worker's edits are committed, not lost.
- The runner says the first worker hit its quota, starts the next round at the same tier on
  the second candidate, and tells it to continue from the committed work.
- The second worker's round does the rest, and the checks pass.
- The quota round doesn't count as a failed round for step-ups.
- The merge commit has a `Worker:` trailer for each worker that committed.

## If this fails
A task stalls or dies with half its work until the user notices and switches models by hand.

## Failure paths
- The second candidate runs out too and no other is left: the run stops, says which accounts
  are out and when the earliest resets, and keeps the committed work.
