# S140 Close the observer without stopping delegated work
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer hiding a dashboard while workers continue in the background.

## Starting state
A worker is running through delegate in an isolated Herdr session. Its task is visible in the right dashboard. Record the delegate task state independently through the CLI.

## Steps
1. Select tasks, expand and collapse groups, then close the observer view.
2. Use delegate status to confirm work continues and reopen the dashboard.
3. Restart only the observer component while the worker runs.

## Passes when
- Observation and view closure neither cancel nor restart the worker, initiate checks/retries/merges, or send worker input.
- Delegate remains the execution authority and the reopened dashboard reconciles with its current state.
- Observation requires no new worker access to Herdr control sockets or other tasks' private data, and no wider worker sandbox permissions.

## If this fails
A harmless viewing action can interrupt work or weaken the isolation I rely on.

## Failure paths
- An observer crash or unavailable UI leaves the worker running independently.
- Safety claims require execution/sandbox inspection as well as the visible UI walkthrough; a continuing worker alone does not prove isolation.
