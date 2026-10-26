# S057 The pane groups tasks by session and marks mine
- Job: J8
- Tier: full
- Judged by: human
- Status: agreed

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses. They lead from Claude Code, often with two or more sessions open on the same project (some in worktrees).

## Starting state
As in S055, seen from session A, in the Claude Code desktop app and in the terminal.

## Steps
1. Open the delegate pane in session A; watch it for a minute in light and dark mode.
2. Do the same in session B.

## Passes when
- Each session's tasks sit under a one-line divider naming it. This session's comes first,
  bright, marked `◆`: `◆ ── This session · <name> ──`. Each other session follows dimmed,
  marked `○`: `○ ── pane-polish ──`. Tasks started outside Claude Code come last under
  `○ ── by hand ──`.
- Every task keeps the same full card and its status colour in every group. The card's second
  line is `◆` or `○`, the id, the title, then the worker; below it, dimmed, the first line of
  the Goal. Lines are cut to the pane's width with `…`.
- Within a group, tasks that need the user come first, as in S030.
- The coloured `┃` bar marks only stalled, interrupted or failed tasks, in any group.
- The row above the prompt (pane closed) counts this session's tasks the same way as the
  status line.

## If this fails
The user can't tell at a glance which tasks are theirs, or loses sight of trouble in another
session.

## Failure paths
- Only one session has tasks: its divider still shows, so the user knows whose they are.

## Properties
- None.
