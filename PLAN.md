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
| t016 | Pane: status-first cards, attention order, opens/closes by itself | running | – | standard | `claude plugin test`, mirror identical; S030 by hand | `.claude/skills/delegate-pane/` + template |
| t017 | `watch`/`status`: aligned table, attention order, colour in a terminal | running | – | standard | `evals/run S035 S024 S025 P009` | `status.rs`, `watch.rs` |

## Open questions
- None.
