# S034 wait doesn't return while a task is just busy, and can time out
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, waiting in a background command that its harness may kill after a while.

## Starting state
A background run whose worker keeps changing files for 30 seconds.

## Steps
1. `delegate wait t001` for 15 seconds.
2. `delegate wait t001 --timeout 5`.

## Passes when
- `wait` doesn't return while the worker is busy.
- With `--timeout 5` it returns after about 5 seconds with its own exit code (3), saying nothing happened yet, so the caller can tell a timeout from an event.

## If this fails
Claude gets woken for nothing, or its waiting command is killed mid-way with no answer.

## Failure paths
- None beyond the above.

## Properties
- None.
