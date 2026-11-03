# S142 Do not give a new conversation the previous lead's tasks
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer starting and resuming conversations in reused Herdr panes.

## Starting state
Conversation A has a running task. The isolated session also permits a lead without a reported conversation identity. Exact fallback and closed-lead presentation remain design questions.

## Steps
1. Start conversation B in A's former pane and launch a separate task.
2. Resume A in another pane.
3. Launch a task where no conversation identity is available, and inspect its attribution.

## Passes when
- B does not acquire A's tasks merely by occupying the same pane.
- Resuming A preserves its existing task attribution rather than creating another owner for those tasks.
- Missing identity is clearly indicated; tasks are not silently attributed to another conversation by checkout, pane reuse or model name.
- Lack of observer identity does not prevent delegate from running the task.

## If this fails
Reusing a pane silently mixes conversations' work, and I cannot trust ownership labels.

## Failure paths
- A missing or closed lead does not cause an active task to be assigned to whichever lead is visible now.
- These requirements do not choose an identity fallback scheme or a retention policy for closed leads; those still need agreement before the prototype identity contract.
