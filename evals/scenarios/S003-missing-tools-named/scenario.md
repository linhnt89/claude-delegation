# S003 A missing tool is named before any work starts
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer (or Claude as lead) in a git repo of their own, with `delegate` on PATH and a `workers.toml` naming their harnesses.

## Starting state
A set-up repo with a ready spec; the configured harness binary, or `bwrap`, is not on PATH.

## Steps
1. Run `delegate run t001`.

## Passes when
- It stops with a message naming what's missing and what to do (install it, or the setting to change).
- No worktree, branch or half-made round is left behind for a run that never started.

## If this fails
The user gets a cryptic failure or a half-made task and has to dig to find that a tool is missing.

## Failure paths
- Only the first candidate's harness is missing: the next candidate is used instead (see S012).

## Properties
- None.
