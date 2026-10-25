# S028 A background run that dies shows as interrupted
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A background run in its implement phase.

## Steps
1. Kill the background process (and its worker).
2. `delegate status t001`, `delegate watch`, `delegate list`.

## Passes when
- All three show the task as interrupted within a few seconds, never as running, stalled or passed.
- They say what to do: `delegate run t001` again or `delegate discard t001`.

## If this fails
A dead run looks alive (or stalled forever), so the user waits for work that will never come.

## Failure paths
- The machine rebooted: same result after the reboot.

## Properties
- Covered by P009.
