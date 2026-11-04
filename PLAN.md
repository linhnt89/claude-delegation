# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Handoff (2026-11-04, green headless observer slice merged)
- User approved decision 035: target installed `herdrdev/herdr` + `penso/herdr-gpui`;
  changes to both are acceptable. Lead left, read-only task dashboard right; sidebar
  tasks nest under the launching lead conversation. `delegate` retains execution ownership.
- Research: `docs/research/herdr-delegation-observers.md` has pinned source versions,
  integration points, and gaps. t123's opt-in headless prototype is now merged under
  `prototypes/herdr-observer/`; no native UI or production ownership/protocol changes.
- t122: user approved observer scenarios S137–S144; all marked agreed. Executable human
  walkthroughs added; `evals/run S137 S138 S139 S140 S141 S142 S143 S144` reports 8 HUMAN,
  no product verdicts. `scripts/check` passes (321 tests, clippy clean).
- User approved P030–P033; all marked agreed. t122's evaluation-definition phase is
  complete. Property automation waits for a pinned real observation boundary in a later task.
- User authorized a one-off `pi:github-copilot/claude-opus-5.5@high` plan reviewer;
  workers.toml unchanged. Initial review blocked oversized t123 and evidence assumptions;
  corrections in `docs/research/herdr-observer-plan-review.md`. t123 merged after six
  implementation rounds; final independent review: 0 high, 0 medium, 0 low. 28 smoke tests
  and 6 related/hidden scenarios pass; 321 Rust tests/clippy clean before merge.
  User authorizes continuing and merging green tasks; no need to ask again for green merges.
- Completed: approved context committed on main; t123 headless adapter merged; lead-run
  private live adapter gate verified Pi/Claude capture, two live task origins, wrapped retry
  identity and retirement. Later authenticated Codex probes reproduced wrong-pane
  attribution under shared-daemon reuse; Codex coverage is not passing (details below).
- Next gate: resolve reproduced Codex shared-daemon stale-pane attribution (see
  Waiting for you), or explicitly approve a Pi/Claude-only bounded native spike. Then scope t124 native GPUI patch with test-only read/action hooks.
  t125's full properties also depend on t127's production identity/attempt contract.
  Do not equate synthetic fixtures, stand-ins or HUMAN output with feature acceptance.
- Prototype attribution is a private sidecar, never a production owner/schema change.
  Unknown identity stays unknown; legacy session fields are not fallback. External unwrapped
  id reuse and exact fallback-attempt fencing remain feed gaps, not claimed guarantees.
  No production protocol migration, history archive or interactive workers in this phase.
- Start with existing status/read-edit-run activity; defer interactive worker sessions.
  Do not promise detailed command output or historical streams from the current feed.
- Exact daemon protocol changes are not settled; prototype existing API reads first.
  Source clones in `/tmp/delegate-herdr-research/` and mockups in
  `/tmp/delegate-herdr-mockups/` are temporary; fetch pinned sources again if absent.
- Private preflight probe confirmed actual Pi TUI and Claude startup identity reporting;
  credential-free Codex remained unknown. `docs/research/herdr-observer-live-identity.md`
  records scope/limits. This is not all-harness acceptance or a fallback decision.
- No active delegated tasks remain. t123 merged with task state removed. Full agreed
  observer S137–S144/P030–P033 acceptance remains pending; no native work has started.

### Previous kit merge handoff (2026-11-03)
- User approved; t121 then t120 merged. No open task state remains.
- Post-merge `scripts/check` passes (321 tests, clippy clean). Before merge, 17 related
  scenarios passed, including hidden S136/P029; final independent review: 0 high, 0 medium,
  1 low (accepted below).
- Still pending: upgrade `trial` with the new binary (its kit files are uncommitted).
  secsw-code was upgraded for decisions 031–033, before decision 034.
- Accepted low reporting limitation: when `.claude` or `.claude/skills` is a regular file,
  full upgrade reports affected skills but silently skips blocked pane files and says up
  to date. No data loss.
- Deferred limitations: trailing-slash command arguments; combined canonical/layout takes
  may require two invocations; conservative no-op refusal for symlinked skill ancestors;
  inherited init `.claude` ancestry behavior; crash-interrupted backup rename needs manual
  recovery (no journal).

## Codex phase handoff (scope approved)
- User approved extending the remedy into Codex itself and one independent Opus plan
  review, temporarily through `pi:github-copilot/claude-opus-5.5@high`. Use a one-off
  `delegate plan-review --planner gpt --worker pi:github-copilot/claude-opus-5.5@high`;
  do not change `workers.toml` or silently substitute another reviewer/provider.
- Next session: redesign the S145 check and scope the smallest caller-context remedy
  against pinned Codex/Herdr source, write reviewable specs, then run the approved
  independent plan review before deep implementation. Codex target approval is not
  blanket approval for ownership/schema changes; settle the exact contract first.
- Rejected t129 draft and latest offline reproducers are retained privately at
  `/var/tmp/ho-codex.OAzcs8/rejected-t129/` while available. Treat them as unsafe
  diagnostic input, never apply the draft wholesale. Workers must not access the
  authenticated temporary profile. No task is currently active.

## Investigation baseline
- Authenticated isolated Codex tracing found a real attribution bug: Codex 0.166.0's
  reused app-server/hook context retained the first pane's Herdr environment. A fresh
  private daemon reports the first interactive session correctly; a distinct second
  pane's session reports to the first pane, replacing its identity while the second
  stays unknown. Both prompts completed; the real hook API returned OK. Private Codex
  and Herdr daemons were stopped; no personal profile/daemon was changed.
  User approved S145 and continuing through checks/investigation and a bounded fix.
  Pinned Codex source confirms hooks snapshot the shared server process environment,
  not each TUI caller's. Codex itself is now an approved remedy target; no safe patch
  or exact caller-context contract has been chosen yet. A Pi/Claude-only native spike
  has not been approved as an alternative. Details:
  `docs/research/herdr-observer-live-identity.md`. No production fallback or
  ownership/schema change has been authorized.
- t129's opt-in S145 check draft was discarded after repeated independent review still
  found safety/oracle failures (latest: 5 high, 1 medium). Lead live runs of the drafts
  reproduced A exposing B with distinct completed threads and one daemon, but this is
  exploratory evidence, not an accepted automated regression. S145 remains agreed,
  without a merged check. Rejected patch/reproducers are private temporary diagnostics.
- Deeper corrective work needs a non-GPT plan reviewer. All configured standard/deep
  plan-review candidates are GPT; independent planning attempts refused that set.
  No worker config was changed, no deep implementation started, and no task state remains.
  The user has now approved one-off Claude Opus via GitHub Copilot and the Codex
  remedy target. The exact caller-context architecture still needs design and review.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t124 | Isolated native GPUI sidebar/dashboard patch | observer-2 | todo | t123 + private live identity gate | deep | two lead groups, lead preserved on task selection, read-only lifecycle; opt-in pinned build/native evidence | blocked on Codex shared-daemon attribution remedy or explicit reduced spike scope; include test-only GPUI read/action hooks; no daemon protocol change |
| t125 | Independent observer property checks | observer-3 | todo | t123, t124, t127 | standard | P030–P033 through approved GPUI read/action hooks, with seeded action sequences | max four properties; scope/coverage gaps must be resolved before writing; family independent from implementation |
| t128 | Codex shared-daemon attribution regression and bounded remedy | observer-codex | todo | safe S145 check + independent plan review + remedy design | deep | S145 through real authenticated isolated Codex/Herdr; no wrong-pane attribution | t129 discarded as unsafe; Codex target + one-off Copilot Opus review approved; contract/design pending; blocks full Codex native gate |
| t127 | Production observer identity/instance/attempt contract | observer-contract | todo | prototype evidence + user design | deep | reconcile decisions 009/031/035; stable task instances and per-fallback attempt identity without weakened P030/P032 | design first; no production schema changes authorized by the prototype; blocks full property completion |
| t025 | Codex workers bypass bwrap (`harness.rs`), so they can read the hidden set and the data folder today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | needs a design: bwrap around Codex, or Codex's own read limits |

## Codex remedy constraints (Codex target approved; exact design pending)
- User approved S145 and autonomous continuation through checks/investigation and
  a bounded fix. t129's check draft was discarded as unsafe; no accepted automated
  check or product patch has landed. Redesign the check before another implementation.
- Trace the pinned Codex client → shared app-server → SessionStart hook environment
  boundary. Identify where the current caller's pane context is lost before choosing
  a patch location in Codex or Herdr. Do not assume Herdr's API acknowledgement means
  ownership was correct, or derive ownership from whichever pane is focused.
- Desired bounded remedy: current-launch/thread-scoped Herdr context reaches the real
  hook without leaking one caller's context into another. Preserve normal shared-daemon
  use, existing conversation identities, delegate execution ownership and public schemas.
- Reject daemon restarts per launch, credential copying, one authenticated profile per
  pane, synthetic identities, weaker attribution guards or unknown-as-success as fixes.
  If a safe context channel requires a protocol/schema change, stop for explicit design.
- Establish the agreed S145 failure before implementation, then require the same check
  to pass without diagnostic destination/payload overrides. Add concurrency/resume tests
  before claiming broader production coverage. No native UI work is unblocked yet.

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
- Codex shared-daemon remediation boundary: explicitly choose an upstream Codex
  caller-scoped context change, Codex-TUI-local reporting, or a Herdr launch adapter
  using real thread responses. Existing reviewed SessionStart path has no caller
  hook-environment field. This blocks a safe product remedy and full Codex native
  coverage. Codex is now an approved external target, and one-off Copilot Opus plan
  review is approved beyond the all-GPT configured set. No exact transport/schema
  change or production identity fallback has been authorized yet.
- Production observer identity fallback when no conversation ID is reported remains open.
  Agreed invariants prohibit guessing ownership and require attribution to survive pane reuse
  and resume. Draft prototype uses captured real identity or explicit unknown, not a fallback policy.
- Dashboard visibility scope (this checkout vs other workspaces/hosts), and history after
  merge/discard or lead closure; blocks production UI/retention contracts, not a bounded prototype.
