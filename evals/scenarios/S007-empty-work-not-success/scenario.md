# S007 A worker that produces nothing is not reported as done
- Job: J2
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A ready spec; a worker that exits 0 without changing anything.

## Steps
1. `delegate run t001`.
2. `delegate list`.
3. `delegate merge t001`.

## Passes when
- The run says plainly that no change was made.
- `list` doesn't show the task as passed.
- Merge is refused.

## If this fails
The kit claims a task is done when nothing was done, which is the fastest way to lose the user's trust.

## Failure paths
- A worker that crashes halfway: same, never shown as passed.

## Properties
- None.
