# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Done
- `plan-review --planner` (2026-11-02, t116, decision 031): the lead names its own family or
  model; scenarios S130, P028 (hidden).
- Back to the core (2026-11-01, decision 030, with the user): `main` rebuilt from `50103a1`
  (before the lead guard) plus waves (t060–t063), date-independent quota tests (t101), warm
  worktrees and the copied `delegate/target` (t074, t083). The guard, autopilot and the check
  rules are retired, with their scenarios and hidden scenarios. Tags:
  `archive/main-before-reset`, `archive/autopilot-c56298ee`, `archive/t109`. t109–t111 and the
  autopilot worktree were discarded.
- Before that, see git history up to `archive/main-before-reset`.

## Handoff (2026-11-01, after the rebuild)
- `delegate list`: no open tasks.
- Next: check the kit in `secsw-code` and `trial` with the rebuilt binary (their
  `.delegate-kit.toml` may name kits it doesn't know), then choose the next core task below.
- 2026-11-02: decision 031 (any model can lead); t116 merged; t115 running.

## Waiting for you
- None.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t115 | Model-neutral kit: lead role from `CLAUDE.md` to `AGENTS.md`, "the lead" in the skills, `--planner` in the delegate skill, README "Leading with another model" (decision 031) | 2 | running | – | fast | `grep -i claude` in the lead texts finds only Claude-specific lines; `scripts/check` passes | root and `delegate/templates/project/` together, then `scripts/kit-history` |
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

## Open questions
- None.
