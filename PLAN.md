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
| t010 | Step adapters: workers run with streaming flags, an adapter for Claude Code, Codex, OpenCode and Pi (not Gemini) writes `events.jsonl`, raw stream kept; `status` shows steps and the stall flag | todo | t008, t009 | standard | `evals/run S023 S025 S027 P010` | new module; `harness.rs` commands |
| t011 | `delegate watch [id]`: refreshing terminal view, clean exit on Ctrl-C, plain frames when not a terminal | done | t009 | standard | `evals/run S024` | reads only what `status` reads |
| t012 | `delegate statusline`, installed by `init` into `.claude/settings.json` without replacing the user's own | done | t009, t011 | fast | `evals/run S029 S031 P009` | `init.rs`; P009 needs `watch` too |
| t014 | `delegate wait [<id>...] [--timeout]`: returns on finish, interrupt or stall; skill tells Claude to run it in the background after `run --background` | done | t010 | fast | `evals/run S032 S033 S034` | `docs/decisions/003-claude-waits-on-runs.md`; new `wait.rs` |
| t013 | Side pane mod in Claude Code, installed by `init` | done | t012 | standard | `evals/run S030` (by hand) | blocked on the open question below |

## Open questions
- None.
