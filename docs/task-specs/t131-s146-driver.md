# Task t131: S146 final sampling loop and owned cleanup

checks_by: t130
size: long

## Goal
Assemble reviewed preparation and evidence components into a bounded real-interface
owner loop whose unchanged live baseline gate exposes Codex wrong-pane attribution.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-s146-live-interfaces.md`
- `docs/research/codex-s146-provision-preflight.md`
- `docs/task-specs/t132-s146-preparation.md`, `docs/task-specs/t133-s146-evidence.md`
- `docs/task-specs/t138-s146-resources.md`
- Merged `evals/lib/codex_identity_{oracle,prepare,evidence,resources}.py` and tests
- `evals/lib/fixtures/codex-identity/`
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`

## Readiness / tier
Explicit `--tier deep`. Do not run until t132/t133/t138 merge and their APIs match the
specs reviewed alongside this task. t130 non-GPT Worker trailer is verified in
6039a94; checks_by enforces driver/oracle family separation. No worker live opt-in:
workers run offline synthetic-process/socket tests only, never pinned real binary
provisioning, auth, sandbox escape or personal profile access. Lead runs live gate.

## Scope
- In: `evals/lib/codex_identity_live.py`, `evals/tests/test_codex_identity_live.py`,
  `evals/scenarios/S146-codex-local-provider-pane-attribution/check`,
  `evals/lib/codex-identity-live.md`, `evals/tests/README.md`
- Out: t130 oracle/tests, preparation/evidence components and their tests, lead-owned
  fixtures, scenario.md, S145, product/Rust/kit code, worker configuration, deps/auth.

## Interfaces and owner flow

Use existing `evaluate(trace)` unchanged. Use t132 create_profile/LocalStub/
resolve_managed_package/validate_managed_version/validate_hook/assert_no_auth and t133 bind_launch/
parse_pid_record/parse_proc_stat/attest_process exactly as specified; no duplicated
parser/stub/profile machinery or heuristics. Internal dependency injection is fine
for offline tests; LIVE CLI never accepts traces, fake events, alternate factories,
synthetic panes or fake-binary test switches. Use t138 read_owned/Subreaper/
ProcessHandle/RpcCall unchanged: do not implement file/proc collectors, pidfd
signaling or RPC framing in this task. All timer-driven logic has an injectable
monotonic clock for tests; live CLI uses real time only. No custom binary supplied outside
explicit pinned path+version+digest gate. APIs used from CLI cannot bypass pins.

CLI takes no args; wrapper locates ../../lib/codex_identity_live.py relative to
its OWN scenario script directory, not cwd or EVALS_LIB. It works directly and from evals/run's work directory.
No opt-in (`DELEGATE_CODEX_LOCAL_PROBE` !=1) or non-Linux: 77 before resolving
binaries, creating files, connections, imports with side effects or reading any
profile. Opted-in Linux with missing inputs: 3, not skip. Require provisioning
opt-in ==1 before any preparation start. Require explicit absolute CODEX/HERDR_BIN
vars, canonicalized once and matching baseline versions/digests in live-interfaces
(not PATH/latest or version-only). Reject unexpected args. Require EVALS_WORK to
be an existing safe directory after opt-in. BEFORE allocation, check longest
planned socket path (including random root suffix, home/.codex/app-server-control/
app-server-control.sock) fits Linux's 107-byte pathname maximum. If not, exit 3
instructing the user to set EVALS_RESULTS to a short fresh /tmp path. The documented
live gate ALWAYS sets EVALS_RESULTS=$(mktemp -d /tmp/s146.XXXX). Then allocate an
exclusive 0700 short run root under EVALS_WORK, never reuse caller HOME/CODEX_HOME/
output path. Never fallback to an external/default root/socket.
Build children only from t132.env, not the lead's account environment.

Phases, all owned by one synchronous loop with nonblocking IO:
1. Validate inputs; establish t138.Subreaper BEFORE spawning any child. Allocate
   root/stub/profile; initialize disposable git repo with
   explicit fake name/email and empty commit; use private cwd for all commands.
2. Prepare managed package via pinned CLI app-server daemon start, <=60s within
   shared <=180s active deadline; capture bounded stdout/stderr to exclusive private
   files. FIRST call resolve_managed_package to verify returned path containment
   and digest, THEN run returned executable --version and validate_managed_version.
   Never execute the unvalidated path from start stdout.
   Verify fresh control/PID and process identity before private stop; stop/reap
   preparation daemon and updater completely, <=10s inside active budget.
3. Spawn pinned private Herdr via `[HERDR_BIN, 'server']` with t132.env/cwd and
   wait for its exact private socket (the owned child must remain alive). Install
   genuine integration with explicit binary/private env; validate_hook/assert_no_auth.
   No hook rewrite, destination override, synthetic identity or trust shortcut.
4. workspace.create yields distinct actual A/B pane IDs and run repo cwd. Ordinary
   Herdr requests each use a NEW nonblocking Unix connection (server closes after
   response); validate id, result.type and returned pane binding. No ok-envelope
   assumption. Use pane.send_input to submit shell-quoted /bin/bash --noprofile
   --norc wrapper around pinned Codex --no-alt-screen and the exact fixture prompt.
   Wrapper writes an exclusive run-owned exit marker only AFTER the actual TUI
   child exits; verify shell wrapper exit via marker+observed foreground-shell
   return/process state, not merely sent Ctrl-C. Neither daemon nor hook reporting
   destination may be changed by the wrapper. Inventory rollout paths before launch.
5. At A launch record wall+monotonic boundaries. Poll real hook-review screen and
   approve option 2 only with exact captured cues and revalidated sole genuine hook;
   once/profile. Poll snapshots via bind_launch; attest serving process by exact
   open rollout inode/device. Observe A completion using monotonic time AFTER valid
   independent evidence. Sample pane identity and daemon throughout. Observe A's
   required postreply/grace dwell before sending Ctrl-C as needed and observing exit.
6. Keep pane A alive as a shell, launch B only after confirmed A TUI exit. Bind B's
   distinct completed rollout and same-serving-process evidence independently.
   Observe both panes throughout B and maintain required final dwell/coverage.
   A may retain A/be unknown after exit, but must never become B. Do not stop/restart
   shared daemon during A->B. Never guess thread ids from API, terminal or stub.
7. Always enter cleanup, then call oracle on complete trace. If both independent
   threads cannot be established, return check_error, not a fabricated product fail.
   A concrete attested counterexample keeps oracle fail precedence even if later
   IO/continuity/cleanup/interruption fails. Never coerce setup invalid into fail.

## Sampling and acquisition

Follow check-contract's exact trace schema/time equality, grace/dwell/cadence,
actual pane IDs and identity namespace. Target <=0.2s cadence; entire two-pane+
daemon cycle <=0.6s; per request <=0.2s including connect/write/read. Use absolute
monotonic deadlines, no accumulating independent timeouts. Retain failed reads;
a missing read is NOT explicit unknown. Keep failed daemon samples too. A cycle
budget overrun becomes a recorded gap/error, never synthesize intermediate samples.
Every wait phase services stub.poll and observes deadline/signals; no sampling
threads or forgotten subprocess.run blocking calls. Bound all stdout/stderr/API/
rollout buffers; overflow is check_error. Define conservative finite limits in docs.

Use t138's trusted file/proc acquisition and exact process handles; its constructor
hashes each attested instance ONCE and caches held-executable identity. Routine
continuity samples recheck identity/liveness, never hash 319MB per cycle. Partial
snapshots/races remain errors. Tokenize its fingerprint privately and compare
actual serving instances across both replies. Monitor from A reply through end,
not just launch. No legacy filenames/newest-file heuristic or duplicated parsers.
Take bind_launch observed_at wall time AFTER reading the inventory snapshot.

## Signals, ownership, cleanup

Install SIGTERM/SIGINT handlers early (after side-effect-free platform/opt-in gate);
handlers only request interruption, loop enters same cleanup from every phase.
Never pass after interruption. <=180s total active, cleanup <=60s, final output
before 300s scenario outer timeout. Cleanup continues servicing required IO/stub
until appropriate teardown, never extends deadline for each resource separately.

Track every acquired resource immediately, including partial acquisitions. Subreaper
adopts detached descendants, so enumeration through /proc/self/task/*/children and
owned child chains is sufficient without scanning personal environments. Reap only
actual/adopted children, not nonchild processes; restore subreaper flag after cleanup.
Stop
TUIs, validate control files again before dispatching exact-profile daemon stop,
terminate owned updater, stop/reap Herdr, close all sockets/PTY/marker/evidence FDs,
close stub and verify no run-owned process remains. pidfd or exact owned Popen
handles required for signaling; acquire pidfd then recheck boot/start/exe/env/argv.
No process-name/global kills, default sockets, guessed owners or broad rm.
Track child ancestry and run-package executables; inspect private env only after
establishing run ownership, not arbitrary personal process environments. Bounded
wait for tracked transient updater/network children; leftovers set cleanup_ok=false,
never signal arbitrary processes because they inherited a matching env string.
Do not delete evidence/root to hide a leftover resource. No raw auth/session/request
contents in stdout; private root files remain mode 0600/0700 under EVALS_WORK.

Output only redacted verdict/reason/issues. Exit 0 pass, 1 concrete product fail,
3 check_error, 77 honest skip. Actionable setup diagnostics may name the missing
input variable, never raw IDs/account paths/request bodies or traceback. Document
pins, opt-ins, honest offline/live distinction and limits. No patched-build support;
that remains t137. S146 remains agreed until lead observes unchanged real verdict.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE bash -c 'out=$(evals/run S146) || exit "$?"; printf "%s\n" "$out"; grep -Eq "1 skipped" <<<"$out"'`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE bash -c 'evals/scenarios/S146-codex-local-provider-pane-attribution/check; test "$?" -eq 77'`
- [ ] `scripts/check`
- [ ] Offline synthetic-resource tests exercise successful owner flow, real RPC
  envelope/framing and close-per-request, failure/partial-acquisition in every phase,
  SIGINT/SIGTERM at setup/A/transition/B/cleanup, fixed deadlines, partial IO,
  continuity changes, exact-owned updater and transient child teardown, read gaps,
  no-opt-in/non-Linux no-side-effect gates and malicious control/marker links.
- [ ] Independent implementation review has no unresolved high findings.
- [ ] LEAD ONLY before merge: unchanged `evals/run S146` with a fresh short
  `EVALS_RESULTS=$(mktemp -d /tmp/s146.XXXX)` and explicit pinned inputs
  and both opt-ins genuinely FAILS on known-bad wrong-pane attribution, with two
  independently completed distinct threads, same serving daemon and verified
  cleanup. Skip/check_error/missing identity alone is not this baseline gate.

No worker runs live binary gate. Do not fix the baseline product failure, weaken
oracle/scenario, change external product/protocol or add dependencies. Stop on
ambiguous contracts. Do not commit; runner commits. Report family/tests/limits/doc
conflicts; never include raw private evidence in report.
