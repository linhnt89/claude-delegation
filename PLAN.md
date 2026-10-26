# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- t001–t007 (2026-10-25): the 12 scenarios failing at baseline now pass; see git history.
  Every spec's Acceptance gets the project gate `scripts/check`; at review, `evals/run` adds
  the hidden scenarios for the same jobs.
- t008–t019 (2026-10-26): live view of running tasks (J8, decision 002): `status`, `watch`,
  `wait`, the status line, the side pane and the summary row above the prompt.
- t020 (2026-10-26): `delegate upgrade` and `.delegate-kit.toml` (J1, decisions 005, 006).

## Goal: the delegate home survives moves and machines
Moving or renaming a repo, or working on another machine, never loses task state or hidden
scenarios (J7, J1; decisions 007, 008). Baseline 2026-10-26: S043–S049, S001 and hidden
P015, P016 fail (no `delegate setup`); the rest of the set passes.

Plan review 2026-10-26 (Codex): split into four tasks, each gated on its own scenarios; the
whole fast set is t024's gate. S038/P014 ignore the record's `project` line; S045 discards the
copy's task before deleting it.

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t021 | Project id, vault (created when needed, locked), `delegate setup [--vault]`, `where`, `hidden add`, sandbox hides the data folder, `init` stops writing `workers.toml`, `evals/run` finds hidden via `where` | done | – | standard | `evals/run S001 S002 S048 S049` + upgrade set | `setup.rs`, `vault.rs`, `init.rs`, `harness.rs` |
| t022 | Task state in `checkouts/<id>-<name>/`: checkout id in the git common dir, moves repaired, copies told apart, stale folders cleaned; `/delegate` skill and docs | done | t021 | standard | `evals/run S043 S045`; P015 hidden | `task.rs`, `commands.rs`, skill + mirror |
| t023 | Migrate old `<repo>.delegate/` on first use, safe to rerun after any interruption | ready | t022 | standard | `evals/run S044` | unit tests interrupt each step |
| t024 | Vault sync: commit and push at every command, pull before hidden runs, offline copy, unpushed hints, conflicts kept apart, not-set-up messages; `/evaluate` uses `hidden add` | ready | t023 | standard | `evals/run S046 S047` + `fast`; P016 hidden | skill + mirror |
| t025 | Codex workers bypass bwrap (`harness.rs:78`), so they can read the hidden set and the data folder today | todo | t021 | deep | a Codex variant of S048 (to write with the user) | needs a design: bwrap around Codex, or Codex's own read limits |

## Goal: each session sees its own tasks
With several Claude Code sessions on one project, the status line and pane show each session
its own tasks, by title, and other sessions' apart (J8, decision 009). Baseline 2026-10-27:
S055, S056, S058 and hidden P018 fail (no owner or title recorded); S029 passes.

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t030 | Record the owning session at `new` (unowned tasks claimed by their first run); `status --json` gains `title`, `goal`, `session`, `session_name` | todo | – | standard | `evals/run S056 S058` | `task.rs`, `status.rs`, `commands.rs` |
| t031 | Status line by session: own tasks, then dimmed per-group counts | todo | t030 | standard | `evals/run S055 S029`; P018 hidden | `statusline.rs` |
| t032 | Pane: session dividers, `◆`/`○`, title and Goal line; summary row counts this session | todo | t030 | standard | S057, S030 (human) | `.claude/skills/delegate-pane/` + mirror |

## Known, not fixed
- Task ids are per repo, not per worktree: two worktree leads can take the same id, and
  `run --background` then reports "running" while the runner died on the existing `task/<id>`
  branch. t021 touches the home, so check whether it should claim ids there.
- A card beside the desktop app's Environment/Context cards isn't possible: the mod API
  (`claude-code.d.ts`, Claude Code 2.1.308) has no side-card component. Revisit when it does.

## Open questions
- How the pane learns a session's title: the mod gets it only at `SessionStart` and the app
  renames sessions later. Blocks nothing (folder or branch is the fallback); settle in t032.
