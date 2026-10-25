# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Goal
Make the 12 failing user scenarios pass, so the kit never reports success it didn't earn,
says why things fail, and leaves main clean (baseline 2026-10-25: 17 pass, 12 fail).

## Tasks
Order: t001 + t007 → t002 → t003 + t004 → t005 + t006 (each wave touches different parts of `commands.rs`).
Every spec's Acceptance also gets the project gate:
`scripts/check`.
At review, `evals/run` adds the hidden scenarios for the same jobs.

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t001 | Explain start failures: name a missing harness or bwrap, show each candidate's reason (stderr tail), and create no worktree/branch/round for a run that never started | todo | – | standard | `evals/run S003 S012 S014` | `harness.rs`, `run()` in `commands.rs` |
| t002 | Honest run outcome: say when a worker changed nothing or exited non-zero, and show an interrupted run in `list` as unfinished, never as passed or "not run" | todo | t001 | standard | `evals/run S007 S021` | same `run()` as t001; touches round state in `task.rs` and `list` |
| t003 | A retry is told what failed: the runner copies the failed round's `CHECKS.txt` into `.task/` and the work prompt points to it | todo | t002 | standard | `evals/run S013` | `docs/decisions/001-retry-sees-failed-checks.md`; `run()` and `prompts.rs` |
| t004 | Clear merge refusals: name the failing check command, name the changed paths that set off a rule; `list` shows which model reviewed | todo | t002 | fast | `evals/run S008 S009 S010` | `merge()` and `list()`; `list` also changed by t002 |
| t005 | Safe conflicting merge: on a squash conflict, put main back exactly as it was and say which files conflict | todo | t004 | standard | `evals/run S016` | `merge()`, after t004 |
| t006 | Research commits nothing: a research run never commits code; any files the researcher changed are listed and left out | todo | t003 | fast | `evals/run S017` | research branch of `run()`, after t002 and t003 |
| t007 | Clear message for an empty candidate list in `workers.toml`, instead of "every candidate is in the ? family" | todo | – | fast | `evals/run P007` | `config.rs` only; can run in parallel with anything |

## Open questions
- None.
