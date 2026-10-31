# S109 The user says "I'm off" and "back", and autopilot records it
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, with autopilot running on a mandate, telling the lead when they leave and return.
Decision 026.

## Starting state
A committed mandate; `delegate autopilot start` has run (stand-in headless lead).

## Steps
1. `delegate autopilot status`, then `delegate autopilot away`, `status`, `delegate autopilot here`, `status`.

## Contract
- Right after `start`, `status` shows `present` (and `--json` has `"presence": "present"`).
- `away` and `here` exit 0; `status` then shows `away`, then `present`.
- With no autopilot run, `away` exits 1 and says to start autopilot first.
- Presence never changes on its own (idle time, no session).

## Passes when
- Presence is exactly what the user last said.

## If this fails
Work lands on main under a user who left, or questions are held from a user who is there.
