# S151 Open the side pane from the lead and inspect a task with the keyboard
- Job: J8
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer leading from Pi inside Herdr who wants to see their delegated tasks next to
the conversation without leaving it.

## Starting state
A git repo with tasks created by this lead: one running with read/edit/run activity, one
ready to merge, one not started. Another lead's task is in the same checkout. One task
title contains terminal escape sequences and a shell-looking string. A stand-in Herdr
socket answers `pane.get`, `pane.split` and running a command in a pane, and records them.

## Steps
1. From the lead's pane, run `delegate watch --beside`.
2. Start the command Herdr was asked to run, in a terminal, as the new pane would.
3. Press `↓` to select the running task (rows are in attention order: ready, running, waiting), `enter` to expand it, `enter` again to collapse,
   `tab` to the other groups, then `q`.
4. Run `delegate watch --sidebar-config`.

## Passes when
- `--beside` asked Herdr to split the lead's pane to the right without focusing it, with
  the lead's checkout as the new pane's working directory, and printed the new pane id.
  Herdr 0.9.5 runs a command by typing it into the new pane's shell, so the typed text is
  a fixed command with no paths or task text. The lead pane and the delegate binary
  reach it through the split's environment. Nothing was sent to the lead's pane.
- The side pane's screen names this lead and lists only its three tasks first, with the
  other lead's task only as a count.
- After `↓` and `enter`, the running task's steps (read/edit/run with targets), worker
  and next step are shown. A second `enter` hides them.
- The escape sequences and shell-looking title are shown as inert text (no terminal
  control reaches the screen) and nothing is executed.
- `q` exits 0 with the terminal restored (echo and line mode back).
- `--sidebar-config` prints TOML that parses, with `rows_by_agent` for `pi` and `claude`
  using `$dlg_t1`…`$dlg_t4` and `$dlg_more`, and colour rules that start with the state.
  It says to paste it into Herdr's `config.toml` and that delegate never writes it.

## If this fails
The developer can't open the task view next to their lead, can't see what a task is
doing, or a task's text could mess up their terminal.

## Failure paths
- Run outside Herdr: `--beside` refuses, saying to run it from a Herdr lead pane.
- The selected task is discarded while selected: "task no longer available", and no other
  task is selected automatically.

## Properties
- None.
