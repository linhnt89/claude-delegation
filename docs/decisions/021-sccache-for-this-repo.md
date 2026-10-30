# 021: This repo's Rust builds go through sccache

- Date: 2026-10-31
- Status: accepted

## Context
Every task worktree builds `delegate` from scratch, minutes per worker round. Warm worktrees
(decision 020) could share `delegate/target`, but that folder holds one
`target/release/delegate`: parallel tasks overwrite each other's build, and one task's evals
can test another task's binary.

## Decision
`delegate/mise.toml` pins `sccache` as a tool and sets `RUSTC_WRAPPER=sccache` and
`CARGO_INCREMENTAL=0` (sccache doesn't cache incremental builds). Each worktree keeps its own
`target/`; compiled crates are reused by content from `~/.cache/sccache`, which the sandbox
already leaves writable. The tool is installed on the host (`mise install`); workers can't
download it.

## Rejected options
- `[warm] share = ["delegate/target"]` — parallel tasks overwrite each other's binaries.
- A shared `CARGO_TARGET_DIR` — same problem.

## Consequences
Rounds after the first build reuse compiled dependencies and unchanged crates. The lead's
own builds lose incremental compilation in exchange. A machine without the pinned sccache
fails the build until `mise install` runs in `delegate/`.
