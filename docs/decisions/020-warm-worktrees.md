# 020: Warm task worktrees inside delegate, not through treehouse

- Date: 2026-10-30
- Status: accepted

## Context
Each task gets a fresh detached worktree in its task folder, so every worker round starts from
a cold build (minutes of `cargo build` in this repo). Treehouse
(github.com/kunchenguid/treehouse) pools and reuses worktrees, keeping their build caches.

## Decision
`delegate` gets a per-project warm-start setting for new task worktrees: share a build cache
(a per-checkout cargo target dir, or similar) or copy/reflink chosen ignored paths from the
main checkout. Worktree lifecycle and the sandbox stay as they are.

## Rejected options
- Treehouse — a new dependency for every kit user; it duplicates our worktree lifecycle; it
  has no sandbox, so bwrap binds and the guard's worktree detection would have to change; and
  pooled reuse breaks leftover-state detection of interrupted runs.

## Consequences
Shared caches must stay writable inside bwrap without opening the main checkout to workers.
`[warm]` paths must be ignored by git (refused otherwise, saying so): the runner commits with
`git add -A`, so a shared or copied path git doesn't ignore would land in the worker's commit.
