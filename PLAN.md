# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- Checks for decision 034 (2026-11-03, t119): S134, S135, S136 (hidden), P014 sees symlinks;
  S038, S039, S041, S042, S082 layout-neutral.
- Any model can lead (2026-11-03, t115, decision 031): skills and README say "the lead";
  README "Leading with another model"; this repo's lead role is the `AGENTS.md` `lead` block.
- Managed blocks (2026-11-03, t117, decisions 032, 033): upgrade keeps kit blocks in
  `AGENTS.md`/`CLAUDE.md` current; the lead's role is the `lead` block; older projects adopt it.
- Checks for managed blocks (2026-11-02, t118): S131–S133, P029 (hidden), S040 and S082
  reworded; all fail until t117.
- `plan-review --planner` (2026-11-02, t116, decision 031): the lead names its own family or
  model; scenarios S130, P028 (hidden).
- Back to the core (2026-11-01, decision 030, with the user): `main` rebuilt from `50103a1`
  (before the lead guard) plus waves (t060–t063), date-independent quota tests (t101), warm
  worktrees and the copied `delegate/target` (t074, t083). The guard, autopilot and the check
  rules are retired, with their scenarios and hidden scenarios. Tags:
  `archive/main-before-reset`, `archive/autopilot-c56298ee`, `archive/t109`. t109–t111 and the
  autopilot worktree were discarded.
- Before that, see git history up to `archive/main-before-reset`.

## Handoff (2026-11-03, awaiting merge approval)
- Decisions 031–033 merged; secsw-code is upgraded. After decision 034 merges, upgrade
  `trial` with the new binary (its kit files are uncommitted).
- t121 round 1: independently authored (Muse) layout-neutral repairs for five unit tests and
  S001, authorized by the user. Checks pass on main and t120; lead reviewed the test-only
  diff. Its patch is already included unchanged in t120; merge t121 first, then t120.
- t120 round 14: shared skill layout, migration, init/upgrade and docs implemented. Earlier
  review findings fixed with regressions, including preservation, idempotence, exact link
  targets, traversal safety, and rename-aside/link/cleanup with rollback on link failure.
- Final lead verification: `scripts/check` passes (321 tests, clippy clean); 17 related
  scenarios pass, including hidden S136/P029. Independent Claude-family review on the final
  commit: 0 high, 0 medium, 1 low. No worker is running; neither task is merged.
- Remaining low review finding: when `.claude` or `.claude/skills` is a regular file, full
  upgrade reports affected skills but silently skips blocked pane files and says up to date.
  No data loss. Ask whether to merge with this reporting limitation.
- Deferred limitations: trailing-slash command arguments; combined canonical/layout takes
  may require two invocations; conservative no-op refusal for symlinked skill ancestors;
  inherited init `.claude` ancestry behavior; crash-interrupted backup rename needs manual
  recovery (no journal). Keep these visible if accepting the merge.
- Merge dry-run via `git merge-tree --write-tree task/t121 task/t120` is conflict-free.
  `PLAN.md` has uncommitted lead changes; preserve/commit them on main before merging.

## Waiting for you
- Merge t121 and t120, accepting the remaining low reporting limitation? Both are reviewed;
  no implementation worker remains running.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t121 | Layout-neutral repairs for five existing upgrade unit tests and S001, preserving assertions | 1 | review | – | standard | `scripts/check`, `evals/run S001`; compatible with main and t120 | Round 1 passed; diff reviewed, patch supplied to t120. Merge decision pending. |
| t120 | Skills in `.agents/skills/`, linked from `.claude/skills/` (decision 034): symlink kit files in `init`/`upgrade`, migration of old projects, this repo and `delegate/templates/project/` moved, README (Pi trust prompt) | 2 | review | t121 | deep | the 034 scenarios, `checks_by: t119` | Round 14: 321 tests and 17 lead-run scenarios passed; final review 0 high/0 medium/1 low. Awaiting merge approval. `init.rs`, `upgrade.rs`, `scripts/kit-history`, kit paths |
| t025 | Codex workers bypass bwrap (`harness.rs`), so they can read the hidden set and the data folder today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | needs a design: bwrap around Codex, or Codex's own read limits |

## Known, not fixed
- A later round can get an older `REVIEW.patch`: the runner never copies the newest review
  patch into the task's `.task/`, so a worker may apply an old one, find it "already applied"
  and report done with no changes (t109 rounds 5 and 6, 2026-11-01).
- `delegate quota` doesn't read opencode-go, so its workers (muse-spark, DeepSeek) have no
  runway estimate.
- A round that only reruns checks after a check fix still steps the tier up.
- P018 (hidden) fails now and then under a full parallel `evals/run fast` and passes on rerun:
  flakiness under load.
- Accepted at t037's merge (review 6): a push address set by hand on the vault
  (`git remote set-url --push`) survives `setup --vault <new>`; fix: `setup --vault` clears it.
  Also during a push, git updates `refs/remotes/origin/*` before delegate retakes `vault.lock`.
- Sessions are named by their checkout folder only: neither the CLI nor the mod reliably knows a
  session's title.
- Task ids are per repo, not per worktree: two worktree leads can take the same id, and
  `run --background` then reports "running" while the runner died on the existing branch.
- An older `delegate` on PATH after a newer one migrated the home finds no hidden scenarios and
  only says "no hidden scenarios found".
- Accepted at t044's merge (review, low): a check-error round keeps the tier even if the
  product-failure count already reached `step_up_after`; `pick_tier` takes the highest tier of
  all rounds, research rounds included.
- A task that adds a crate fails `scripts/check` in the sandbox: `~/.cargo` is read-only there.
  Workaround: the lead runs `cargo fetch` in the task's worktree, then `delegate check`.
- `delegate/tests/quota_review_check.sh` and `quota_review_round3_check.sh` aren't run by
  `scripts/check`; turn them into scenarios or Rust tests, or drop them.
- `delegate list` says "vault: N commit(s) not pushed" when the vault has no remote at all.
- A card beside the desktop app's Environment/Context cards isn't possible: the mod API has no
  side-card component. Revisit when it does.

- Muse models have no family in `config.rs`: a Muse worker's family is its whole model name,
  so it never matches `--planner muse` or another Muse model.

- A running plan review shows nowhere: it isn't a task, so `status`, `watch` and the pane
  show only its tasks as "not started", and `pi -p` writes `PLAN_REVIEW.md` only at the end.
  Fix: a plan-review phase on the reviewed tasks' rows, with the reviewer.

- `delegate wait` returns at once while a task stays stalled (no file changes, e.g. a worker
  thinking for minutes), so a lead can't wait on it; it should report a stall once, then wait
  for a change (t118, 2026-11-02).

## Open questions
- None.
