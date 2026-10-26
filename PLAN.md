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

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t021 | Central home by id: `checkouts/<id>-<name>/`, checkout id in `.git`, project id in `.delegate-kit.toml`, copies told apart, stale checkouts cleaned, automatic migration of `<repo>.delegate/`, `delegate where`, sandbox hides `~/.local/share/delegate/`, `evals/run` asks `delegate where`; `delegate setup` writes `workers.toml` and checks tools, `init` no longer does | todo | – | standard | `evals/run S001 S002 S043 S045 S048`; P015 hidden | `task.rs`, `init.rs`, `harness.rs`, `evals/run` + template mirror; the vault parts of S044/S049 wait for t022 |
| t022 | Vault: created locally when first needed, `delegate setup --vault <url>`, `delegate hidden add`, commit and push on each change and at the next command, `evals/run` pulls first, unpushed hint in `list`, the status line and `setup` | todo | t021 | standard | `evals/run S044 S046 S047 S049`; P016 hidden | `/evaluate` skill switches to `delegate hidden add` (root + template) |

## Known, not fixed
- Task ids are per repo, not per worktree: two worktree leads can take the same id, and
  `run --background` then reports "running" while the runner died on the existing `task/<id>`
  branch. t021 touches the home, so check whether it should claim ids there.
- A card beside the desktop app's Environment/Context cards isn't possible: the mod API
  (`claude-code.d.ts`, Claude Code 2.1.308) has no side-card component. Revisit when it does.

## Open questions
- None.
