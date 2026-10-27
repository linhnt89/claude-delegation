# S064 In the desktop app the row uses all its room
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer leading from the Claude Code desktop app, with tasks in this session and in others.

## Starting state
As S061: this session has one task running and one ready to merge; session `pane-polish` has
two (one stalled); one task was started by hand. The app's row is about 65 code-font columns
wide, as the app reports it, but draws its text in a proportional font that takes less room.

## Steps
1. With the pane closed, look at the row above the prompt.

## Passes when
- The mod hands the app the whole row: this session's counts and every other group in full
  (`│ pane-polish 2 (1 stalled) · by hand 1`), with no `…` of its own, and the `Open tasks`
  button.
- Where the row really is too narrow, the app cuts the other groups at the edge, never this
  session's counts or the button.
- In the terminal, where columns are exact, the row is cut as before (S061).

## If this fails
In the desktop app the row stops at `· by …` with half of it empty, and with this session's
own counts there is no room left for other sessions at all.

## Failure paths
- None.

## Properties
- None.
