# 040: Park Herdr/Sheprd support

- Date: 2026-11-08
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Decisions 035–039 built toward showing delegated tasks inside Herdr/Sheprd: a headless
observer prototype (t123), the lead origin recorded by `delegate new` (t139), a side pane
that reports a lead's tasks as Herdr sidebar tokens (t124, t140), and paused groundwork for
Codex pane attribution (t130, t132, t133). The user stopped this line of work on 2026-11-08,
to revisit later when needed.

## Decision
- **Removed from `main`, kept under the tag `archive/herdr-sheprd`** (main as of `954d3f3`):
  `origin.rs`, `herdr_view.rs`, `watch --lead-pane/--beside/--sidebar-config`, the task
  `origin` and instance fields, the bounded side-pane reader in `status.rs`/`events.rs`,
  the `signal-hook` dependency (039's permission lapses with it), the skill's "Watch beside
  the lead" text, `prototypes/herdr-observer/`, the Codex identity eval libraries, fixtures
  and `evals/tests/`, and scenarios S137–S148, S151. Hidden S149/S150 leave the vault and
  stay in its history.
- **Kept:** workers, checks and reviews still lose every `HERDR_*` variable
  (`harness::strip_herdr_env`), and `evals/lib/common.sh` still unsets them. Users still
  lead from Herdr, and a worker's hook must never report into the lead's pane.
- **Kept as history:** decisions 035–039, `docs/research/` and `docs/task-specs/` for this
  work. `docs/parked/herdr-sheprd.md` is the entry point for resuming it.
- Parked plan items: t125, t127 and the deferred Codex wave (t128, t131, t134–t138).
  t025 (Codex workers bypass bwrap) is not Herdr work and stays on the plan.

## Rejected options
- Keep the code dormant — about 5,000 lines and 13 eval checks to maintain, and the side
  pane's Herdr API pin would drift unnoticed.
- Revert every Herdr commit one by one — same result, noisier history; three code reverts
  plus file removals give one commit.

## Consequences
`delegate watch` is the terminal view again, with no Herdr mode. Task states written by
the newer binary keep an `origin` field that the current one ignores. Resuming starts with
a new decision that supersedes this one, reapplies from the tag and re-checks Herdr's API,
since the probe was against Herdr 0.9.5.
