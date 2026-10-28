# S067 The user decides how many failed rounds come before a stronger model
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer who wants the same tier to get a second try before paying for a stronger one.
Decision 013.

## Starting state
`workers.toml` has `step_up_after = 2` under `[defaults]`. A task at standard whose work fails a
check every round.

## Steps
1. `delegate run t001` three times, without `--tier`.

## Passes when
- Round 2 runs at standard and says it is the 2nd try at that tier after 1 failed round
  (of the 2 allowed).
- Round 3 runs at deep and says it stepped up after 2 failed rounds at standard.
- With no `step_up_after` in the config, the second round steps up, as before.
- `step_up_after = 0` or a non-number is refused with a message naming the setting, the file
  and what values work.

## If this fails
The user can't trade a cheap second try against an expensive model, so a one-off slip always
costs a stronger, pricier run.

## Failure paths
- A check-error round in between doesn't count toward the failures and doesn't reset them.

## Properties
- See P019.
