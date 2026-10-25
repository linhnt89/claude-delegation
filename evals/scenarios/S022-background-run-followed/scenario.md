# S022 Start a run in the background and follow it to the end
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A ready spec `t001` and a worker that takes about 20 seconds, editing two files.

## Steps
1. `delegate run --background t001` from a shell, then close that shell.
2. `delegate status t001` a few times while it runs.
3. `delegate status t001` after it finishes.

## Passes when
- `run --background` returns within a few seconds and prints the task id and the command to follow it.
- The run keeps going after the shell that started it is gone.
- While it runs, `status` shows the phase (questions, implement, checks...), the worker and tier, the elapsed time and the files changed so far.
- After it finishes, `status` shows the outcome (passed / failed checks) and what to do next (merge, review or another round).

## If this fails
The user can hand off work but can't see where it stands without digging through folders, or the work dies when the terminal or Claude turn that started it ends.

## Failure paths
- `run --background` on a task that can't start (no harness, no bwrap) fails in the foreground with the same clear message as a normal run, and leaves nothing running.

## Properties
- What `status` shows matches what is really happening (P009).
