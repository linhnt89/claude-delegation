# 003: Claude waits on background runs with `delegate wait`

- Date: 2026-10-26
- Status: accepted

## Context
With `run --background` (decision 002), Claude as lead only learns how a run is going when it
runs `delegate status`, which it does only during its own turns. A run that finishes, dies or
stalls between turns goes unnoticed. Polling on a timer costs a Claude turn per poll, mostly
to hear "still running".

## Decision
`delegate wait [<id>...] [--timeout <duration>]` blocks until one of the given tasks (all
running tasks when none are given) finishes, is interrupted, or is flagged stalled; prints one
line per event (task, what happened, the next command or the last step) and exits 0. With
nothing running it returns at once and says so. `--timeout` ends it with exit 3 and "nothing
happened yet". It only reads status, like the other views.
The delegate skill tells Claude to start `delegate wait <id>` as a background command right
after `delegate run --background <id>`; the harness wakes Claude when it exits. Claude then
reviews, investigates (stall: last step, processes, log tail) or waits again.

## Rejected options
- A timer that polls `status` every few minutes — a Claude turn per poll, mostly for nothing.
- Leaving it to each lead session to build its own monitor — every session re-invents it, and
  most won't.

## Consequences
One wake-up per real event. A false stall (a long build, long thinking) still wakes Claude;
the skill says how to tell it apart and wait again.
