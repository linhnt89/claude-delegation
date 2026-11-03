# S138 Inspect a task without replacing its lead conversation
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer discussing work with a lead while checking its workers.

## Starting state
An isolated Herdr session shows a lead on the left, with two delegated tasks beneath it in the sidebar and task observations on the right.

## Steps
1. Select one task in the sidebar, then the other.
2. Expand and collapse their activity in the dashboard.
3. Continue typing in the lead conversation.

## Passes when
- Selecting a task shows that task's details on the right and keeps its launching lead visible on the left.
- Task selection never replaces the lead with a worker terminal or interactive worker session.
- Task title, delegate phase/outcome, worker, and available read/edit/run activity identify the selected task.
- Typing in the lead does not send input to a worker.

## If this fails
Watching workers disrupts my conversation or sends input to the wrong process.

## Failure paths
- Selecting a task whose data disappeared shows an unavailable state rather than another task's details.
- A task without activity is not presented as a failed or completed worker solely because its activity list is empty.
