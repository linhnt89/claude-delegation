FINDINGS: 4 high, 7 medium, 4 low

Scope: the plan-review `specs/` folder is empty. I reviewed what exists: the uncommitted `PLAN.md` change, `docs/research/codex-attribution-redesign.md` (untracked), the S145 scenario, the research notes, and decisions 013, 023–025, 028, 030 and 035. I checked them against `harness.rs`, `commands.rs`, `config.rs`, `evals/run`, the live `workers.toml`, and the installed Codex 0.166.0 and Herdr 0.9.5 integration. The pinned upstream source clones in `/tmp` are gone, so I did not re-verify the Codex source claims in §2.

1. **High — design §1 "Workers never receive the private authenticated profile" / PLAN "Workers must not access the authenticated temporary profile".**
   - **Problem:** Nothing enforces this. `harness::sandbox` uses `--ro-bind / /` and hides only `/tmp`, `bounds.hide` and the data dir. From inside this sandboxed review I could list `/var/tmp/ho-codex.OAzcs8/` and confirm `home/.codex/auth.json` is readable. I checked the permissions only and did not open the file. The `rejected-t129/` draft is readable too. Codex-harness workers skip bwrap entirely (t025). The safety claim depends on worker behaviour, not on the sandbox.
   - **Fix:** Before any S145 task starts, do one of these:
     - keep the profile only for the lead-run gate and delete or move it otherwise;
     - add the private parent to the bwrap hide list (spec'd code change) and keep Codex workers off these tasks until t025 is fixed;
     - drop authentication from the check (see finding 11).

2. **High — design §1 work package 1 "Independent family … via `checks_by`" and PLAN t128 (deep).**
   - **Problem:** `implement.deep` in `~/.config/delegate/workers.toml` holds only GPT models (`gpt-6.1-sol` through two providers).
   - If the check packages run at deep tier, they are GPT. A deep t128 spec with `checks_by:` naming them then has no candidate left. `implement_candidates` errors and the run stops.
   - The design also points `checks_by` the wrong way. Packages 1 and 2 are both S145 check code. `checks_by` belongs on the remedy (t128) spec and names those check tasks.
   - **Fix:** Decide the families now. One option: write the check packages with a non-GPT standard-tier candidate (Muse or DeepSeek) and keep t128 on GPT deep. The other: get user approval for a non-GPT deep implement candidate. Then fix the wording to "t128 carries `checks_by: <pkg1>, <pkg2>`".

3. **High — design §1 "Inputs and ownership" versus "Real-interface execution".**
   - **Problem:** Two requirements cannot both hold:
     - Each run gets a fresh, unpredictable run directory with its own HOME and disposable repo, and nothing may be overwritten.
     - The real SessionStart integration must already be installed, and both repo and hook already trusted, inside a persistent CODEX_HOME supplied from outside.
   - Codex keys project trust by absolute path, so a repo at a new path brings back the trust prompt on every run. That leaves three outcomes: a check error every time, the check answering trust prompts, or the check writing to the profile.
   - `herdr integration install codex` writes `hooks.json` and `herdr-agent-state.sh` into CODEX_HOME. That means either the profile gets overwritten, or the hook lands in a per-run `~/.codex` that Codex never reads.
   - Codex itself writes into CODEX_HOME on every start (sessions, sqlite logs, PATH aliases), so "never overwrite existing files" can't apply there.
   - **Fix:**
     - The user prepares the profile once: integration installed and hook approved.
     - The check verifies this read-only, by hash and version, and never installs anything.
     - Use a fixed, pre-trusted repo path under the private parent, or pass trust per run through `codex -c 'projects."<path>".trust_level="trusted"'` if pinned source supports it.
     - List the files Codex is allowed to write in the profile.

4. **High — t128 (remedy).**
   - **Problem:** Codex is now the remedy target, but no task or design covers getting a patched build:
     - where the patch lives (upstream PR, fork, or a patch file in this repo);
     - how `codex-rs` gets built (a cold build of a large workspace needs network, and `~/.cargo` is read-only in the sandbox per Known issues);
     - how S145 picks the patched binary instead of the installed one.
   - `codex` currently resolves through mise `latest`, so the known-bad 0.166.0 baseline can drift without anyone noticing.
   - t128's acceptance ("S145 through real authenticated…") can't run in the runner's sandbox. The remedy worker has no regression it can run, which is the same blind-feedback setup that cost t129 its rounds.
   - **Fix:**
     - Add a build/delivery task, or make it a lead gate the way the earlier GPUI review did.
     - Add an explicit binary input (e.g. a path variable) to S145 and refuse to run on a version mismatch.
     - Give t128 a worker-runnable acceptance: a pinned-source Codex test with two clients on one app-server and a recording hook. The lead-run S145 stays as the final gate.

5. **Medium — review package / `PLAN.md` Codex phase handoff.**
   - **Problem:** The one approved independent Opus review is being spent on a design with no task specs. The design itself says executable Acceptance and filenames are "settled in task specs" later. The t129 failures came from exactly those details (fixture pane ids, missing post-reply windows, hardlinks, control-path checks), so the specs that matter will not get an independent review.
   - **Fix:** Ask the user to approve a second one-off review of the actual package-1 and package-2 specs before workers start, or rerun this review once the specs exist.

6. **Medium — design §1 "Verdict state machine": "Missing opt-in … are check errors (exit 3)".**
   - **Problem:** With no opt-in, `evals/run full` would end in exit 3 on every machine forever, and any delegated run naming S145 would record CheckError. The repo convention is exit 77 (skip) for a missing prerequisite (S006: `exit 77` when `claude` is absent). That still meets S145's rule that missing setup must be "distinguished, not passing".
   - **Fix:** Exit 77 with a clear "not run: opt-in absent" reason. Keep exit 3 for a broken setup after opting in.

7. **Medium — design §1 cleanup and S145 `Timeout: 300`.**
   - **Problem:** `evals/run` wraps the check in `timeout 300` and reports exit 124 as FAIL, a product failure. If the runner's timeout fires, the run is misclassified, which breaks the design's own rule that gaps are check errors. SIGTERM reaches only the check process, so the owned Herdr child and the detached Codex daemon can be left running.
   - **Fix:** Require an internal deadline well under the scenario timeout that returns 3 for timeouts the check itself causes. Also require a SIGTERM/SIGINT handler that runs the same owned cleanup, and a test that sends SIGTERM to the check at each phase.

8. **Medium — design §1 "PASS requires … unchanged daemon".**
   - **Problem:** An unchanged daemon pid and start time prove only that the daemon stayed alive, not that A's and B's threads ran on it. A remedy (or a fallback) that runs B in-process or on another server would pass the "shared daemon" scenario without testing it. PLAN rejects per-launch restarts but doesn't rule out bypassing the daemon.
   - **Fix:** Require proof that each thread was served by the recorded daemon. Examples: the TUI's unix-socket peer resolves to the daemon pid, or the daemon's own thread listing or log contains both thread ids. Add "bypass the shared daemon" to the list of rejected remedies.

9. **Medium — design §2 preferred candidate: "applied only to that client's launch reporting".**
   - **Problem:** Codex's hook runner can't tell which hook is Herdr's. The real `hooks.json` runs several SessionStart hooks (`gh-axi`, `chrome-devtools-axi`, `lavish-axi`, and Herdr's). "Suppress stale Herdr routing" also requires Codex to know Herdr's variable names. As written, the candidate is either Herdr-specific coupling inside Codex or an unscoped env map, and the design rejects both.
   - **Fix:** For the protocol design, consider having each hook declare an allowlist of caller variables it needs (e.g. `HERDR_PANE_ID`, `HERDR_SOCKET_PATH`, `HERDR_ENV`). Codex would take those only from the thread's caller context and remove them from the daemon snapshot when the context is missing. Codex stays Herdr-neutral, and the trusted hook owner limits what passes through.

10. **Medium — t128 acceptance / evals.**
    - **Problem:** S145 runs strictly one after the other (A exits before B starts). Real Codex leads run side by side, and the preferred remedy introduces per-connection/per-thread state where last-writer-wins and same-thread-resume bugs only appear under concurrency. The design calls concurrency and resume tests "mandatory before broader claims", but there is no scenario or task for them, and t128's acceptance cites only S145.
    - **Fix:** Use `/evaluate` to write a concurrent scenario (two live TUIs, plus a resume in the other pane) and a mixed-version case, then add both to t128's acceptance.

11. **Medium — design §1 overall architecture.**
    - **Problem:** The design isn't compared with a credential-free reproduction: real Codex and Herdr binaries, the real hook, one shared daemon, and a private CODEX_HOME pointed at a local stub Responses endpoint (`model_providers` with `base_url`, no OpenAI auth). The bug lives in the hook-environment snapshot, not in the model. Most of the safety machinery that sank t129 exists because the check handles a live authenticated profile: profile exclusivity, credential exposure (finding 1), and interactive login.
    - **Fix:** In the source preflight, check whether a stub provider reproduces A-exposes-B on 0.166.0. If it does, propose to the user an offline sibling scenario that workers can run, keeping S145 as the authenticated lead gate. S145's `scenario.md` must not change.

12. **Low — repo state.**
    - **Problem:** `docs/research/codex-attribution-redesign.md` is untracked and `PLAN.md` is modified. Task worktrees branch from committed main, so specs that cite the design won't find it. The earlier review made committing context to main mandatory before workers start.
    - **Fix:** Commit both before creating any task.

13. **Low — decision 035 and the Codex target.**
    - **Problem:** Decision 035 targets only `herdrdev/herdr` and `penso/herdr-gpui`. Approval of Codex as an upstream target, plus a possible new protocol contract, appears only in PLAN prose.
    - **Fix:** Record a decision through `/distill` (Codex target, S145 check architecture, the remedy-boundary choice once the user picks it).

14. **Low — PLAN `Next` table.**
    - **Problem:** t128 depends on a "safe S145 check" and a "source preflight", but neither check package nor the preflight has a task id or row. The waves and dependencies can't be tracked.
    - **Fix:** Add rows for the preflight (research), the package-1 oracle and the package-2 driver, with tiers matching finding 2, and point t128's dependencies at those ids.

15. **Low — design §1 process ownership.**
    - **Problem:** The live-identity notes record a leftover Codex updater from a private startup that had to be killed separately. The cleanup design covers only the Herdr child and the app-server daemon.
    - **Fix:** Turn off Codex's update check for the private profile through config. Have cleanup verify that no process remains whose environment or command line references the run's CODEX_HOME or run directory, and return a check error (not PASS) if one does.

The split into a pure oracle and a separate driver, the verdict rules, and the remedy guardrails in PLAN (no restart per launch, no unknown-as-success, stop before any schema change) are sound and consistent with S145.
