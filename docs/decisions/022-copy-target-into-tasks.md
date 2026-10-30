# 022: Each task gets its own reflinked copy of the build output

- Date: 2026-10-31
- Status: accepted

## Context
Decision 021 routed builds through sccache. Its client hands compile jobs to a server that may
run outside the worker's sandbox: in t081 that server couldn't see the sandbox's `/tmp`, and
had it worked, a worker could have had it write where the sandbox forbids (the main
checkout). Sharing `delegate/target` instead (decision 020's `share`) lets parallel tasks
overwrite each other's binaries.

## Decision
This repo's `.delegate.toml` copies `delegate/target` into each new task worktree with
`[warm] copy`. The copy clones blocks where the filesystem supports it (btrfs, XFS), so it is
near-instant and takes no space until files change; elsewhere it is a plain copy. Each task
owns its copy: no shared output, no server, nothing crossing the sandbox.

## Rejected options
- sccache (decision 021) — its server can act outside the sandbox; confining it needs
  per-sandbox sockets and hiding the host's, which is sandbox design for little gain here.
- `[warm] share` of `delegate/target` — parallel tasks overwrite each other's binaries.

## Consequences
A task's first build only recompiles what changed since the main checkout's last build, so
the lead's own builds keep the copy useful. On a filesystem without cloning, each task pays a
plain copy of `target/` (1.3 GB here).
