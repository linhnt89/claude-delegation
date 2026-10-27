# 010: Vault sync rules, as built

- Date: 2026-10-27
- Status: accepted

## Context
Decision 007 put task state in `~/.local/share/delegate/checkouts/` and hidden sets in one vault
repo. Building it (t021–t024) showed three of its rules wrong: the vault folder was renamed to
each machine's checkout name, so two machines wrote to different paths; only `evals/run` pulled,
so a machine that never ran evals never caught up; and `vault init`/`vault clone` became
`delegate setup --vault <url>` (decision 008). Everything else in 007 stands.

## Decision
- A project's vault folder `<project-id>-<name>` is named once, by whichever machine creates it,
  and never renamed; it's found by the id prefix. Duplicate folders for one id (old behaviour)
  are merged into the oldest, keeping every scenario. Checkout folders still follow the
  folder's current name.
- Every command except `where`, `statusline` and help syncs the vault: commits hand edits
  (naming the scenario ids), fetches and merges the remote (short timeout, silent when nothing
  is new), pushes unpushed commits. Once the remote is reachable, one command on each machine
  leaves every machine's hidden set equal to the remote's.
- A deletion the user made and committed on any machine is a change like any other and is
  merged. Delegate itself never deletes a scenario on its own initiative. Deleted on one side,
  edited on the other: a conflict.
- Conflicts abort the merge, keep the local commit and the remote side on `refs/delegate/theirs`,
  and are reported by every command until resolved. Never reset or force-push.
- "Sandboxed" is decided only by `DELEGATE_SANDBOXED=1`, which the runner sets for every worker
  and acceptance check. A `.task/` folder is not a sign: the lead's review runs in the worktree.

## Rejected options
- Renaming the vault folder to match the checkout name — each machine renames it to its own.
- Refusing merges that delete a scenario — the machines never converge.
- Pulling only before hidden runs — a machine that only runs `list` keeps a stale set.

## Consequences
Hidden sets converge across machines without a manual step. A vault folder's name can drift from
the repo's name; that's cosmetic. Scenario checks that run `evals/run` themselves must clear
`DELEGATE_SANDBOXED` (`evals/lib/common.sh` does).
