# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Handoff (2026-11-04, Pi/Claude native dashboard first)
- User approved decision 037: build the bounded native dashboard for Pi/Claude;
  defer Codex attribution/check/remedy work. The goal is the viewer beside the lead,
  not completing the Codex regression framework first.
- Target remains installed `herdrdev/herdr` + `penso/herdr-gpui` (decision 035).
  Lead left, read-only task dashboard right; sidebar tasks nest under the launching
  lead conversation. `delegate` retains execution/lifecycle ownership.
- Codex attribution is explicitly unsupported/unknown in this slice. Never group
  its tasks using the known-stale pane identity or guess a launching lead. This is
  an observer-support boundary, not permission to change worker execution.
- NEXT SESSION: scope t124's isolated Pi/Claude native GPUI patch against pinned
  sources; map bounded evidence to S137–S144 without weakening agreed scenarios;
  pin test-only native read/action hooks and independently review the plan before
  delegation. No native implementation has started.
- No active delegated tasks remain. t138 was finished but unreviewed and had no
  runner-recognized Acceptance heading (`Tests / acceptance`). Its code was NOT
  merged. Saved temporary diagnostic patch/spec/report under
  `/tmp/delegate-codex-deferred.pnZRzk/`, then discarded t138. Discarded never-run
  t131 state too. Archived draft is not accepted or safe to apply wholesale.
- Keep one session per phase: this scope/cleanup handoff is complete; use a fresh
  session for native-spike scoping. User already authorizes green task merges.

## Observer baseline and limits
- Merged t123: opt-in headless adapter in `prototypes/herdr-observer/`, not native UI
  or a production ownership/protocol change. Research/pins:
  `docs/research/herdr-delegation-observers.md`.
- Lead-run private gate verified actual Pi/Claude capture, two live task origins,
  wrapped retry identity and retirement. Evidence/limits:
  `docs/research/herdr-observer-live-identity.md`. Later Codex probes reproduced
  wrong-pane attribution; Codex support is not passing and is now deferred.
- User approved S137–S144 and P030–P033. The eight human walkthroughs yield HUMAN,
  not feature verdicts. Full agreed observer/property acceptance remains pending;
  no all-harness completion is implied by the reduced native slice.
- Private sidecar only. Unknown stays unknown; legacy session fields are not a
  fallback. External unwrapped id reuse and exact fallback-attempt fencing remain
  feed gaps. Production identity/instance/attempt contract t127 still blocks full
  properties, not the bounded native spike.
- Start with task status and normalized read/edit/run activity. No production schema
  or daemon protocol migration, history archive, interactive workers, detailed
  command output promise or permanent completed-task stream in this phase.
- Source clones `/tmp/delegate-herdr-research/` and mockups
  `/tmp/delegate-herdr-mockups/` are disposable; fetch pinned sources if absent.

## Next
| id | task | wave | status | depends on | tier | acceptance | notes |
|----|------|------|--------|------------|------|------------|-------|
| t124 | Isolated Pi/Claude native GPUI sidebar/dashboard patch | observer-2 | todo | t123 + recorded Pi/Claude live identity evidence + bounded spec/review | deep | two Pi/Claude lead groups, lead preserved on selection, read-only lifecycle; opt-in pinned native evidence | NEXT; reduced scope approved in 037; Codex unsupported/unknown; pin native read/action hooks; no daemon protocol change |
| t125 | Independent observer property checks | observer-3 | todo | t123, t124, t127 | standard | P030–P033 through approved GPUI read/action hooks, with seeded action sequences | max four properties; resolve scope gaps first; family independent from implementation; no full acceptance from reduced coverage |
| t127 | Production observer identity/instance/attempt contract | observer-contract | todo | prototype evidence + user design | deep | reconcile decisions 009/031/035; stable task instances and per-fallback attempt identity without weakened P030/P032 | design first; no production schema changes authorized; blocks full properties, not bounded spike |
| t025 | Codex workers bypass bwrap (`harness.rs`), so they can read hidden scenarios/data today | – | todo | – | deep | a Codex variant of S048 (to write with the user) | separate sandbox design; unchanged by reduced observer scope |

## Deferred Codex groundwork
Decision 037 pauses this wave; it is NOT on the native dashboard critical path.
Resume only after explicit reprioritization. Decision 036's safety/independence
constraints remain intact. No retained active task state.

- Already merged: t130 pure oracle (6039a94, DeepSeek), t132 preparation/stub
  (03a861a, GPT), t133 pure evidence readers (4924f26, DeepSeek). Preserve them.
  Last main `scripts/check`: 193 Python tests, 321 Rust tests, clippy clean.
- t132 final independent review: 0 high/0 medium/1 low, accepted test-coverage gap:
  shared-stub malformed-request subtests cannot individually detect every missing
  invalid-flag mutation; current branches verified correct. Review retained in
  `docs/research/codex-s146-prepare-final-review.md`.
- t133 final independent review: 0 high/0 medium/0 low. Full-turn binding/output,
  strict timestamp subset and numeric validation have regression tests. Review:
  `docs/research/codex-s146-evidence-final-review.md`.
- t138 reported 18 offline resource tests passing, but was not independently reviewed
  or runner-accepted. Archived/discarded with user approval; no live S146 verdict.
- Real credential-free and authenticated probes reproduced Codex 0.166.0 / Herdr
  0.9.5 publishing B's identity on A under shared-daemon reuse. S145/S146 remain
  agreed WITHOUT merged live checks or accepted automated verdicts; no product fix.
- Research/fixtures/spec pointers: `codex-credential-free-attribution.md`,
  `codex-s146-live-interfaces.md`, `codex-s146-check-contract.md`,
  `codex-s146-split-review.md` under `docs/research/`; `docs/task-specs/`;
  `evals/lib/fixtures/codex-identity/`. These are not permission to resume the wave.
- Operational facts if resumed: ordinary Herdr RPC closes after one response;
  live gate needs short private EVALS_RESULTS for Unix sockets; fresh provisioning
  uses `app-server daemon start`, not update; verify path/digest before executing
  managed --version; stop exact private updater too. No personal credentials.
- Exposed authenticated temporary CODEX_HOME was deleted with user approval.
  Rejected t129 diagnostics, while present under
  `/var/tmp/ho-codex.OAzcs8/rejected-t129/`, are unsafe inputs, not a patch to apply.
- Prior one-off Opus reviews are consumed; do not silently reuse authorizations,
  substitute providers or edit workers.toml. GPT-authored check components block
  the current GPT-only deep remedy pool; request independent non-GPT capacity if
  the remedy is reprioritized. Codex target approval is not schema-change approval.

| id | task | status | gates retained for later |
|----|------|--------|--------------------------|
| t138 | Trusted acquisition, owned-process/subreaper handles and one-shot RPC | todo | deferred; fresh task + fix Acceptance heading + synthetic tests + independent review; archived draft unreviewed |
| t131 | S146 final owner loop, signals/cleanup and wrapper | todo | deferred; merged t138/peer APIs + checks_by t130 + expected-failing unchanged live baseline and verified cleanup |
| t134 | Pinned external Codex build and patch delivery | todo | deferred; approved remedy contract + reproducible baseline/patched digests and source tests |
| t135 | Safe authenticated S145 check | todo | deferred; reviewed S146 components + separately isolated user login; no authenticated profile retained |
| t136 | Concurrent/resume and mixed-version evaluation definition | todo | deferred; user design and scenario approval before remedy acceptance |
| t137 | Check-author-owned patched-binary version/digest policy | todo | deferred; t131/t135 + delivery contract; independent check-author family |
| t128 | Codex shared-daemon regression and bounded remedy | todo | deferred; all prior gates + independent author/review + exact design; S145 AND S146 and approved concurrent/resume/mixed-version checks |

Never fix attribution by per-launch daemon restarts, credential copying, per-pane
login profiles, synthetic/manual identities, unknown-as-success or weaker guards.
A new caller-context protocol/schema still requires explicit user design approval.

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

## Open questions
- Pi/Claude native spike: exact isolated GPUI build, task-data bridge and test-only
  read/action hooks need a bounded spec and independent review before t124 starts.
- Production identity fallback, pane reuse/resume, stable task instance and fallback-
  attempt identity remain open. Use captured real identity or explicit unknown.
- Dashboard visibility across checkouts/workspaces/hosts and retention after
  merge/discard or lead closure block production contracts, not the bounded spike.
- Deferred Codex remedy boundary: caller-scoped Codex context, TUI-local reporting
  or Herdr adapter using real thread responses. No exact transport/schema approved;
  blocks full Codex coverage, NOT the approved Pi/Claude native spike.
