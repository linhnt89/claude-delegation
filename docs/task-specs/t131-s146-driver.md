# Task t131: Credential-free real Codex/Herdr S146 driver

checks_by: t130
size: long

## Goal
Drive the agreed S146 through genuine interactive Codex and Herdr in a fresh private
profile, with a local model stub, without repeating t129's resource/oracle failures.

## Context (read these, nothing more)
- `AGENTS.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-credential-free-attribution.md`
- `docs/research/codex-attribution-redesign-review.md`
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`
- `evals/lib/codex_identity_oracle.py`, `evals/tests/test_codex_identity_oracle.py`
- `evals/run`, `evals/README.md`

## Scope
- In: `evals/lib/codex_identity_live.py`, `evals/tests/test_codex_identity_live.py`,
  `evals/scenarios/S146-codex-local-provider-pane-attribution/check`,
  `evals/lib/codex-identity-live.md`
- Out: t130 oracle/tests, scenario.md (lead marks automated only after real baseline),
  S145, Rust/product code, kit, worker config, personal or retained diagnostic files.
  Never read/apply rejected t129 drafts or copy the exploratory probe wholesale.

## Contract
- Implement the complete pinned inputs, independent evidence, trace, provisioning,
  resource/cleanup and timing policy in the context contract. No new dependencies.
- Prerequisite: t130 merged and its author family verified compatible with this
  task's checks_by. Do not bypass family exhaustion via an explicit worker override.
- Wrapper executes the live driver only; no fake-binary, injected-trace or offline
  fixture mode accepted by the live CLI. Unit tests may inject adapters internally.
- No opt-in exits 77 before side effects; explicit opt-in requires canonical absolute
  binaries and a second explicit provisioning opt-in. No PATH/latest fallbacks,
  inherited credentials or preexisting CODEX_HOME. Version/digest evidence is private.
- New run-owned HOME/CODEX_HOME/XDG/repo only. Prove exclusive root/control-file
  ownership, reject symlinks/hardlinks/unexpected control layout before stop. Never
  chmod/truncate preexisting output/lock/evidence entries. Profile files that Codex
  itself updates are allowed only inside that fresh exclusive root.
- Local Responses stub returns minimal ordinary SSE text/completion events; record
  and reject Authorization headers. Validate no auth.json, unchanged genuine hook
  script digest and structurally inspected sole SessionStart declaration; approve
  only that private hook. Real pane ids come from create responses. No focused-pane
  inference, payload/destination override or manual identity report.
- The first actual TUI must seed the shared daemon with A's true pane environment.
  Any provisioning preparation daemon/updater must be stopped before A starts. Verify
  selected managed executable path/version/digest. No restart or bypass between A/B.
- Bind independently evidenced distinct real threads and completed user turns as
  specified. Compare validated daemon PID/boot/start identity through A/B transition;
  attest each thread's rollout fd belongs to that same managed serving process.
- One synchronous owner samples both real panes and daemon state at the contract's
  cadence with bounded nonblocking API/PTY operations, timestamps failed reads and
  retains all required phases. The oracle alone decides attribution from the trace;
  no live fallback, ad hoc relaxed predicate or fixture-only shortcut.
- Internal active/cleanup deadlines precede outer timeout. SIGTERM/SIGINT trigger
  bounded owned cleanup at every phase. Exact process identity for daemon/updater
  signals; never process-name/global-server kills. Join/reap all owned resources,
  verify no run-profile processes remain, then finalize private evidence/verdict.
- Driver returns 0/1/3 for oracle pass/fail/check_error; 77 only for absent opt-in.
  Never suppress concrete independently attested wrong-pane failure due to later
  coverage/cleanup problems; report all issues without raw ids or paths.
- Offline tests exercise the resource owner/real driver control flow with internal
  adapters, not merely the oracle. Include partial startup and all-phase signal
  cleanup, blocked/failed IO, changed daemon, wrong serving process, ambiguous or
  incomplete rollout evidence, failed post-reply reads, actual opaque pane ids,
  missing opt-in side-effect guard, version drift, auth leakage, altered hook,
  symlink/hardlink controls/output, updater left behind, and failed joins/reaps.
- Document exact opt-in invocation, provisioning/network needs, expected baseline
  failure, evidence privacy and sub-cadence limits. No credentials required, but
  do not call the entire system air-gapped or claim full S145 acceptance.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE evals/run S146`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE bash -c 'evals/scenarios/S146-codex-local-provider-pane-attribution/check; test "$?" -eq 77'`
- [ ] `scripts/check`
- [ ] Lead-only gate before merge: unchanged driver with explicit pinned baseline
  binary/provisioning opt-ins reports genuine wrong-pane product FAIL via evals/run
  S146 (not skip/check error), with two completed independent real threads, same
  serving daemon and verified cleanup. Offline acceptance or a skip is not this gate.
- [ ] Independent implementation review has no unresolved high findings; lead runs
  adversarial/related hidden checks. Any defect returns to the worker for another round.

## Constraints
The product bug is expected on the pinned baseline; do not fix Codex/Herdr or weaken
S146 to obtain green. Runner's non-opted-in scenario command deliberately tests skip
behavior only. Record the real failing baseline separately; do not report it as pass.
No baseline-binary updates, account login, arbitrary profile input, secrets in output,
new dependencies or worker config changes. Keep driver/oracle separate; if the resource
layer exceeds one reviewable diff, stop for a task split, not a generic framework.

## If unsure
STOP with a question in `.task/REPORT.md` rather than guessing live control semantics.
Workers may run credential-free opt-in only when pinned binaries/provisioning are
explicitly made available by the lead; no sandbox escape or credentials workaround.

## When done
Do not commit; runner commits. Write `.task/REPORT.md` (40 lines max) with tests,
limits, live-vs-offline evidence, actual author family and any doc conflicts.
