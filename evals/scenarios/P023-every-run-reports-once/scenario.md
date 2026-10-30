# P023 Every autopilot run ends with one notification and one report
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, from S083, S086, S089. Decision 019.

## Property
For any stop reason (goal met, all parked, gate broken, quota floor, wave limit, `autopilot stop`), each run makes exactly one notify attempt and writes exactly one run report naming that reason.

## How it is checked
Seeded random plans and failure points; counts notify calls and reports per run. Prints the seed.

## If this fails
The user comes back to no news, or to a phone full of repeats.
