# S042 Run upgrade with a delegate older than the project's kit
- Job: J1
- Tier: full
- Judged by: auto
- Status: automated

## Who
A developer on a second machine whose `delegate` binary wasn't rebuilt, in a project that was
already upgraded on the first machine.

## Starting state
A project whose recorded kit version is newer than the binary's.

## Steps
1. `delegate upgrade`.

## Passes when
- Nothing in the project changes.
- The message says the binary is older than the project's kit and gives the rebuild command.

## If this fails
A stale binary quietly rolls a team's project back to old skills.
