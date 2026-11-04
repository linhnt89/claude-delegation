# Plan

<!-- Living and small. Prune finished items: git history remembers them.
     Status: todo | ready (spec written) | running | review | done -->

## Handoff (2026-11-04, real tasks beside the lead, no Sheprd patch — 038 accepted)
- User chose option B (real /delegate tasks under real Pi/Claude leads) and mockup
  variant B: per-task sidebar rows from Herdr pane tokens + a read-only
  `delegate watch --beside` side pane. No Sheprd/Herdr changes, no pin to maintain.
  Decision 038 accepted 2026-11-04; 031/037 Status lines note it.
- Sequential: t139 (`docs/task-specs/t139-herdr-lead-origin.md`) then t124
  (`docs/task-specs/t124-herdr-watch-beside.md`), which reuses t139's Herdr client.
  Both are DRAFTS: independent plan review before either runs.
- Probe (private Herdr 0.9.5, synthetic ids): `docs/research/herdr-sidebar-tokens-probe.md`.
  Key facts: tokens survive a conversation change in the pane, any source can overwrite
  any key, values capped at 80 chars, bidi and U+2028 not stripped, TTL works.
- Workers inherit `HERDR_*`, so a worker's hook can overwrite the lead pane's identity.
  t139 strips them.
- Scenarios done (user-approved): S147 (t139) and S148 (t124) are automated and fail
  until built. Hidden S149/S150 hold their failure paths. Stand-in Herdr socket:
  `evals/lib/herdr_standin.py`; side-pane driver: `evals/lib/watch_pane.py`.
  Plan review (GPT-6.1 Sol): 0 high, 5 medium, 1 low. All fixed. User chose signal-hook,
  isolating unreadable tasks and splitting t124/t140 (decision 039). Hidden S150 revised
  to match. S151 (t140) approved and automated (fails until built).
  Re-review: 0 high, 3 medium (t124/t140 only), fixed: remembered attribution for
  unreadable tasks, `read_tasks` step budget, whole-refresh budget. t124/t140 plan
  review: 0 high, 1 medium (S151 bare `wait`), fixed.
- t139 MERGED (673fb8e, DeepSeek v4.1 Flash, 1 round): S147 + hidden S149 pass.
  Accepted deviation: category C detected without a Unicode table, so scattered
  unassigned codepoints aren't enumerated.
  signal-hook 0.4.6 fetched into the cargo cache by the lead.
- t124 MERGED (a4145f5, DeepSeek v4.1 Flash, 1 round): S148 + hidden S150 pass;
  361 Rust tests. Accepted: `Budget.details` flag, transitive `errno`, bounded merge
  readiness beside the new reader.
- t140 MERGED (ca5fe04, 1 round): S151 + S148 pass. Isolated walkthrough with a
  private Herdr daemon and two real Pi leads passed 17/17. It checked origins, the
  unfocused side pane, per-lead tokens, expand, `/new` and exit clearing rows,
  resuming A in another pane restoring A's rows, and `q` clearing them. Not covered:
  a Claude lead (no credentials in isolation; the print-mode probe passed earlier)
  and how Sheprd renders the rows (the user checks this after pasting
  `delegate watch --sidebar-config`).
- t141 (`docs/task-specs/t141-kit-history-real-kits-only.md`) is running. t140
  hand-added a fake legacy kit to kit-history.txt. t141 moves that need into the
  upgrade tests. Don't run `scripts/kit-history` on main until it merges.
- Later, optional: ask upstream Sheprd for a mod data source (agent identity + external
  data) if per-task clickable rows or native cards prove needed.
- t138 diagnostics remain under
  `/tmp/delegate-codex-deferred.pnZRzk/` (unreviewed, not safe to apply wholesale).

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
| t140 | Interactive side pane keys, `--beside` launcher, `--sidebar-config`, skill text | observer-4 | todo | – | standard | `scripts/check`, S151 S148; lead-run real Pi/Claude walkthrough (S137/S138) | split from t124 by decision 039 |
| t125 | Independent observer property checks | observer-5 | todo | t124, t139, t140 | standard | P030–P033 through status JSON and the side pane's Herdr reports (stand-in socket), with seeded action sequences | max four properties; resolve scope gaps first; family independent from implementation; no full acceptance from reduced coverage |
| t127 | Production observer attempt identity, retention, fallback | observer-contract | todo | t124 in use + user design | deep | per-fallback attempt identity, closed-lead/merge retention, cross-host visibility, without weakened P030/P032 | origin + task instance moved to t139 (038); the rest still design-first |
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
- Later: herdr-app + a delegate mod could beat the Sheprd setup (native clickable rows,
  task panel beside the lead) but needs a herdr-core migration and loses resume-safe
  identity. See `docs/research/herdr-app-delegate-mod.md`; revisit as a decision amending 035.
- Does a Pi lead's `path` identity survive resume unchanged? S142's resume step depends
  on it; verify in the S137–S144 walkthroughs.
- Side pane scope: one checkout (the lead's cwd). Leads delegating into several
  checkouts see only that checkout's tasks; revisit after use.
- Production identity fallback, pane reuse/resume, stable task instance and fallback-
  attempt identity remain open. Use captured real identity or explicit unknown.
- Dashboard visibility across checkouts/workspaces/hosts and retention after
  merge/discard or lead closure block production contracts, not the bounded spike.
- Deferred Codex remedy boundary: caller-scoped Codex context, TUI-local reporting
  or Herdr adapter using real thread responses. No exact transport/schema approved;
  blocks full Codex coverage, NOT the approved Pi/Claude native spike.
