# S072 A run skips a model whose account is about to run out
- Job: J4
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead starting a task. Decision 014.

## Starting state
The tier's first candidate is on an account the stand-in vendor reports as nearly exhausted
(at its pace it runs out in 10 minutes); the second candidate's account is healthy.

## Steps
1. `delegate run t001 --dry-run`, then `delegate run t001`.

## Passes when
- Both say they skip the first candidate, naming its account, what's left and when it resets
  or runs out.
- The task runs on the second candidate; the first is never started.
- With quota checks turned off in `workers.toml`, the first candidate runs as before.

## If this fails
Work is handed to a model that will stop partway, and the user has to restart it by hand.

## Failure paths
- See S074 (unknown quota) and S076 (nothing left).
