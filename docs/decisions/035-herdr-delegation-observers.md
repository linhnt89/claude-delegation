# 035: Observe delegated tasks in Herdr, grouped by lead session

- Date: 2026-11-04
- Status: accepted

## Context
The kit supports leads using different models and harnesses. Users running those
leads in Herdr want to watch delegated work without leaving the lead conversation.
The existing Claude-specific pane integration is not sufficient. Research in
`docs/research/herdr-delegation-observers.md` identifies available APIs and gaps;
source-level feasibility has been checked, but runtime behavior needs a prototype.

## Decision
Target `herdrdev/herdr` plus `penso/herdr-gpui`, the user's installed stack. Changes
to both projects are acceptable; migration to the separate `herdr-core` and
`herdr-app` projects is not part of this work.

Keep the active lead conversation on the left. Show a read-only delegated-task
dashboard on the right, grouped by the lead conversation that started each task.
Nest task entries beneath their lead in the sidebar. Selecting a task keeps its
lead visible and opens that task's details on the right. A task remains one entry
across worker retries and model changes; tasks are not interactive worker sessions.

`delegate` continues to own worker execution, sandboxing, checks, retries, and task
lifecycle. Add model-neutral, persistent launching-session attribution rather than
infer ownership from a checkout name or a pane's current occupant. Closing an
observer must not cancel work or send input to a worker.

Start from existing task status and normalized read/edit/run activity. Define user
acceptance scenarios, then validate attribution, grouping, and selection in an
isolated prototype before committing to daemon protocol changes. Keep interactive
worker sessions out of this phase.

## Rejected options
- Run workers through a different backend merely to obtain a parent tree — changes
  execution and lifecycle rather than adding observation.
- Treat the exact UI as configuration-only or an existing Luau mod — current mod
  data lacks delegate tasks and lead/task relationships.
- Automatically adopt the separate `herdr-app` implementation — it is not the
  installed GPUI client; use its parent-tree implementation only as a reference.

## Consequences
Implementation spans delegate attribution and a trusted GPUI task-data adapter and
UI. Herdr extensions are permitted, but their necessity and contracts remain to be
validated. Worker control sockets and sandbox access must not be expanded for the
viewer. Identity fallback, visibility scope, and history after merge/discard or lead
closure remain design questions; current attempt-local streams do not supply full
round history or permanent completed-task records.
