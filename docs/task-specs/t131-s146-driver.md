# Task t131: S146 final sampling loop and owned cleanup (blocked draft)

checks_by: t130
size: medium

## Goal
Assemble already-tested profile/stub and evidence components into one bounded
real-interface owner loop for S146, rather than rebuilding t129's oversized driver.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-spec-review.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/herdr-observer-live-identity.md`
- `prototypes/herdr-observer/README.md`, `prototypes/herdr-observer/observer`
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`
- Merged t130 oracle, t132 preparation/stub and t133 evidence interfaces/fixtures

## Scope
- In: `evals/lib/codex_identity_live.py`, `evals/tests/test_codex_identity_live.py`,
  `evals/scenarios/S146-codex-local-provider-pane-attribution/check`,
  `evals/lib/codex-identity-live.md`
- Out: t130 oracle/tests, t132 preparation/stub, t133 evidence readers/fixtures,
  scenario.md, S145, product/Rust/kit code, worker configuration, authenticated data.

## Contract
This replaces the oversized pre-review t131 spec. DO NOT START until merged
components and their exact APIs are substituted below and independently reviewed:
- t132 owns localhost SSE stub, fresh private environment/profile, explicit pinned
  provisioning and genuine hook validation. No such logic is reimplemented here.
- t133 owns pure rollout/PID/proc evidence readers, real-format redacted fixtures,
  thread/completion binding and serving-instance attestation. No heuristics here.
- t131 owns only phase transitions, bounded IO sampling, actual pane creation/input,
  tracing into the t130 oracle, signal/deadline handling and ordered resource cleanup.
  Follow the detailed contract; no supplied-trace/fake-binary live input mode.
- Lead must capture concrete Herdr API and TUI-approval fixtures before this task
  starts, settle the exact persistent-socket wire adapter and worker opt-in permission,
  and verify merged t130 non-GPT Worker trailer plus family capacity.
- Profile/stub and evidence component interface placeholders are blocking questions,
  not permission for a worker to invent more abstraction or expand this task's scope.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE bash -o pipefail -c 'evals/run S146 | tee /dev/stderr | grep -Eq "1 skipped"'`
- [ ] `env -u DELEGATE_CODEX_LOCAL_PROBE bash -c 'evals/scenarios/S146-codex-local-provider-pane-attribution/check; test "$?" -eq 77'`
- [ ] `scripts/check`
- [ ] Offline owner-flow tests cover all-phase signals, IO gaps/errors, partial
  acquisition, serving-daemon changes, concrete wrong-pane evidence precedence,
  cleanup deadlines, exact-owned updater termination and no-side-effects skips.
- [ ] Lead-only live gate before merge: unchanged check reports genuine wrong-pane
  baseline FAIL via evals/run S146, with distinct independently completed threads,
  same serving daemon and verified cleanup. Skip/check error is not the live gate.
- [ ] Independent implementation review has no unresolved high findings.

## Constraints
Run explicitly at `--tier deep` only after dependencies merge. Do not rewrite the
oracle or component tests to pass. A pinned baseline product FAIL is expected, not
something to fix by changing Codex/Herdr or weakening S146. No dependencies, auth,
worker-config changes, sandbox escape or product/protocol changes.

## If unsure
STOP. Record missing component contracts or interface facts in `.task/REPORT.md`.

## When done
Do not commit; runner commits. Report actual family, tests, limits and any doc conflicts.
