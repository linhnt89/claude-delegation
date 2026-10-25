# S032 Claude is woken when a background run finishes
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, who started a run with `delegate run --background` and wants to be told when it ends instead of polling.

## Starting state
A ready spec and a worker that takes about 10 seconds and passes its checks.

## Steps
1. `delegate run --background t001`, then `delegate wait t001`.

## Passes when
- `wait` returns within 5 seconds of the run finishing, not before, exit 0.
- It prints one line naming the task, what happened (done, pass) and the next command (`delegate merge t001`).
- With no tasks running, `wait` returns at once and says nothing is running.
- The delegate skill tells Claude to start `delegate wait` in the background after `run --background`.

## If this fails
Claude has to poll or be told by hand when work finishes, so finished work sits unreviewed or Claude spends its turns checking.

## Failure paths
- An unknown task id: a clear error naming the known tasks, non-zero exit.

## Properties
- Waiting never changes anything (hidden P011).
