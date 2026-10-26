# S035 `delegate watch` reads as a table, with trouble on top
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses, following several runs from a terminal.

## Starting state
Four tasks: one implementing with a worker that streams steps, one stalled, one finished and ready to merge with many changed files, one interrupted.

## Steps
1. `delegate watch`, piped and in a terminal (stopped after a few refreshes).
2. `delegate status` with no id.

## Passes when
- There is a header row naming the columns ID, STATE, WORKER, TIME, QUIET, FILES, and each column starts at the same place on every task's line.
- Tasks that need the user come first: stalled, interrupted or failed, then ready to merge, then running, then not started (id order within each group).
- A stalled task's line says STALLED and how long it has been quiet; stalled and interrupted tasks have a line under them, starting with `→`, with the commands to run.
- A running task has one line under it with its latest step.
- At 100 columns each task's main line fits on one line; a long file list ends with "+N more".
- In a terminal the states are coloured (the stalled one in a warning colour); piped output has no colour codes.
- `delegate status` with no id prints the same table.

## If this fails
With several tasks in flight the user has to read long run-on lines to find the one that stalled or died.

## Failure paths
- No tasks at all: still says so and how to start one, not an empty table.

## Properties
- None beyond P009 and S024 (each task keeps one line starting with its id, with phase, worker, times and a file name).
