# 019: Autopilot runs the plan's waves while the user is away

- Date: 2026-10-30
- Status: accepted

## Context
The lead runs a wave without waiting for a yes (decision 016), but stops at every merge
question, at worker questions that are the user's call, and at the end of each wave. A user
who leaves comes back to a lead waiting on "Merge t061, t062?". Keeping one session running
the whole plan grows its context (decision 018).

## Decision
- **Prep first.** With the user present, the lead walks the plan to its goal and gathers what
  needs the user up front: open questions, scenarios, contracts, likely worker questions. It
  writes a `## Mandate` section in `PLAN.md`: the goal (tasks or scenarios), the merge rule
  (merge without asking when checks, the task's and hidden scenarios, and required reviews
  pass), and what still needs the user (new decisions, scope changes, accepting findings,
  discarding a feature).
- **Driver.** `delegate autopilot` is a plain loop, no model: it starts a fresh headless lead
  (`claude -p`, auto mode, the lead guard as the boundary) for one wave, waits for it to exit,
  checks `PLAN.md` and `delegate list`, and starts the next. Subcommands: start, stop, status, land.
- **Its own checkout.** Autopilot works in a linked worktree on an `autopilot` branch and
  merges there, never into the user's checkout. `delegate autopilot land` brings it to the
  base branch; the user is asked once, on return.
- **One lead at a time.** A lead lock is held while autopilot runs. Another lead session may
  discuss and write specs for later waves, but runs or merges tasks only after `autopilot stop`.
- **What a headless lead may do.** Run and merge per the mandate; write specs for tasks already
  in `PLAN.md`; add fix tasks only for breakage it caused; take small, reversible choices as
  `proposed` decisions for the user to confirm. Anything else is parked: the task and its
  question go to "Waiting for you" in `PLAN.md`, its dependents wait, all other work continues.
- **Stops** when the goal is met, everything left is parked, `scripts/check` fails on
  `autopilot`, the lead's Claude account drops below a quota floor, or after a maximum number
  of waves (flag, default 5). Numeric limits are flags; the mandate is prose.
- **Records.** Each wave ends with a handoff in `PLAN.md` (decision 018). In the delegate home:
  an autopilot log (one line per action, plus each lead's `stream-json`) and a run report
  (merged, decisions to confirm, parked with questions, why it stopped).
- **Notification.** One per run, when it stops: the driver runs the user's `notify` command
  from the user config with `DELEGATE_MESSAGE` set. Messages carry counts, ids and reasons,
  never code or diffs. The kit contacts no service itself; a failed notify is logged.

## Rejected options
- The main session supervising headless sessions — its context grows again, and the mandate
  already holds what it would decide.
- A Stop hook keeping one session going — one session's context grows across the whole plan.
- Visible interactive lead sessions per wave — they stall on prompts nobody is there to answer.
  A live view comes from the log instead.
- Merging into the user's checkout — files change under their session and editor.
- A command allowlist instead of auto mode — the run stalls on the first command not foreseen.
- A notification per task — noise overnight; the run report has the detail.

## Consequences
The user answers in batches and confirms small decisions after the fact. The mandate and
handoffs must be complete, since nothing passes between waves but the repo. Landing
`autopilot` can conflict when the user changed the same files meanwhile.
