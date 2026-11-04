# Task t130: Fail-closed attribution trace oracle for S146

checks_by:
size: medium

## Goal
Implement a small pure oracle for the agreed credential-free shared-daemon scenario,
so live resource handling cannot hide gaps, wrong-pane reports or incomplete turns.

## Context (read these, nothing more)
- `AGENTS.md`
- `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-spec-review.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-attribution-redesign-review.md`
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`
- `evals/README.md`

## Scope
- In: `evals/lib/codex_identity_oracle.py`, `evals/tests/test_codex_identity_oracle*.py`,
  `evals/tests/README.md`, `scripts/check`
- Out: live driver, scenario files/checks, Rust/product code, kit, workers config,
  private profiles/evidence, S145, t131 files. Do not read rejected t129 artifacts.

## Contract
- Implement the eval-private trace v1 and evaluate(trace) contract exactly as defined
  in the context contract. Standard library only, no process/env/file/network access
  from evaluate. No import-time side effects. No mutable global oracle state.
- Results are deterministic and redacted. Validate malformed/nonfinite/out-of-order
  data, exact qualified identity and independently evidenced distinct completed turns.
- Enforce both real opaque pane ids, all observation phases/edges, post-reply dwell,
  grace, max gaps, daemon continuity and same serving-instance proof. No fixture-name
  fallback, truthy-string booleans, empty-slice success, skipped read errors or
  swallowed malformed rows. Unknown identity differs from failed observation.
- Keep concrete attested product failure despite later observation/cleanup problems;
  untrusted identity/evidence cannot manufacture product failure. Pure tests may use
  synthetic trace identities, never present them as a live scenario verdict.
- Tests must independently build a valid passing trace with non-fixture pane ids,
  then mutate one aspect per regression. Include A/B post-completion missing/unknown,
  startup/transition gaps and endpoint gaps, failed reads, wrong source/agent/kind,
  A=B transient during B startup, distinctness, malformed times/fields, changed or
  unused daemon, interruption and cleanup failure, contradictory evidence and
  multiple calls not leaking state. Include property-like seeded mutations around
  grace/dwell/gap boundaries; print seed on failure.
- Add standard-library eval check tests to the standing `scripts/check` gate so
  they continue running after merge, while retaining Rust checks unchanged. Document
  `evals/tests/` as tests of eval infrastructure (not user-level product verdicts)
  in `evals/tests/README.md`. Root `evals/README.md` mirrors an installed kit file:
  leave it unchanged rather than introducing kit changes just for project-only tests.
  The lead will document the project-only tests in AGENTS.md after merge.
- Apply the contract's explicit verdict truth table and all specified time equalities;
  a_exit is observed process exit, not sent input. False setup/evidence flags always
  prevent product verdicts. Error-vs-counterexample ordering does not affect precedence.
- This task contributes to S146; it must not claim a live eval pass or add a scenario
  check before the driver exists. If the written contract is ambiguous, ask before coding.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_oracle.py' -v`
- [ ] `scripts/check`
- [ ] Review verifies the agreed S146 attribution/coverage rules and malformed input
  handling against adversarial tests; no authenticated/live access was used.

## Constraints
No dependencies or product APIs. Keep the implementation independently reviewable;
stop instead of growing a generic event engine. Report actual selected worker family.
The lead runs `--tier standard` explicitly, stops on any GPT selection or step-up,
then verifies the non-GPT Worker trailer before scheduling deep t131. No model pin
or worker configuration change is authorized by this spec. Later live-wave tasks
remain blocked on split specs and redacted-interface/provisioning preflight.

## If unsure
STOP. Write the blocking question in `.task/REPORT.md`; do not improvise the contract.

## When done
Do not commit; the runner commits. Write `.task/REPORT.md` (40 lines max), including
coverage limits and any doc conflicts. Do not claim offline oracle tests pass S146.
