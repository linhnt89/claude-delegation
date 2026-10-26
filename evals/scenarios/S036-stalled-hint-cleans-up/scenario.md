# S036 The commands under a stalled task actually clear it away
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated
- Timeout: 120

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses, who sees a task flagged as stalled and wants it gone.

## Starting state
A background run whose worker makes one edit, then stays alive doing nothing past the (shortened) stall threshold.

## Steps
1. `delegate status` and `delegate status t001` show the stalled task with a `→` line under it.
2. Run each command on that line, exactly as printed.
3. Run the command the `→` line then offers for the interrupted task to throw it away.
4. `delegate list`.

## Passes when
- The `→` line under the stalled task reads `delegate status t001  ·  kill <pid>`, with the pid of the task's run, and `delegate status --json` gives the same commands as the task's `next` (the side pane shows it from there).
- `delegate wait t001`, which returns on the stall, names the same commands.
- Each command on the line succeeds; after the `kill`, the task shows as interrupted within a few seconds, with `→ delegate run t001 or delegate discard t001`.
- `delegate discard t001` succeeds and `delegate list` no longer shows the task.

## If this fails
The user follows the tool's own advice on a hung worker and is refused ("t001 is running"), so they have to work out by themselves how to get rid of it.

## Failure paths
- Discard is still refused while the run is alive; only the hint changes.

## Properties
- None.
