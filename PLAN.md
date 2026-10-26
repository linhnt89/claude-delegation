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
scenarios (J7; `docs/decisions/007-delegate-home-by-id.md`). Baseline 2026-10-26: S043 fails.

Next: `/evaluate feature` — revise S043 to "after a move, nothing breaks" (needs the user's
yes), and add scenarios for: migrating an existing `<repo>.delegate/`, a copy vs a move, a new
machine with and without the vault, a hidden-set change pushed to the vault. Then specs:

| id | task | status | depends on | tier | acceptance | notes |
|----|------|--------|------------|------|------------|-------|
| t021 | Central home by id: `checkouts/<id>-<name>/`, checkout id in `.git`, project id in `.delegate-kit.toml`, automatic migration, sandbox hides `~/.local/share/delegate/`, `evals/run` uses the same rule | todo | scenarios | standard | S043 + new | `task.rs`, `evals/run` (+ template mirror), `harness.rs` |
| t022 | Vault: `delegate vault init/clone/sync`, commit and push on each hidden-set change, pull at review, unpushed hint in `list` and the status line | todo | t021 | standard | new scenarios | |

## Known, not fixed
- Task ids are per repo, not per worktree: two worktree leads can take the same id, and
  `run --background` then reports "running" while the runner died on the existing `task/<id>`
  branch. t021 touches the home, so check whether it should claim ids there.
- A card beside the desktop app's Environment/Context cards isn't possible: the mod API
  (`claude-code.d.ts`, Claude Code 2.1.308) has no side-card component. Revisit when it does.

## Open questions
- None.
