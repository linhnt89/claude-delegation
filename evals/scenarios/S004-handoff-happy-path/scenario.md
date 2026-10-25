# S004 Hand off a ready task and merge the result
- Job: J2
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A repo with code and tests; spec t001 with Goal, Scope and an acceptance command that passes once the change is made. A worker that does the job.

## Steps
1. `delegate new t001`, fill the spec.
2. `delegate run t001`.
3. `delegate list`.
4. `delegate merge t001`.

## Passes when
- The run ends with the worker's commit on the task branch and its checks passing.
- `list` shows the round, tier, model and checks passed.
- After merge, main has one new commit titled `t001: <title>` with the Goal as its body, containing the change.

## If this fails
The main promise of the kit, give a task to a model and get a checked change back, doesn't hold.

## Failure paths
- Merge run before any run: refused, saying there is nothing checked to merge.

## Properties
- Merge never succeeds unless acceptance checks passed on the final commit.
