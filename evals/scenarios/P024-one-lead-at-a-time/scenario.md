# P024 Only one lead runs or merges tasks at a time
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user and autopilot, from S085, S088. Decision 019.

## Property
For any interleaving of autopilot and another lead session issuing `run`, `merge` and `discard`, no two leads start or merge tasks while both hold work; and after the driver dies, the user can run tasks again within one command.

## How it is checked
Seeded random interleavings with a killable stand-in driver. Prints the seed and the shortest failing interleaving.

## If this fails
Two leads fight over the same tasks and branch, or a crash locks the user out.
