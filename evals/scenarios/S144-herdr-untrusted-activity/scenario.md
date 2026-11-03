# S144 Inspect noisy or malformed worker activity safely
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer monitoring workers whose output and task text cannot be trusted as UI instructions.

## Starting state
An isolated observer setup has a healthy task and another producing oversized titles/targets, terminal control sequences, shell-looking strings and malformed activity records through the task-data interface.

## Steps
1. Open the dashboard and select both tasks in turn.
2. Expand the noisy task's activity and continue interacting with the lead.
3. Send valid activity after the malformed records and refresh.

## Passes when
- Task text is displayed as inert text, not executed as shell commands or interpreted as terminal/UI controls.
- Oversized or malformed data does not make the lead or healthy task unusable; any truncation or unavailable detail is clear.
- Later valid observations remain visible or recover on refresh without restarting workers.
- The default view shows normalized activity, not unrequested raw streams or private logs.

## If this fails
Watching an unreliable worker can freeze my workspace, mislead me or execute its text.

## Failure paths
- An unreadable task record is isolated rather than turning the entire dashboard into a false empty or successful state.
- Passing requires checking the real observer interface, not merely drawing a synthetic mockup with safe text.
