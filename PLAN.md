# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Handoff (2026-11-08, Herdr/Sheprd support parked — decision 040)
- The user stopped Herdr/Sheprd support. Its code, scenarios, prototype and Codex
  attribution groundwork left `main`; tag `archive/herdr-sheprd` keeps them. Everything
  needed to resume (state, Herdr facts, gaps, open questions, parked tasks t125, t127,
  t128, t131, t134–t138): `docs/parked/herdr-sheprd.md`.
- Kept on `main`: workers/checks/reviews lose every `HERDR_*` variable, since leads still
  run inside Herdr.
- No task state left (t142/t143 research discarded; findings in the parked note).

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t025 | Codex workers bypass bwrap (`harness.rs`), so they can read hidden scenarios/data today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | separate sandbox design |

## Previous kit handoff
- t121/t120 merged; no task state remains. `trial` still needs the new binary/kit
  upgrade (its kit files are uncommitted). secsw-code was upgraded for decisions
  031–033, before 034.
- Accepted low: if `.claude` or `.claude/skills` is a regular file, full upgrade
  reports skills but silently skips blocked pane files and says up to date; no loss.
- Deferred: trailing-slash arguments; combined canonical/layout takes may need two
  invocations; conservative symlink-ancestor refusal; inherited init `.claude`
  ancestry behavior; interrupted backup rename needs manual recovery (no journal).

## Known, not fixed
- A later round can get an older `REVIEW.patch`: the runner never copies the newest
  review patch into `.task/`; worker may apply an old one and report no changes
  (t109 rounds 5/6). Lead must copy the CURRENT patch for each fix round.
- `delegate quota` doesn't read opencode-go; DeepSeek/Muse have no runway estimate.
- A round only rerunning checks after a check fix still steps the tier up.
- P018 (hidden) flakes under full parallel `evals/run fast`, passes on rerun.
- Accepted at t037 merge: a hand-set vault push address survives `setup --vault
  <new>`; setup should clear it. During push git updates refs/remotes/origin/* before
  delegate retakes vault.lock.
- Sessions use checkout-folder names; neither CLI nor mod reliably knows titles.
- Task IDs are per repo, not worktree: two leads can collide; run --background can
  report running after its runner dies on an existing branch.
- Older delegate after a home migration may report no hidden scenarios found.
- Accepted at t044: check-error round keeps tier despite reached product-failure
  threshold; pick_tier takes highest tier of all rounds, including research.
- Adding a crate fails sandbox checks with read-only ~/.cargo; lead must cargo fetch
  in the task worktree, then delegate check.
- delegate/tests/quota_review_check.sh and quota_review_round3_check.sh aren't in
  scripts/check; turn into scenarios/Rust tests or drop them.
- delegate list reports vault commits not pushed even with no vault remote.
- Desktop Environment/Context side-card isn't possible with the current mod API.
- Muse family is its whole model name in config.rs, so planner muse or another
  Muse model doesn't match it.
- Running plan review isn't shown by status/watch/pane; rows show not started and
  pi -p writes PLAN_REVIEW.md only at end. Needs plan-review phase/reviewer display.
- delegate wait returns immediately on an unchanged stalled task instead of waiting
  for change after reporting the stall (t118).
- A research run shows phase `implement` throughout, and reads "STALLED · implement"
  when stalled (t143 survey).
- Doc conflict: decision 002 lists phases `questions…interrupted` and outcomes
  `pass, fail`; code adds `not_started` and `quota|research|check error|no_checks|no_changes`.

## Open questions
- None open outside the parked Herdr work (`docs/parked/herdr-sheprd.md`).
