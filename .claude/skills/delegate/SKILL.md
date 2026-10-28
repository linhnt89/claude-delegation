---
name: delegate
description: Hand well-specified tasks to worker models (Pi, Codex, Claude Code, OpenCode, Gemini) by role and tier, each in its own git worktree and sandbox, then check, review and merge or discard them. Also runs plan review before a batch of work. Use when the user says /delegate, "hand this to a worker", "review the plan", or when a PLAN.md task is ready and is implementation, refactor, tests, migration or research rather than design or open-ended debugging.
---

# Delegate tasks

Goal: put every available model to work where it fits (quality, quota, parallel runs, a
second opinion), while Claude keeps planning, specs and decisions. Trust comes from the same
checks for every model, not from which model did the work.
Runner: the `delegate` binary on PATH (`delegate --help`). All task state lives outside the
repo, under `~/.local/share/delegate/checkouts/<checkout-id>-<name>/<id>/`, found by the
checkout's id (shared by linked worktrees; override with `DELEGATE_HOME`). Moving or renaming
keeps tasks; copying starts a separate task home. Task state is deleted on merge or discard.
`delegate where` prints the task folder.

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

When showing the spec, tell the user the task and tier on a line of its own, in exactly this
form: `t007: implement @ standard` (role `implement`, `research` or `review`). Repeat that line
in every later update on the task (started, round N, done): the user may read only the last
message. Then, without waiting for a yes, run it in the background (the user can still change
the tier):

    delegate run <id> --background [--tier fast|standard|deep]

Then start a wait as another background command, so the harness wakes you when something
happens instead of polling:

    delegate wait <id> --timeout 25m

Act on what `wait` printed when it exits:
- `<id> done, <outcome>` — review the result (steps 5 and 6).
- `<id> interrupted` — read the run log, then `delegate run <id>` again or `delegate discard <id>`.
- `<id> stalled in <phase> for <duration>` — check the last step, the worker's processes and
  the log tail; then wait again, stop it and rerun, or tell the user.
- nothing happened yet (exit 3) — it timed out; start `delegate wait <id> --timeout 25m` again.

What happens:
- **Question phase** (first round): the worker reads the spec and code and answers `READY` or
  asks blocking questions, with no edits. If it asks, the run stops and prints them. Answer
  in the spec under `## Answers` (ask the user when it's their call), then run again.
- **Work phase** continues in the same session, then the runner commits and runs the
  acceptance commands itself (`CHECKS.txt`).
- **Fallbacks:** if a candidate can't start (quota, auth), the next one in the tier is used.
- **Step-up:** after `[defaults] step_up_after` product-failing rounds at the current tier
  (default 1), the next run goes one tier higher, up to deep. A passing round resets the count;
  check errors (exit 3) and unfinished rounds neither count nor reset it. After a check error,
  fix the broken check or the spec's Acceptance; the next run stays at the same tier. Automatic
  selection never steps down, even after a pass.
- `--no-ask` skips the question phase; `--worker harness:model@effort` overrides the config
  for a one-off; `--dry-run` shows the tier and candidates.
- `--research` runs a read-only research task (research tiers, no question phase, no checks):
  the worker's `REPORT.md` is the result. Read it, keep what matters, then `delegate discard`.
- `delegate new <id> --prune` starts from the stale-doc pruning spec.

Independent tasks with separate files can run in parallel. Check each run with
`delegate status <id> --json`: `phase` says what is happening, `outcome` says how it
finished, `stalled` warns of no recent file activity, and `next` gives the next action.
A stalled run is not automatically stopped. `delegate list` shows the batch at a glance.
For the user: `delegate watch` in a terminal, and the `/delegate-pane` mod (installed by
`delegate init`): a row above the prompt with every session's counts, and a side pane that
opens when they ask. Both show the same status live.

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

You never commit on a task branch, not even a one-line fix or a doc: everything on it is
the worker's, so the record shows what each model did. Judge by the spec's Acceptance and
contract, not taste: a choice that works, passes the checks and keeps the contract (a test
file named differently, other wording) is accepted and mentioned to the user, not reworked.

Then:
- **Accept:** ask the user, naming the action ("Merge t007?", not "shall I go ahead?"), then
  `delegate merge <id>`. Merge is refused unless checks pass on
  the final commit, project rules are met, and any review on the final commit has no
  high-severity findings (`--accept-findings` only if the user accepts them). It squash-merges
  with the spec's title and Goal as the message and deletes all task state.
- **Merge conflict** with the base branch: run `git -C <worktree> merge --no-commit <base>`,
  append a `## Round N` saying to resolve the conflicts, then `delegate run <id> --background`.
  The runner's round commit concludes the merge.
- **Another round:** for anything that breaks the spec or the contract, however small, append
  `## Round N` to the spec with only what to fix, then `delegate run <id> --background` again.
  For `REVIEW.patch` tests, copy the patch to `<worktree>/.task/REVIEW.patch` and have the
  round say to apply it first.
- **Reject:** `delegate discard <id>`; note in `PLAN.md` why if the task stays open.

Before ending a session, run `delegate list`: merge, discard, or tell the user what's open.

## 7. Close the loop (part of merging, not later)
- Did this change make any statement in `AGENTS.md`, a module README or `PLAN.md` false?
  Fix it, and mark the task done in `PLAN.md`, in your own commit on the base branch right
  after the merge, never on the task branch.
- Anything worth keeping from the report or review goes into a decision or `AGENTS.md`
  before merge; the task folder, report included, is deleted.
- Write the spec's Goal as a good commit body: it becomes the permanent record of why.
