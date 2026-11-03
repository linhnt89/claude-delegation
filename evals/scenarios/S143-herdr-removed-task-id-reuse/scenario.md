# S143 Distinguish a new task from a removed task with the same id
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer who finishes one task and later reuses its task id.

## Starting state
Task t-observe belongs to lead A and is selected in an isolated Herdr session. A second lead B uses the same checkout.

## Steps
1. Merge or discard t-observe through delegate and refresh the dashboard.
2. Create a different task with that id from B and observe it.
3. Reconnect the observer and inspect the task's parent, title and activity.

## Passes when
- The removed task stops appearing as live work.
- The new task belongs to B and shows its own title, attempt and activity, not A's cached data.
- Old selection or delayed observations do not silently turn the new task into the old task instance.
- The UI does not promise recoverable historical detail that delegate has removed.

## If this fails
A newly launched task inherits another task's ownership or apparent results.

## Failure paths
- Archived summaries, if a future policy permits them, must not masquerade as the current live task. No archive policy is selected here.
