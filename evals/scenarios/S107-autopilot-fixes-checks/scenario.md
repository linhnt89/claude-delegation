# S107 Autopilot fixes a check bug on its own and says so in the report
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away, with autopilot running a wave whose implementer hit a check bug. Decision 025
(amends 019).

## Starting state
A mandate whose goal includes t003; the stand-in autopilot lead judges S001's failure a check
bug and runs `delegate fix-check S001 --from t003 --reason "…"`.

## Steps
1. `delegate autopilot start`, then let it stop.

## Contract
- The fix task runs (it is not refused as outside the mandate), on the writers' family.
- The run report lists it with S001 and the reason.

## Passes when
- A check bug doesn't stop autopilot or wait for the user.

## If this fails
Every check bug parks work until the user is back.
