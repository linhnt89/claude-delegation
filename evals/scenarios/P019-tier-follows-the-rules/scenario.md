# P019 A task's tier only goes up, and only after enough product failures
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Anyone running tasks without `--tier`. From: S013, S066, S067. Decision 013.

## Property
For any `step_up_after` from 1 to 3 and any sequence of round results (pass, product failure,
check error, worker couldn't start), the tier of each round is never lower than the round
before, and it moves one tier up exactly when the rounds at the current tier, since its last passing round, include
`step_up_after` product failures (check errors and rounds that didn't start don't count),
up to deep, where it stays.

## How it is checked
Seeded random sequences of stand-in workers per round, read from what `delegate run` prints
and `delegate list`. Prints the seed and the shortest failing sequence.

## If this fails
Tasks land on the wrong model: either the expensive one too early, or a weak one forever.
