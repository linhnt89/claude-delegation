# 007: The delegate home is found by id, and hidden sets live in a vault

- Date: 2026-10-26
- Status: superseded by 010

## Context
The delegate home is `<main checkout>.delegate/`, named by path (t018). Moving the repo from
`trial/cooperation` silently started an empty home; the old one, with the only copy of the
hidden set, was lost (S043). The home mixes things with different lifetimes: task state (one
task, one machine), eval results (until the next run), and hidden scenarios (the project's
life, and the only part that can't be rebuilt). The user develops on several machines and
syncs projects with git.

## Decision
- **Split by lifetime, in one central place, `~/.local/share/delegate/`:**
  - `checkouts/<checkout-id>-<name>/`: task state and eval results for one checkout on this
    machine; never synced. Task folders are deleted on merge or discard, as before. The folder
    records its checkout's path; delegate removes it once that checkout is gone and no task is
    open.
  - `vault/<project-id>-<name>/hidden/`: the project's hidden scenarios.
  - Folders are found by id; `<name>` is the repo folder's name, for people, and is renamed to
    match when it changes.
- **Two ids.** The checkout id lives in `.git` (not committed): it moves with the repo, and a
  fresh clone gets its own. A copy (`cp -a`) carries it too; delegate tells a copy from a move
  because the recorded checkout path still exists, and gives the copy a new id. The project id
  is committed (`project` in `.delegate-kit.toml`), so every clone on every machine finds the
  same hidden set.
- **The vault is one private git repo for all projects.** `delegate vault init` or
  `delegate vault clone <url>`, once per machine. Delegate commits each change to a hidden set
  and pushes right after; it pulls before using the hidden set at review. `delegate list` and
  the status line say when the vault has unpushed commits. The remote URL lives only in the
  vault's `.git/config`. The sandbox hides all of `~/.local/share/delegate/` from workers. A
  project whose id isn't in the vault says so (no more silent "no hidden scenarios found").
- **Existing homes move automatically:** the first time delegate finds `<repo>.delegate/`, it
  moves task state and results into `checkouts/` and the hidden set into the vault, and says so.
- After a move or rename, everything is found again without a message.

## Rejected options
- Home beside the repo, or central folders named by path — a move silently orphans them.
- `~/.claude/` — the sandbox mounts it read-write for Claude workers, over the hidden home.
- `~/.cache/` — every worker gets it read-write.
- A companion repo per project — a chore to create and sync for every project.
- A hidden branch in the project repo — workers can read `.git`.
- Syncing all of `~/.local/share/delegate/` with a file-sync tool — it would carry live task
  state (paths, PIDs, phases) between machines.
- Later, not rejected: the hidden set encrypted in each project repo (`age`/`gpg`, key outside
  the sandbox), for team projects where one personal vault doesn't fit.

## Consequences
One extra private repo, set up once per machine. Delegate needs network access when it pushes
or pulls the vault, and says so when that fails. S043 changes from "tells you where the home
went" to "nothing breaks", which needs the user's yes on its new text.
