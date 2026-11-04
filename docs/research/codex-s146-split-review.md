FINDINGS: 2 high, 6 medium, 5 low

The three specs agree with each other on the important rules: the split between the oracle and the driver, the opt-in and exit-77 handling, the lead-only baseline gate, and keeping the driver's family apart from t130's (t130 was written by DeepSeek, so `checks_by: t130` leaves the GPT deep pool for t131). t132 and t133 touch different files and can run in parallel. The problems below are about what the live gate will actually meet and whether the acceptance checks work.

**1. High — t131 (and `codex-s146-check-contract.md`): the run root can't hold Unix sockets.**
- **Problem:** The run root has to sit under `EVALS_WORK`. Under the default `evals/run`, `EVALS_WORK` is `<tasks dir>/_evals/results/work/S146`, which is 124 bytes here. A Unix socket path can be at most 108 bytes.
  - Herdr's socket (`<root>/config/herdr/s`) and Codex's control socket under `CODEX_HOME/...` will always be too long.
  - The spec makes that overflow a check error, so the lead's gate ("unchanged `evals/run S146`") can never give the required FAIL.
  - `prototypes/herdr-observer/README.md` already warns: "Use a short private path so UNIX socket paths fit."
  - Every preflight ran under `/tmp`, which is tmpfs. The run root under `EVALS_WORK` sits on the same `/home` filesystem as the mise-installed CLI, so the package executable's `nlink=1`/no-hardlink assumption has never been tried there.
- **Fix:** Settle this in the contract before workers start. Either:
  - the gate is documented to run with a short `EVALS_RESULTS` (for example `mktemp -d /tmp/s146.XXXX`), and t131 checks the longest socket path before any side effect and exits 3 with that instruction; or
  - the run root may be a short owner-only `mkdtemp` outside `EVALS_WORK`, with evidence kept under `EVALS_WORK`.

  Either way, record the Codex control-socket path in a fixture.

**2. High — t131: still too big for one reviewable diff.**
- **Problem:** After the split, t131 still holds all of these:
  - a one-shot Herdr RPC client;
  - the trusted file and `/proc` collectors (no-follow walks, before/after rechecks, rollout fd inode/device binding);
  - pidfd-based process control;
  - provisioning orchestration and hook-screen approval;
  - the sampling loop, signals and cleanup at every phase;
  - the wrapper and docs;
  - offline tests for all of it.

  That is the same breadth that sank t129.
- **Fix:** Move the trusted acquisition and process-control helpers, plus the RPC client with envelope validation, into their own task, for example t138, in their own files with synthetic tests. It depends only on interfaces, so it can run alongside t132/t133. t131 keeps the owner loop, signals, cleanup, wrapper and docs.

**3. Medium — t132: needed interface facts are outside its Context.**
- **Problem:**
  - The spec asks for a config that "matches captured local configuration". That TOML is only in `docs/research/codex-credential-free-attribution.md`, which isn't in Context ("read these, nothing more").
  - The exact syntax of the repo trust key isn't recorded anywhere.
  - There's no fixture for the `daemon start` stdout, the package tree (does a `current` symlink exist?) or the name of the `app-server-control` socket directory.
  - Offline tests can't catch a wrong key, so the first real contact would be the lead gate.
- **Fix:** Add redacted fixtures for `config.toml`, the start stdout and the package layout, and add the investigation doc to Context.

**4. Medium — t132/t131: Codex runs the managed binary before validating it.**
- **Problem:** `validate_managed_package` takes `managed_version` as input. t131 can only get it by running `--version` on the `managedCodexPath` from the start stdout, before anyone has checked that path's containment or digest.
- **Fix:** Split the API.
  1. `resolve_managed_package(codex_home, start_response, expected_digest) -> {executable, sha256}`.
  2. t131 runs `--version` on the validated path.
  3. A separate version check follows.

**5. Medium — t131: cleanup verification can't be done as written.**
- **Problem:** The daemon and updater are detached (ppid 1 in `processes.json`).
  - "Track child ancestry" can't find them, or their short-lived network children after reparenting.
  - "Reap" is impossible for processes that aren't the driver's children.
  - The spec forbids scanning other processes' environments.

  So "verify no run-owned process remains" has no allowed method.
- **Fix:** Specify one of:
  - `PR_SET_CHILD_SUBREAPER` through ctypes `prctl`, set right after the opt-in gate, so every descendant reparents to the driver and can be listed through `/proc/self/task/*/children` and reaped; or
  - a `/proc` scan limited to same-uid processes whose `exe` is under the run's package root.

**6. Medium — t131: hashing the daemon executable doesn't fit the sampling budget.**
- **Problem:** Trusted acquisition includes the SHA-256 of `/proc/PID/exe`. The pinned Codex binary is 319 MB; hashing it took 0.17 s with a warm cache here, longer when cold. Done on every daemon sample, that breaks the 0.2 s per-operation and 0.6 s per-cycle budgets.
- **Fix:** State that the digest is computed once per attested instance, on a held descriptor or pidfd. Each cycle's daemon sample then only rechecks pidfd liveness, boot id, start ticks and the PID record.

**7. Medium — t133: real rollouts may be wrongly judged ambiguous.**
- **Problem:** The rollout fixtures leave out "unrelated records", but those are the records the ambiguity rules depend on. Real Codex rollouts normally also contain `event_msg` `user_message`/`agent_message` (duplicating the prompt and reply), `turn_context`, `token_count` and reasoning items. "Additional user/assistant message → ambiguous" could then fail every real rollout, and only the lead gate would show it.
- **Fix:**
  - Say that only `response_item` messages with role user or assistant count for prompt, reply and ambiguity.
  - List the record types that are tolerated.
  - Add an unfiltered redacted real rollout fixture that keeps record types and order.

**8. Medium — t131 Acceptance #2: the pipeline fails spuriously.**
- **Problem:** In `evals/run S146 | tee /dev/stderr | grep -Eq "1 skipped"` under `pipefail`, `grep -q` exits on the match. `tee` then gets SIGPIPE writing the trailing `results:` and `(no hidden…)` lines, and the pipeline returns 141. I reproduced 141 three times out of three with a simulated delay. The runner would record it as a product failure.
- **Fix:** Capture the output and grep it without a pipe, e.g. `out=$(evals/run S146); printf '%s\n' "$out"; grep -Eq ' 1 skipped' <<<"$out"`.

**9. Low — t131 wrapper path.**
- **Problem:** "execs python3 evals/lib/codex_identity_live.py" is a path relative to the repo root. `evals/run` runs checks from inside `EVALS_WORK`, so it breaks there. In Acceptance #3 the check is run directly with no `EVALS_LIB` set.
- **Fix:** Resolve the module relative to the check script's own directory.

**10. Low — t131 Context: how to start Herdr isn't named.**
- **Problem:** The specs never give the command that starts the headless Herdr server. `herdr server` with `HERDR_SOCKET_PATH`, and the readiness wait, are documented in `docs/research/herdr-observer-live-identity.md` and `prototypes/herdr-observer/README.md`, which aren't in Context.
- **Fix:** Add both to Context, or put the command into `codex-s146-live-interfaces.md`.

**11. Low — t132/t131 tests depend on wall-clock time inside the shared gate.**
- **Problem:** These tests will run in `scripts/check` for every future task:
  - t132's "incomplete >2 s is invalid" stub test;
  - t131's 0.2/0.6 s deadline tests and signal tests.

  Real sleeps slow the gate and become flaky under load, as P018 already does.
- **Fix:** Require an injected monotonic clock for timing logic and put a limit on the suite's runtime.

**12. Low — t133 `bind_launch`: `observed_at` timing.**
- **Problem:** The spec doesn't say whether `observed_at` is taken before or after the inventory is read. If it's taken before, a session created during the read has a timestamp later than `observed_at`, and the sole candidate is rejected. That gives an intermittent check error.
- **Fix:** Say `observed_at` is taken after the snapshot read, and define what happens to a sole candidate outside the window (pending or error).

**13. Low — `evals/tests/README.md` will go stale.**
- **Problem:** The README describes each test file, but no task's Scope includes it. After merge it will be missing three test files and three new `evals/lib` modules.
- **Fix:** Add it to t131's Scope, or fix it on main right after each merge.

## Lead disposition

All findings accepted; corrections before worker starts (not a claimed Opus re-review):
1. Gate now requires fresh short EVALS_RESULTS under /tmp; longest Codex socket
   preflight before allocation; default long root gives actionable check_error.
   Added actual provisioning stdout/layout/socket fixture. The known-good /tmp
   filesystem yields nlink=1; never silently accept hardlinks on another filesystem.
2. Extracted reviewed acquisition/process-control/RPC responsibilities into t138
   with separate module/tests. It depends on merged t133 rather than duplicating
   parsers. t132/t133 run together; t138 follows t133; t131 only assembles components.
3. Added config.toml/provisioning.json fixtures and investigation Context pointer.
4. Split resolve_managed_package (containment/digest first) from pure
   validate_managed_version; execute --version ONLY after safe resolution.
5. Chose subreaper via stdlib ctypes/prctl before any child, owned descendant
   enumeration/adoption and waitpid reaping; no arbitrary personal environ scan.
   Resource primitives live in t138, owner cleanup policy stays in t131.
6. Held-descriptor digest once per instance; stable identity/liveness per cycle.
7. Full 14-record real rollout retained with content/IDs redacted; message rules
   count ONLY response_item messages. Explicitly tolerate duplicate event records,
   reasoning/context/token records; no event-derived double counting.
8. Skip assertion now captures output before grep, with no pipe/SIGPIPE race.
9. Wrapper resolves ../../lib relative to its own scenario directory.
10. Exact [HERDR_BIN,'server'] command/readiness specified in t131.
11. Inject monotonic clocks for timer tests; no multi-second sleeps or strict
    loaded-machine timing assertions; short isolated subprocess integration tests.
12. observed_at is sampled AFTER bytes; out-of-window sole candidate is error.
13. t131 updates evals/tests/README.md; lead also updates it after earlier merges.

This implements the review's requested extraction using already-reviewed resource
contracts; no new live/product/schema surface or further Opus invocation. All live
work remains lead-only; workers use synthetic subprocess/loopback tests. Remaining
implementation risks are subject to independent final-commit reviews and the
unchanged lead-run expected-failing S146 gate, not fixture success alone.
