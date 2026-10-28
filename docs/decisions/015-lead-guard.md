# 015: A guard of Claude Code hooks keeps the lead out of product files

- Date: 2026-10-28
- Status: accepted

## Context
Decision 004 and the skills tell the lead not to implement, but only as text: when the user
asks nicely or the fix is one line, Claude does it itself (J9). The checks for the guard
(S050–S054, P017, written in t026) fix its behaviour, not its mechanism. Workers that are
Claude Code load the same project settings (`--setting-sources project`), so they get the
same hooks; bwrap hides task state from them.

## Decision
- **Three hooks in `.claude/settings.json`**, each running `delegate guard <event>`:
  `PreToolUse` (matcher Edit|Write|NotebookEdit), `UserPromptSubmit` and `Stop`.
  `init` and `upgrade` merge them into the user's settings once, keep everything else,
  leave invalid JSON alone with a message, and say how to turn the guard off.
- **Lead files** are root `*.md`, `docs/**`, `evals/jobs.md`, `evals/scenarios/*/scenario.md`
  and any globs in `[guard] lead_files` in `.delegate.toml`. Everything else in the repo is
  product. Paths are judged after resolving `..` and symlinks; paths outside the repo pass.
- **Task worktrees** of this checkout follow the same rules, with `.task/**` also a lead
  file (review patches go there), so the lead can't fix a worker's branch (decision 004).
- **A refusal** names the file and says to write a spec and `/delegate` it.
- **Shell edits**: `UserPromptSubmit` records each product file's content at turn start;
  `Stop` holds the turn, naming product files whose content changed since, until they are
  undone. Changes brought by commits with a `Worker:` trailer (`delegate merge`) or already
  on a remote are not blamed. A turn without such changes ends silently.
- **Workers**: the runner sets `DELEGATE_WORKER=<task id>` on every worker. A hook is a
  worker's only when that is set and its project dir is that task's worktree; then every
  hook allows. A lead started by hand in a task worktree is still a lead.
- **Off switch**: `DELEGATE_GUARD=off` in the lead's environment, set by the user. Nothing
  else turns it off; the settings and `.delegate.toml` that could are product files.
- It is a seatbelt, not a sandbox: a lead that deliberately starts its own `claude` with
  forged worker variables is not stopped.

## Rejected options
- `DELEGATE_WORKER` alone as worker identity — a stray variable would switch the guard off.
- A runner-written token in task state — bwrap hides task state from the worker.
- A fixed list of lead files — READMEs and repo-specific lead files (this repo's
  `evals/lib/streams/*.steps`) would need delegating.
- Guarding the repo only — task worktrees live outside it, leaving decision 004 to text.
- Refusing shell commands in `PreToolUse` — a command's effect can't be read from its
  text; comparing contents at `Stop` catches every route.

## Consequences
The lead can't make even a one-line product fix; it costs a spec and a worker round.
`delegate` must be on `PATH` for the hooks; a hook that can't run fails open. Projects
that keep lead-written files outside the defaults list them in `.delegate.toml`.
