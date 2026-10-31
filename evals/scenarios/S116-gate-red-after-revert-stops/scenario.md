# S116 A gate still red after the revert stops the run
- Job: J10
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
The user, away, when the gate is broken by something other than the merge. Decision 026.

## Starting state
A mandate whose gate fails always, whatever is merged.

## Steps
1. `delegate autopilot start`; let it stop.

## Contract
- The gate is rerun once, the last merge is reverted, the gate still fails: the run stops with
  stop reason `gate_failed`.
- The report says the gate stayed red after the revert, so the cause isn't the merge, and
  shows the gate output.
- One notification is sent.

## Passes when
- Autopilot doesn't revert its way through the whole run.

## If this fails
A broken environment eats all the merged work one revert at a time.
