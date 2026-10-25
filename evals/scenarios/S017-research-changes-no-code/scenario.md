# S017 A research run returns a report and changes no code
- Job: J6
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A repo with code; a research spec asking a question about it.

## Steps
1. `delegate run t001 --research`.

## Passes when
- The report is shown or its path printed.
- The worktree has no code changes and nothing is committed.

## If this fails
A research question turns into unrequested code changes, or the answer is lost.

## Failure paths
- None beyond the above.

## Properties
- None.
