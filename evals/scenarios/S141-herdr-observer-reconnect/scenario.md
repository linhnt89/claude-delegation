# S141 Recover an accurate task view after losing the connection
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer returning to the dashboard after its data connection was interrupted.

## Starting state
An isolated Herdr session displays tasks from two leads in one checkout; one task is selected.

## Steps
1. Interrupt the observer's data connection without stopping delegate workers.
2. While disconnected, retry one task, create another and merge or discard a third through delegate.
3. Restore the connection and compare the dashboard with current delegate status.
4. Refresh or reconnect once more.

## Passes when
- Disconnected data is visibly stale or unavailable rather than presented as confirmed live state.
- After a successful refresh, current tasks, owners, phases and attempts agree with delegate without duplicate rows.
- Removed tasks are no longer presented as live; an unavailable selection does not silently select unrelated work.
- A second reconnect does not resurrect removed tasks or older attempts.

## If this fails
The dashboard confidently shows work that no longer exists or hides work that needs my attention.

## Failure paths
- A failed refresh preserves an explicit unavailable/stale state instead of treating an error as an authoritative empty task list.
- This scenario requires no permanent task archive after merge/discard.
