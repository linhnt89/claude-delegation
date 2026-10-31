# P026 No ambient variable outside the allowlist reaches a child command
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Property
For any variable name not on the allowlist (random names, `DELEGATE_*` names other than
`DELEGATE_HOME`, names that look like secrets) set in the caller's environment, the gate,
the Acceptance commands and the scenario checks that `delegate check` runs never see it; every
allowlisted variable that was set reaches them unchanged.

## Contract
- Seeded loop over generated names; each child writes `env` to a file; print the seed and the
  smallest failing name.

## If this fails
Some stray variable can still change a check's verdict.
