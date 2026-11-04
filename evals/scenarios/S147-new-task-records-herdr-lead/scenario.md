# S147 A task created from a Herdr lead says which conversation created it
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer leading from Pi or Claude Code inside a Herdr pane, with `delegate` on PATH.
Their Herdr integration reports the lead's conversation identity on that pane.

## Starting state
A git repo of their own and a running Herdr endpoint (a stand-in socket, answering
`pane.get` like Herdr 0.9.5). Three lead panes:
- a Pi lead whose pane reports a path identity
- a Claude Code lead whose pane reports its session id, matching `CLAUDE_CODE_SESSION_ID`
- a Codex lead whose pane reports a Codex identity

A plain terminal outside Herdr is also used.

## Steps
1. From each lead pane, and from the plain terminal, run `delegate new` for a new task.
2. Run `delegate status --json`, then `delegate run` one Pi task with a stand-in worker
   and read status again.
3. Discard one task, and create a new task with the same id from the Claude lead.

## Passes when
- The Pi and Claude tasks show `origin` with support `supported` and that pane's
  conversation identity (source, agent, kind, value).
- The Codex task shows support `unsupported` and does not reveal the Codex value.
- The plain-terminal task shows `origin: null`, and every task, including that one, was
  created normally.
- Every task has an `instance`. Running the task leaves its origin and instance
  unchanged. The re-created id gets a new instance and the Claude lead's origin.
- The worker started by `delegate run` sees no `HERDR_*` variable.
- `session`/`session_name` are what they were before this feature.

## If this fails
Tasks can't be shown under the conversation that started them, or a worker could
overwrite the lead's identity, so the dashboard would group work under the wrong lead.

## Failure paths
- Herdr answers with an error, the wrong pane, an over-long or control-character
  identity, or not within a few seconds: no origin, a one-line note that says the task
  runs normally, and `new` still succeeds.
- A Claude pane whose identity disagrees with `CLAUDE_CODE_SESSION_ID`: no origin.
- `delegate new` run inside a worker sandbox never asks Herdr.

## Properties
- Origin and instance are set once, at creation, and nothing later changes them.
