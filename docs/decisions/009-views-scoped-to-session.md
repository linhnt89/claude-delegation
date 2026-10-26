# 009: The status line and pane are scoped to the Claude Code session

- Date: 2026-10-27
- Status: accepted

## Context
Several Claude Code sessions often lead the same project at once, some from worktrees. The
status line and the side pane showed every task in the project in every session, by id only,
so a user couldn't tell which tasks were theirs or what a task was doing.

## Decision
- A task belongs to the Claude Code session that ran `delegate new` for it, taken from
  `CLAUDE_CODE_SESSION_ID`. A task with no owner (created outside Claude Code) is claimed by
  the first session that runs it. Running, checking, reviewing or merging from another session
  never moves it. A resumed session keeps its id; a fork has a new one.
- A session is named by its title when known, else by its folder or branch. Tasks with no
  session are grouped as `by hand`.
- `status --json` gives every task `title` (spec's first line), `goal` (first Goal line),
  `session` and `session_name`.
- Status line: this session's tasks as before, then one dimmed count per other group:
  `t021 implement · 1 ready to merge │ pane-polish 2 (1 stalled) · by hand 1`. With no
  `session_id` on stdin (older Claude Code) it shows every task, as before.
- Pane: every task keeps the same full card and its status colour. Each session has a one-line
  divider, this session's first (`◆ ── This session · <name> ──`), others dimmed
  (`○ ── <name> ──`), `by hand` last. `◆`/`○` before each id; the title after it and the Goal's
  first line below, dimmed. The `┃` bar marks only stalled, interrupted or failed tasks.

## Rejected options
- Owner is whichever session ran the task last — tasks would jump between sessions whenever
  another one retries or checks them.
- Hiding other sessions' tasks — trouble in another session would go unseen.
- One-line rows for other sessions' tasks — the user preferred the same card for every task,
  with dividers and markers to tell sessions apart.

## Consequences
Task state records an owner. Views need the viewer's session id: the status line reads it from
stdin, the pane from `$.session.id()`. A session's title isn't reliably known to the CLI or the
mod, so the folder or branch is the name users usually see. Checks run from a Claude Code
session must unset `CLAUDE_CODE_SESSION_ID` (`evals/lib/common.sh` does).
