# S024 See all tasks at once
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
Four tasks: one implementing, one running checks, one finished and ready to merge, one interrupted.

## Steps
1. `delegate watch` (stopped after a few refreshes).
2. `delegate status` with no id.

## Passes when
- Each task is one row: id, phase, worker, elapsed time, time since the last activity, files changed.
- The finished task says it is ready to merge; the interrupted one says interrupted and what to do (discard or run again).
- `watch` refreshes by itself: a step or phase change shows up without re-running it.
- `watch` works in a plain terminal and exits cleanly on Ctrl-C, leaving the terminal usable.

## If this fails
With several tasks in flight the user loses track of which need attention.

## Failure paths
- No tasks at all: says so and how to start one, instead of an empty table.

## Properties
- None beyond P009.
