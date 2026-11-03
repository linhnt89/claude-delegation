# P032 A successful refresh converges to current delegate state
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer relying on a dashboard despite missed updates and worker retries.

## Starting state
An isolated observer setup with task creation, retries, merge/discard, id reuse and interruptible data reads. Based on agreed S139, S141 and S143. Visibility is bounded to explicitly registered test checkouts; this does not define production visibility scope.

## Property
After data changes stop and an authoritative refresh succeeds, the observer's live task set, attribution, phases, workers and current-attempt activity agree with delegate's available state. Repeating the refresh without source changes leaves that view unchanged. Failed reads must not be interpreted as an authoritative empty task set, and stale observations must not resurrect removed instances or replace a newer attempt.

## Steps
1. Generate seeded sequences of lifecycle actions and connection interruptions through real interfaces.
2. Restore the connection, quiesce source changes and request reconciliation.
3. Compare the public observer read model with independent delegate reads and repeat reconciliation; report the seed and smallest failing sequence.

## Passes when
- All generated sequences converge and repeated refreshes preserve the same live view.
- Failed refreshes explicitly show stale/unavailable data, not confirmed current emptiness.

## If this fails
A reconnect can leave me confidently watching obsolete work or missing a task that needs attention.

## Failure paths
- Attempt-local step id resets must not mix old command results into a new attempt.
- No archived transcript or retained completed-task policy is implied.
