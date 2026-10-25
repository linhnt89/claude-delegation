# S023 See the worker's steps while it works
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A ready spec and a stand-in worker that streams events in a real harness's JSON format (one stand-in per supported harness): it reads two files, edits one, runs a test command that fails, edits again, runs it and passes.

## Steps
1. `delegate run --background t001`.
2. `delegate status t001` while it runs, and once after.

## Passes when
- The steps appear in order as one line each: what kind (read, edit, run, think, message), on what (file or command), and for a command its result.
- A step shows up in `status` within a few seconds of the worker doing it.
- After the run, the full step list is still there to read.

## If this fails
The user sees that a worker is busy but not what it is doing, so they can't tell a worker going in circles from one making progress.

## Failure paths
- None beyond P010 (bad or unknown worker output).

## Properties
- Any worker output, however broken, still gives a working view (P010).
