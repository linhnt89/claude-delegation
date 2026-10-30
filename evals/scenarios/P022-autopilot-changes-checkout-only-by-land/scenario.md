# P022 Autopilot changes my checkout only through land
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, from S083, S085, S088. Decision 019.

## Property
For any sequence of autopilot start, stop, kill, restart and task outcomes (pass, fail, question, crash), the user's checkout and base branch are byte-for-byte unchanged until `delegate autopilot land`.

## How it is checked
Seeded random action sequences with the stand-in lead and workers; after each action compares the checkout and the base ref with the starting state. Prints the seed and the shortest failing sequence.

## If this fails
Autopilot touches the user's work behind their back.
