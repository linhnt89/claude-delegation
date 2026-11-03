# S137 See each lead's delegated tasks beneath its conversation
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer using different lead harnesses in the installed Herdr desktop client.

## Starting state
An isolated Herdr session has two lead conversations in the same checkout and tasks with similar titles. Repeat with Pi, Codex and Claude leads whose integrations report conversation identity. Workers may use different harnesses from their leads.

## Steps
1. Launch tasks from both leads through delegate.
2. Open the task dashboard and inspect the sidebar.
3. Switch between the leads while their workers report activity.

## Passes when
- Sidebar tasks nest beneath the conversation that launched them, not merely their checkout or worker harness.
- The right dashboard groups tasks by launching conversation and distinguishes the two leads even if their titles match.
- Switching focus or changing a worker does not move tasks between leads.
- Attribution works for all three lead harnesses, without a Claude-only ownership requirement.

## If this fails
I cannot tell which conversation owns the work and may review the wrong task.

## Failure paths
- Missing conversation identity is covered by S142; do not infer ownership from the active pane or checkout name.
