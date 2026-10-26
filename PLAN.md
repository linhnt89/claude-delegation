# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- t001–t007 (2026-10-25): the 12 scenarios failing at baseline now pass; see git history.
  Every spec's Acceptance gets the project gate `scripts/check`; at review, `evals/run` adds
  the hidden scenarios for the same jobs.

## Goal: live view of running tasks
Let the user and Claude see, any time, each task's phase, its worker's steps, and whether it
stalled or died (J8; `docs/decisions/002-live-task-view.md`). Baseline 2026-10-25: S022–S029,
S031, P009, P010 and hidden P011 fail; S030 is judged by hand.

Order: t008 + t009 → t010 + t011 → t012 + t014 → t013. t010, t011 and most of t012 can run in parallel.

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t008 | Record each harness's stream (script in `evals/lib/streams/README.md`), confirm its streaming flags, write the `.steps` files | done | – | – | the recordings exist | Claude Code, Codex, OpenCode, Pi. Gemini is out of scope for now: no adapter, basic view |
| t009 | Live status: `status.json` kept by the runner, `interrupted` for a dead run, `status [id] [--json]`, `run --background`, `list` reads the same phase; delegate skill tells Claude to use `run --background` and `status --json` | done | t005 | standard | `evals/run S022 S026 S028` | `run()` in `commands.rs`, `task.rs`; skill in both root and `templates/project/` |
| t010 | Step adapters: workers run with streaming flags, an adapter for Claude Code, Codex, OpenCode and Pi (not Gemini) writes `events.jsonl`, raw stream kept; `status` shows steps and the stall flag | done | t008, t009 | standard | `evals/run S023 S025 S027 P010` | new module; `harness.rs` commands |
| t011 | `delegate watch [id]`: refreshing terminal view, clean exit on Ctrl-C, plain frames when not a terminal | done | t009 | standard | `evals/run S024` | reads only what `status` reads |
| t012 | `delegate statusline`, installed by `init` into `.claude/settings.json` without replacing the user's own | done | t009, t011 | fast | `evals/run S029 S031 P009` | `init.rs`; P009 needs `watch` too |
| t014 | `delegate wait [<id>...] [--timeout]`: returns on finish, interrupt or stall; skill tells Claude to run it in the background after `run --background` | done | t010 | fast | `evals/run S032 S033 S034` | `docs/decisions/003-claude-waits-on-runs.md`; new `wait.rs` |
| t013 | Side pane mod in Claude Code, installed by `init` | done | t012 | standard | `evals/run S030` (by hand) | blocked on the open question below |

S030 verdict (2026-10-26, by hand while t015 ran): tasks with phase, worker, times and files,
running steps, self-update and the no-tasks state are fine. Not yet seen: a stalled task
standing out, and light/dark mode. The user finds the pane and `delegate watch` hard to read;
improving their layout is a later task.

Readable live views (2026-10-26): the user picked an aligned table for `watch`/`status` and
status-first cards for the pane, trouble first, stalled in the warning colour with `⚠ STALLED`;
the pane opens when a task starts and closes once no task is left to act on (a card beside
Environment/Context isn't possible: the mod API has no such slot). Criteria in S030 and S035.

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t016 | Pane: status-first cards, attention order, opens/closes by itself | done | – | standard | `claude plugin test`, mirror identical; S030 by hand | `.claude/skills/delegate-pane/` + template |
| t017 | `watch`/`status`: aligned table, attention order, colour in a terminal | done | – | standard | `evals/run S035 S024 S025 P009` | `status.rs`, `watch.rs` |
| t018 | Summary row above the prompt (`AbovePrompt` band) that opens the pane | done | – | standard | `claude plugin test`, mirror identical; S030 by hand | replaces the side-card wish |
| t019 | Summary row: counts in the pane's colours, `[ Open tasks ]` button, keyboard hint on the terminal's normal screen, row shows while the pane waits undrawn | done | t018 | standard | same | 2 rounds |
| t019 | Stalled hint offers commands that work: `delegate status <id>  ·  kill <pid>` in status, watch, `--json` `next` (pane) and `wait` | done | – | standard | `evals/run S036 S035 S025 S032 S033 S034` | `status.rs`, `wait.rs` |

t018 and t019 were each used by two worktree sessions at once (ids are per repo, see below);
git history tells them apart by commit title. The worktree-state t018 is in the next merge.

S030 verdict (2026-10-26, by hand on a demo run with a real stall, light and dark): full pass.
The pane opened by itself, reopened when a new task started after a manual close, and closed
once every task was gone. Still wanted: a card beside the desktop app's Environment/Context
cards, once Claude Code offers mods such a slot. Seen on the way, not fixed:
- The stalled hint said `delegate discard <id>`, which refuses a running task: fixed by t019
  (the hint is now `kill <pid>`, then the interrupted task's run-or-discard hint).
- Led from a git worktree, task state lands in `<worktree>.delegate/` beside the other
  worktrees (the desktop app blocks the lead's edits there), while `evals/run` results go to
  the main repo's `.delegate/`, which the worker sandbox mounts read-only, so evals in a
  spec's Acceptance fail unless they set `EVALS_RESULTS`. Moving task state with
  `DELEGATE_HOME` under `/tmp` doesn't help: the evals' own sandboxed runs mount an empty
  `/tmp`, hiding the checkout's stand-in workers, so every run-based scenario fails setup
  (t019 used `~/.cache/...` instead, with `DELEGATE_SANDBOX=0 delegate check`).
- Task ids are per repo, not per worktree: a lead in another worktree had taken `t018`, and
  `run --background` said "running" while the runner had already died on the existing
  `task/t018` branch (`delegate wait` then timed out on a `not_started` task).

A card beside the desktop app's Environment/Context cards (checked 2026-10-26, Claude Code
2.1.308): not possible with supported means. In the mod API types (`claude-code.d.ts`), the
components a mod draws are `RenderComponent` (~line 9812): message rows, tool rows, spinner,
hints, `AbovePrompt` and `Pane`, with no side card. `Pane` is the only one a mod opens, seated
as `placement: 'dock' | 'inline'` (~10879). The `desktop` element table (~3951) lists elements
(Box, Text, Svg, Client...), not places. `$.ui.mount` draws in tests only (`MountTarget`, ~16765).
MCP app widgets render inline in the chat, and the app's settings have nothing for it. Closest
supported: the docked pane we have, made narrow with `$.ui.open({ columns })` (~7647), plus the
status line. Revisit when the mod API adds a side-card component.

Instead (2026-10-26): a summary row above the prompt, shown while the pane is closed. The user
picked coloured counts plus an `Open tasks` button over a whole-row click (a Button draws its
own label style, so counts inside it lose their colours), and, on the terminal's normal screen
where clicks don't reach it, the hint `/delegate-pane or ctrl+x tab, Enter to open`.
S030 verdict on the row (2026-10-26, by hand in the desktop app and the terminal): pass.

## Open questions
- None.
