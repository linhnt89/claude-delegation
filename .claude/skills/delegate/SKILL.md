---
name: delegate
description: Hand well-specified tasks to worker models (Pi, Codex, Claude Code, OpenCode, Gemini) by role and tier, each in its own git worktree and sandbox, then check, review and merge or discard them. Also runs plan review before a batch of work. Use when the user says /delegate, "hand this to a worker", "review the plan", or when a PLAN.md task is ready and is implementation, refactor, tests, migration or research rather than design or open-ended debugging.
---

# Delegate tasks

Goal: put every available model to work where it fits (quality, quota, parallel runs, a
second opinion), while Claude keeps planning, specs and decisions. Trust comes from the same
checks for every model, not from which model did the work.
Runner: the `delegate` binary on PATH (`delegate --help`). All task state lives outside the
repo (`../<repo>.delegate/<id>/`) and is deleted on merge or discard.

Which model does the work is decided by **role × tier** in `~/.config/delegate/workers.toml`,
never per task. Project rules in `.delegate.toml` set minimum tiers and required reviews by path.

## 1. Decide whether it should be delegated
Keep it here if it is ambiguous, defines shared contracts or architecture, is
security-sensitive, or is a debugging hunt with an unknown cause. Otherwise delegate.
If it is too big for one reviewable diff, split it first and add the pieces to `PLAN.md`.

## 2. Write the specs
- `delegate new <id>` (id like `t007`) prints the spec path; fill it in.
- Context section: **pointers** to files and decisions, never pasted discussion.
- Acceptance: lines like ``- [ ] `command` `` are **run by the runner**, and merge requires
  them all to pass. If you can't write one, the task isn't ready.
- Acceptance also names the user scenarios the task must satisfy: ``- [ ] `evals/run S004` ``.
  No agreed scenario for the feature yet → `/evaluate feature` first.
- Scope In: list paths in backticks; project rules use them to raise the tier up front.
- Scope Out: name files parallel tasks are touching.

## 3. Plan review (before a batch)
For a multi-task plan, or any task you'd put at `deep`, run `delegate plan-review [ids...]`.
A model from a different family than you reviews `PLAN.md` and the specs against the code.
Fix the plan for real findings; bring disagreements to the user. Skip it for one small task.

## 4. Pick a tier and run
| Task | Tier |
|---|---|
| Mechanical edits, docs, codebase survey | `fast` |
| Implementation with clear contracts and tests | `standard` (default) |
| Algorithms, concurrency, cross-module refactors, security-sensitive | `deep` |

Tell the user the choice when showing the spec ("t007: implement @ standard"), then, unless
they change it, run in the background:

    delegate run --background <id> [--tier fast|standard|deep]

What happens:
- **Question phase** (first round): the worker reads the spec and code and answers `READY` or
  asks blocking questions, with no edits. If it asks, the run stops and prints them. Answer
  in the spec under `## Answers` (ask the user when it's their call), then run again.
- **Work phase** continues in the same session, then the runner commits and runs the
  acceptance commands itself (`CHECKS.txt`).
- **Fallbacks:** if a candidate can't start (quota, auth), the next one in the tier is used.
- **Step-up:** after failed checks, the next run goes one tier higher automatically.
- `--no-ask` skips the question phase; `--worker harness:model@effort` overrides the config
  for a one-off; `--dry-run` shows the tier and candidates.
- `--research` runs a read-only research task (research tiers, no question phase, no checks):
  the worker's `REPORT.md` is the result. Read it, keep what matters, then `delegate discard`.
- `delegate new <id> --prune` starts from the stale-doc pruning spec.

Independent tasks with separate files can run in parallel. Check each run with
`delegate status <id> --json`: `phase` says what is happening, `outcome` says how it
finished, `stalled` warns of no recent file activity, and `next` gives the next action.
A stalled run is not automatically stopped. `delegate list` shows the batch at a glance.

## 5. Review
Run `delegate review <id>` when the risk calls for it: `deep` tasks, public API or schema
changes, anything a `.delegate.toml` rule marks `review = "always"`, or when your own look
leaves doubt. A model from a different family than the implementer tries to break the change;
it sees the spec and diff, not the implementer's report. Failing tests it writes come back as
`REVIEW.patch` (the branch is untouched).

## 6. Decide (cheaply)
Read in this order and stop when you can decide:
1. `SUMMARY.txt` and `CHECKS.txt` in the task folder
2. `/evaluate review <id>`: the task's scenarios plus the hidden ones, run in its worktree
   (the sandboxed checks can't see hidden scenarios; you can)
3. `REPORT.md`, and `REVIEW.md` if reviewed
4. `git diff <base>...task/<id> -- <file>` only for files that matter

Then:
- **Accept:** ask the user, then `delegate merge <id>`. Merge is refused unless checks pass on
  the final commit, project rules are met, and any review on the final commit has no
  high-severity findings (`--accept-findings` only if the user accepts them). It squash-merges
  with the spec's title and Goal as the message and deletes all task state.
- **Another round:** append `## Round N` to the spec with only what to fix (or apply
  `REVIEW.patch` tests in the worktree and commit), then `delegate run --background <id>` again.
- **Small fix:** commit it in the worktree yourself, then `delegate check <id>`.
- **Reject:** `delegate discard <id>`; note in `PLAN.md` why if the task stays open.

Before ending a session, run `delegate list`: merge, discard, or tell the user what's open.

## 7. Close the loop (part of merging, not later)
- Did this change make any statement in `AGENTS.md`, a module README or `PLAN.md` false?
  Fix it in the same branch before merging.
- Anything worth keeping from the report or review goes into a decision or `AGENTS.md`
  before merge; the task folder, report included, is deleted.
- Write the spec's Goal as a good commit body: it becomes the permanent record of why.
