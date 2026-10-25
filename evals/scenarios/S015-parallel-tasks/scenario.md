# S015 Two tasks run at the same time without stepping on each other
- Job: J5
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
Two ready specs changing different files.

## Steps
1. Start `delegate run t001` and `delegate run t002` at the same time.
2. `delegate list`.
3. Merge both.

## Passes when
- Both runs finish with their own commits; neither sees the other's changes.
- `list` shows both tasks with the right round and checks.
- Both merge, and main has both changes.

## If this fails
Running tasks in parallel, one of the main reasons to use several models, corrupts tasks or state.

## Failure paths
- None beyond the above.

## Properties
- None.
