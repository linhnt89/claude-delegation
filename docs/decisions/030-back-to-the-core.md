# 030: Back to the core: no guard, no autopilot, no check rules

- Date: 2026-11-01
- Status: accepted

## Context
From 2026-10-28 to 2026-11-01 the work went into three kinds of enforcement: the lead guard
(015), autopilot (019, 026) and rules on who may write and fix checks (023–029). Each turned a
good habit into a refusal, and each refusal needed exceptions: about ten tasks only fixed
checks, every autopilot run stopped or parked early, check-writer families emptied the worker
pool (029 was a fallback for that), and `fix-check` (t109) reached round 7 on parsing text
output. The core loop, worker picking, review and the live view kept working throughout.

## Decision
- **Rebuilt from before the guard.** `main` takes the tree of `50103a1` (the commit before
  t056) plus the work worth keeping, re-applied: waves and the overlap warning (t060–t063),
  quota tests that don't depend on the date (t101), warm worktrees and the copied
  `delegate/target` (t074, t083). Decisions stay, as history.
- **Retired:** the lead guard (015), autopilot (019, 026), check independence and check fixes
  by writers' family (023, 024, 027, 028, 029), and the parts of 025 that need code
  (`fix-check`, question-phase check flags, the allowlisted child environment). Their
  scenarios and hidden scenarios are removed; git history and the tags below keep them.
- **Kept as habits, not code:** Claude leads and delegates (the kit teaches it, nothing refuses
  it); a review by another family before merge; decision 025's checklist for check tasks;
  one session per phase with a `PLAN.md` handoff (018).
- **Archive tags:** `archive/main-before-reset` (main as it was), `archive/autopilot-c56298ee`
  (the last run's unlanded work: t096, t112–t114) and `archive/t109` (`fix-check`, round 7).
- Projects with the guard's hooks in `.claude/settings.json` lose them by hand: the rebuilt
  binary has no `delegate guard`.

## Rejected options
- Remove each feature from current `main` in delegated tasks — every removal touches
  `commands.rs`, so they run one at a time for hours; the rebuild needed one small conflict
  fix and passed the gate at once.
- Revert the ~100 commits one by one — the same result with a much noisier history.
- Force-push `main` to the old commit — loses history others may have; one commit forward
  keeps it.

## Consequences
Shorter code and fewer moving parts. Lost on purpose: the lead can now edit product files and
checks (it's a matter of discipline), and nothing runs the plan while the user is away. Kit
files in other projects may name kits newer than this binary knows; reinstall and check
`delegate upgrade` there. Any of the retired ideas comes back only through a new decision.
