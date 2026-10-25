# 001: A retry's worker sees the failed checks

- Date: 2026-10-25
- Status: accepted

## Context
After failed acceptance checks, the next `delegate run` steps up a tier and starts a new
session with a different model. That worker gets the spec and the half-done code, but not the
check output: `CHECKS.txt` stays in the task folder outside the worktree. Unless the lead
writes a `## Round N` section, the stronger model starts blind (scenario S013).

## Decision
When a round follows a round whose checks failed, the runner copies that round's `CHECKS.txt`
into `.task/CHECKS.txt` in the worktree, and the work prompt tells the worker to read it
before changing anything. The lead may still add a `## Round N` section; the copy doesn't
depend on it.

## Rejected options
- Require the lead to write `## Round N`, and refuse a rerun after failed checks without one —
  relies on the lead every time, and a plain rerun is the common case.

## Consequences
A plain `delegate run <id>` after failed checks is enough for the next worker to know what
broke. `.task/CHECKS.txt` must be removed at the start of a round that doesn't follow failed
checks, so a worker never sees stale results.
