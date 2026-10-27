# 012: Tasks show once, in the row above the prompt; the pane opens on request

- Date: 2026-10-27
- Status: accepted

## Context
Trying decision 009 with two sessions in the desktop app showed three problems. The desktop
app doesn't run Claude Code's status line, so the row above the prompt (the pane mod's) was
the only line there, and it counted this session's tasks only: other sessions were invisible
until the pane was opened. In the terminal the status line and the row showed the same counts
twice. The pane opened by itself in every open session whenever any session started a task.
And with `delegate status` slow (S059), the mod started a new read every 3 seconds without
waiting for the last one; reads piled up, timed out, and the row said "status unavailable".

## Decision
- Tasks show once, in the row above the prompt, in the desktop app and in the terminal. The
  row gives this session's counts, then the other groups dimmed with the status line's groups
  and counts (`│ pane-polish 2 (1 stalled) · by hand 1`); a narrow row cuts the other groups
  first. With no session id, every task counts as this session's.
- `delegate init` adds no status line. It says in one line that `delegate statusline` can be
  added to a status line. `delegate statusline` stays, unchanged, for those who want it.
- `delegate upgrade` removes a status line whose command is exactly `delegate statusline` (the
  one older kits added) and says so; any other status line is left alone, with a line why.
- The pane opens only when the user asks (`/delegate-pane`, the row's button, ctrl+x tab) and
  closes only when the user closes it. It never opens or closes by itself.
- The mod runs at most one `delegate status` at a time. While one is running, the pane and the
  row keep the last result, and `/delegate-pane` opens the pane at once with it. Only a read that
  fails or gives up shows `delegate: status unavailable`.

This changes decision 002 (init installs the status line) and decision 009 (the row counts
only this session's tasks; the status line is where other sessions show).

## Rejected options
- Status line in the terminal, row in the app — the mod can't tell whether delegate's status
  line is set up; users who kept their own status line (S031) would see no tasks in the terminal.
- Both, as before — the same counts twice in the terminal.
- Opening the pane by itself only in the session that started the task — the user still
  didn't ask for it; the row already tells them a task started.

## Consequences
Users of the desktop app see other sessions' trouble without opening the pane. Projects set up
by older kits lose the duplicate status line on their next `delegate upgrade`. The pane checks
(S060–S062) run the installed mod through `claude plugin test` with a stand-in host
(`evals/lib/mod.sh`, `mod-host.tsx`), so they need Claude Code on PATH.
