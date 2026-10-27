# S060 The pane opens only when I ask for it
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer with two or more Claude Code sessions open on one project (desktop app or
terminal), each leading its own work.

## Starting state
A project set up with `delegate init`. Sessions A and B are open; neither has the delegate
pane open. No task is running.

## Steps
1. From session B, start a task with `delegate run --background`.
2. Look at session A and session B.
3. In A, open the pane (`/delegate-pane`, the row's button, or ctrl+x tab); merge or discard
   every task.

## Passes when
- Neither session's pane opens by itself when a task starts, in any session.
- Both sessions show the row above the prompt as soon as the task starts, in the desktop app
  and in the terminal. The tasks are shown once: a project set up by `delegate init` has no
  delegate status line repeating them (S031).
- The pane opens only when the user asks, and stays open until the user closes it, even once
  every task is merged or discarded (it then says there are no tasks).

## If this fails
Every delegation in one session pops a pane open in every other session, covering what the
user was doing there.

## Failure paths
- The user closes the pane while tasks run: it stays closed and the row comes back.

## Properties
- None.
