# P030 Task attribution follows conversations, not panes or workers
- Job: J8
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer running multiple lead conversations in the same checkout.

## Starting state
An isolated observer setup with several conversations, reused panes, tasks and worker attempts. Based on agreed S137, S139, S142 and S143.

## Property
For any sequence of focus changes, pane reuse, conversation resume, retries and worker changes, each live task instance appears at most once in the sidebar and keeps its recorded launching conversation. A new task instance reusing an old id must not inherit the removed instance's attribution or activity. Missing conversation identity must never be replaced by a guessed conversation owner.

## Steps
1. Generate seeded action sequences through the real delegate and observer interfaces.
2. After each successful refresh, compare task instances and parent relationships with a simple model of their creation and removal.
3. Report the seed and smallest failing action sequence.

## Passes when
- Every generated sequence obeys the property; no unknown identity is silently assigned to a known conversation.

## If this fails
Reusing panes or retrying workers silently gives work to the wrong lead.

## Failure paths
- Same task ids in different checkouts must not alias.
- Exact missing-identity fallback and task-instance representation remain open design choices; this property does not prescribe them.
