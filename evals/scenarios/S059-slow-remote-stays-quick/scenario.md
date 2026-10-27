# S059 delegate stays quick when the vault's remote is slow
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated
- Timeout: 300

## Who
Claude as lead, and the developer, on a machine whose vault is connected to a remote, with other
Claude Code sessions open on the same machine (their status lines refresh all the time).

## Starting state
A project with a ready task `t001` and a stand-in worker. The vault has a remote that takes
3 seconds to answer every fetch or push. Two other processes run `delegate list` in a loop, as
other sessions' commands would.

## Steps
1. `delegate statusline`, as Claude Code calls it, several times.
2. `delegate list` and `delegate status t001`.
3. `delegate run t001 --background`, then `delegate wait t001`.

## Passes when
- Every `delegate statusline` answers in under half a second.
- `delegate list` and `delegate status t001` each answer in under 2 seconds, although the remote
  takes 3 and other sessions are calling `delegate` at the same time.
- `run --background` says the task is running, and `wait` reports it done.

## If this fails
Every `delegate` command waits on the network and on other sessions, so the status line lags,
checking a task takes many seconds, and background runs fail to start at all.

## Failure paths
- The remote is unreachable: the same limits hold, and `delegate list` names a hidden hand
  edit as not on the remote yet.
- None of steps 1–3 contacts the remote at all (decision 011).
- Once the remote is quick again, one `evals/run` leaves the vault in sync with it.

## Properties
- None.
