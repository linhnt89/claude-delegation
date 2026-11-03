# S139 Follow a task across retries and worker changes
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer monitoring a task that needs another implementation attempt.

## Starting state
A delegated task is selected in an isolated Herdr session. Its current worker has emitted read/edit/run activity.

## Steps
1. Let the attempt fail checks, then retry through delegate with a different worker model.
2. Observe the sidebar and selected details while the new attempt starts and emits activity.
3. Let delegate finish checks or report a blocked/interrupted outcome.

## Passes when
- The task remains one sidebar entry beneath its original lead; selection stays on that task.
- The displayed worker and phase follow delegate, including checking, blocked, interrupted and ready-to-merge states when exercised.
- Reset step identifiers do not attach the previous attempt's activity or command result to the new attempt.
- Only available activity is shown; missing historical streams and command stdout are not invented or promised.
- A worker exiting or a terminal becoming idle is not itself reported as task success.

## If this fails
Retries look like new tasks, or stale activity makes me believe the wrong work passed.

## Failure paths
- Delayed observations from an older attempt must not overwrite the current attempt's state.
