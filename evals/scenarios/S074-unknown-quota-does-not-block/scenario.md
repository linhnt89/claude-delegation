# S074 When quota can't be read, the run goes on and says so
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Claude as lead, on a machine where one login is expired or a vendor is down. Decision 014.

## Starting state
The first candidate's vendor stand-in times out (or its login is expired); there is no other
problem.

## Steps
1. `delegate run t001`.

## Passes when
- The run warns that the account's quota is unknown, why, and uses the candidate anyway.
- The check gives up within a few seconds, so the run isn't held up.
- The login file is unchanged.

## If this fails
A broken quota reading blocks work that would have run fine, or hides that the check failed.

## Failure paths
- None beyond the above.
