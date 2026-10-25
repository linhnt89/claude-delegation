# S016 Two tasks change the same file: the second merge reports it, nothing is lost
- Job: J5
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
Two finished tasks that both changed the same lines.

## Steps
1. Merge t001.
2. Merge t002.

## Passes when
- The second merge is refused with a message naming the conflict and what to do.
- Main has t001's change and no conflict markers; t002's work is still in its task.

## If this fails
A conflicting merge leaves main broken or silently drops one task's work.

## Failure paths
- None beyond the above.

## Properties
- None.
