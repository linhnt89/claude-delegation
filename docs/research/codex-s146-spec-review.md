FINDINGS: 2 high, 4 medium, 5 low

1. **High — t131 (size).** t131 is too big for one reviewable diff. It covers the local SSE stub, the fresh profile and provisioning, the hook digest and `hooks.json` checks, rollout binding, `/proc` fd and daemon fingerprint proof, the PTY/API sampling loop, signal cleanup at every phase, symlink and hardlink guards, the check wrapper, docs and a long list of offline tests. The spec already says "stop for a task split" if the resource layer gets too large, and t129 failed on this same breadth. **Fix:** split it now into three tasks run one after another:
   - (a) the stub, profile, provisioning and hook validation;
   - (b) pure evidence readers (rollout/session binding, `daemon.pid` and `daemon-updater.pid` JSON, `/proc` fd and start-tick fingerprints), tested against fixtures;
   - (c) the single-owner live loop, signals and cleanup, plus the `check` wrapper and docs.

2. **High — t131 (Context and live interface).** The worker has to guess how the live system is driven. Nothing names the Herdr calls for creating a pane, sending input and reading identity (`pane.get` → `agent_session`). Nothing says how to work the TUI's hook-approval prompt, or gives the real `daemon.pid` JSON schema, the rollout JSONL records or the provisioning command. The contract describes these only in prose. The Context list leaves out `docs/research/herdr-observer-live-identity.md` and `prototypes/herdr-observer/`, which already use `pane.get`. The source clones in `/tmp` are gone. Workers may run the opt-in path only if the lead "makes it available", and nothing decides that. The result is a blind implementation whose first real contact is the lead gate, the same pattern that sank t129. **Fix:**
   - The lead captures redacted real-format fixtures: pane create/get responses, `daemon.pid` and updater JSON, rollout lines, `hooks.json`, the approval-screen cues and the SSE exchange. Put them in an in-scope fixtures path.
   - Name the exact API methods in the contract.
   - Decide up front whether the t131 worker may run the credential-free opt-in inside its sandbox.

3. **Medium — contract "Provisioning" / t131.** How a fresh root gets its daemon package is undefined. The contract notes that `daemon update --from-cli` fails on an empty profile, but names no working method. It also doesn't say which version gets downloaded: if it's the latest rather than 0.166.0, every opted-in run is a permanent check error. Every run downloads again because the root is always fresh, and the download counts against the 180 s active deadline. In practice `DELEGATE_CODEX_LOCAL_PROVISION` is always required. **Fix:**
   - A lead preflight pins how the package version is chosen.
   - Give provisioning its own budget under the 300 s limit, or allow a lead-supplied, read-only, digest-pinned package copied into the run root.
   - Document that the provisioning opt-in is always needed.

4. **Medium — contract "Resource and timing policy" vs oracle gap rule.** One synchronous loop does two pane reads plus a daemon sample per cycle, each allowed to take up to 0.5 s. That cycle can legally reach 1.5 s, but the oracle rejects any gap over 1 s, so slow-but-allowed reads turn into spurious check errors. **Fix:** set a per-cycle budget well under the max gap (for example 0.2 s per operation and 0.6 s per cycle over a persistent socket), and state the max gap as a consequence of those numbers.

5. **Medium — t130 / contract "Trace interface".** Several points are ambiguous, and t130 tells the worker to stop on ambiguity, so each one risks a stalled round:
   - Can `setup_valid=false` still yield a fail when wrong-pane evidence is concrete?
   - Does concrete wrong-pane evidence win over read gaps that happened *earlier*? The contract says "later" in one place and "already exists" in another.
   - Missing orderings: `a_launch<a_reply`, `b_launch<b_reply`, and how `threads.*.started/completed` relate to the boundaries.
   - What does "A's first completed observation" mean for where daemon coverage starts?
   - How can a pure oracle "enforce real opaque pane ids"? It can only check that they are distinct and nonempty.
   - Is an Authorization header or `auth.json` a check error or a product failure?
   - Does `a_exit` mean the exit command was sent, or that the TUI was seen to exit?

   **Fix:** settle these in the contract with a small verdict truth table before t130 starts.

6. **Medium — PLAN t128 / missing tasks.** By design the driver requires 0.166.0 with a pinned digest and rejects patched builds ("future patched-build support needs explicit policy"). So S146 can never accept a remedy, and the remedy's implementer must not change the check.
   - t128's acceptance still cites only S145, whose safe check has no task since t129 was discarded.
   - The prior review's patched-build delivery task and concurrency/resume scenario still have no rows.

   **Fix:**
   - Add a task, written by an allowed check-author family, for a reviewed patched-binary version/digest policy in S146 and S145.
   - Add rows for the S145 check, the build/delivery task and the concurrency scenario.
   - Add S146 to t128's acceptance.

7. **Low — t131 Acceptance #2.** `env -u DELEGATE_CODEX_LOCAL_PROBE evals/run S146` exits 0 for both a pass and a skip, so it doesn't prove the skip. Acceptance #3 covers the check directly. **Fix:** assert the runner output shows `skip` (or "1 skipped").

8. **Low — t130/t131 test gating.** `scripts/check` runs only Rust clippy and tests, so in t130's Acceptance it proves nothing about the oracle. After merge, nothing reruns `evals/tests/*.py`; this is the same gap PLAN already lists for `quota_review_check.sh`. The new `evals/tests/` folder isn't documented in `evals/README.md` or the AGENTS.md layout. **Fix:** plan a follow-up that wires these tests into a standing gate, and update the layout docs after merge.

9. **Low — t130/t131 scheduling.** Only the lead enforces the tiers and the order:
   - Neither spec has a tier line, so t131 runs at deep only if `--tier deep` is passed.
   - t130 can step up to deep (GPT) after `step_up_after=5` failed rounds, or fall through to GPT Luna at standard.
   - If t130 hasn't merged, `checks_by: t130` prints "running without that rule" and the run goes ahead.

   **Fix:** run t130 at explicit standard and stop on any GPT pick. Start t131 only after t130's merge with its `Worker:` trailer exists. State `--tier deep` in the PLAN row.

10. **Low — S146 vs contract.** The scenario says a missing opt-in *or missing prerequisites* is an honest skip. The contract makes missing or mismatched inputs after opt-in a check error. Non-Linux hosts are unspecified, and the driver needs `/proc`. **Fix:** state explicitly that missing prerequisites after opt-in are covered by "opted-in broken setup" (exit 3), and that non-Linux exits 77 before any side effect.

11. **Low — docs/decisions.** Codex as a remedy target, the S146 credential-free check design and the check-author family separation exist only in PLAN and research prose. The prior review's low #13 is still open. **Fix:** record decision 036 via `/distill` before the implementation wave.

The oracle/driver split, the fail-precedence and unknown-vs-failed-read rules, the opt-in/77 handling and the lead-only baseline gate all hold up against `evals/run` and decisions 013, 030 and 035. The problems above are about size, missing interface facts and ambiguity, not direction.

## Lead disposition (after review)

- Finding 5: resolved for t130 with a complete precedence table, setup/evidence guards,
  explicit strict boundary ordering/time equalities, a_exit meaning observed exit,
  daemon coverage [a_reply,end], and driver-vs-oracle pane-proof responsibilities.
- Findings 4/7/8/9/10/11: cycle budget corrected; final-loop skip assertion added;
  t130 now installs a standing Python test gate and layout documentation; explicit
  tiers/stop-on-GPT/merged-Worker prerequisites recorded; non-Linux and opted-in
  prerequisite semantics settled; accepted boundaries recorded in decision 036.
- Findings 1/2/3: later live work remains BLOCKED. Oversized t131 state discarded;
  smaller t132 preparation, t133 readers and t131 owner-loop waves are planned.
  Real-format fixtures, exact APIs and a working pinned-provisioning preflight/budget
  are required before their specs can be completed/reviewed or implementation starts.
  The oracle-only t130 does not access these live interfaces and may proceed.
- Finding 6: added explicit t134–t137 build/auth-check/eval-definition/patched-policy
  rows; t128 now names both S145/S146 plus later approved acceptance. Those are not
  ready tasks and do not authorize product/schema changes.
- No additional reviewer invocation was made. This disposition is the lead's scope
  and correction assessment, not a claim that Opus re-reviewed the revised documents.
