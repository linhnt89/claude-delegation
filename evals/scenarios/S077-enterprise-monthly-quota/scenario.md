# S077 A Claude Enterprise account shows its monthly spend cap
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer whose Claude Code login is a Claude Enterprise seat. The seat has no 5-hour or
weekly limit, only a monthly spend cap set by their organisation. Reported 2026-10-28: setup
said "unknown: vendor answer had no quota windows … log in again with claude auth login"
right after a fresh login.

## Starting state
A Claude Code login in `~/.claude/.credentials.json`. The usage endpoint answers the
Enterprise shape: `five_hour`, `seven_day` and `seven_day_oauth_apps` null, `limits` empty,
`extra_usage` enabled with $225.50 of a $500 monthly limit used (`utilization` 45.1,
`spend_limit_reached` false), and the same figures under `spend`.

## Steps
1. `delegate quota`.
2. `delegate quota --json`.

## Passes when
- The Claude line shows a monthly window with about 55% left, not "unknown".
- The JSON has the Claude account `ok` with that monthly window.
- Nothing tells the user to log in again.

## If this fails
An Enterprise user sees their quota as unknown and is told to log in again, which doesn't
help. Runs can't steer away from a seat that is nearly out of its monthly budget.

## Failure paths
- The monthly cap is used up (`spend_limit_reached` true): the window shows 0% left.
- A Pro or Max account that has plan limits and also extra usage turned on: its 5-hour and
  weekly windows are what it shows. The extra-usage budget isn't shown as a monthly window,
  because it only applies once the plan limits run out.
- A 200 answer with nothing readable in it: Claude is unknown, but the message doesn't say to
  log in again, because the login worked.
- A 401 answer: still unknown, and it does say to log in again with `claude auth login`.
