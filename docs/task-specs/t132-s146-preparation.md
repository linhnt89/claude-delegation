# Task t132: Fresh S146 profile, local Responses stub and artifact validation (blocked draft)

size: medium

## Goal
Prepare credential-free inputs for the final single-owner S146 loop without owning
its Herdr/TUI/daemon lifecycle or reimplementing the oracle/evidence readers.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-s146-provision-preflight.md`
- `docs/task-specs/t133-s146-evidence.md` (proposed peer API, not a merged dependency)
- `evals/lib/fixtures/codex-identity/README.md`
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`

## Readiness
DO NOT RUN. Missing concrete hook/SSE fixtures, stub IO contract and independent
split-spec review are blocking, not implementation choices. Explicit `--tier deep`.
Offline tests only; worker live opt-in has NOT been granted or sandbox-validated.

## Scope
- In: `evals/lib/codex_identity_prepare.py`,
  `evals/tests/test_codex_identity_prepare.py`
- Out: lead-owned fixtures, t130 oracle/tests, t133 readers, t131 owner loop/wrapper,
  scenario.md, all product/Rust/kit code, dependencies/worker configuration and
  authenticated data.

## Proposed component boundary (review before activating)
- Gate no opt-in/non-Linux in t131 before calling this module. t132 must never
  discover or read personal profiles. It receives only explicit canonical binary
  paths and a newly allocated private run root. Never accept a caller CODEX_HOME.
- `create_profile(run_root: Path, port: int) -> dict`: fresh exclusive files only;
  run_root already exists with 0700, owned by current uid, not a symlink. Fail on
  any existing child or ancestor/link hazard rather than truncating or changing it.
  Return paths and a minimal allowlisted child environment: HOME/CODEX_HOME/XDG,
  private Herdr config/socket, PATH limited to pinned Herdr's directory and system
  executables, LANG/TERM/SHELL; no inherited credentials/proxy/provider/Herdr context.
  All writable state under root; use disposable repo and bash without user rc files.
  Exact config follows the approved local-provider research: gpt-5.4, provider local,
  responses wire API, loopback allocated endpoint, requires_openai_auth=false,
  supports_websockets=false, hooks enabled, shell_snapshot=false, update checks off,
  ephemeral credential store, exact absolute disposable repo trust key.
- `validate_managed_package(codex_home: Path, start_response: bytes,
  expected_digest: str) -> dict`: validate actual start object, resolve managed path
  under fresh run-owned packages, regular executable and 0.166.0 version/digest.
  No preexisting cache/profile reuse. File acquisition/link/race policy must be
  specified before activation; do not treat a check-then-open path as race-proof.
- `validate_hook(codex_home: Path) -> None`: structurally validate hooks.json has
  exactly the genuine SessionStart declaration, run-owned script path, command type
  and timeout 10; verify script SHA-256
  c34709efe76f4d83614b47f5b3f5880b621a0f53bf96022393db0d4686b29340.
  Never rewrite hook/script/payload/destination or create a hook-trust bypass.
  Herdr integration installation and actual TUI hook approval belong to t131.
- Local Responses stub: bind IPv4 127.0.0.1 on port 0; ordinary response.created,
  response.output_item.done assistant/output_text LOCAL_STUB_OK, response.completed
  events, unique response/message ids but NO fabricated thread ids. Reject any
  Authorization header, even empty; record setup invalid without logging contents.
  No credentials/auth.json, raw request logging, provider passthrough or remote model
  traffic. Handle auxiliary requests without mistaking them for user completion.
  Exact GET/POST paths, body limits, HTTP/SSE fixtures, nonblocking service/close API
  and bounded partial-read behavior still need to be pinned by the lead. STOP until
  those contracts are present; no worker-designed background service framework.
- t131 owns every spawned command/process and cleanup, including preparation start,
  daemon stop and exact updater termination, socket/stub teardown and final scan.
  Initial provisioning is pinned CLI `app-server daemon start` (not update), only
  after explicit provisioning opt-in; validate selected managed version/digest.
  No preparation daemon/updater may remain when A's real TUI starts the shared
  daemon for the A/B sequence. No restart during A->B.
- Proposed timing: <=60s preparation start within total <=180s active deadline;
  <=10s preparation teardown; total final cleanup reserve <=60s. No per-component
  clock may extend the shared absolute deadline. This budget needs live confirmation.

## Acceptance (offline only; complete before activation)
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_prepare.py' -v`
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `scripts/check`
- [ ] Offline tests cover minimal-environment leakage, exclusive/symlink/hardlink
  hazards, malformed hook declarations/digest drift, version/path/digest mismatch,
  stub Authorization rejection, bounded malformed/partial HTTP, multiple requests,
  stub teardown and auth.json invalidation; no network other than private loopback.
- [ ] Independent implementation review has no unresolved high findings.

Offline tests are not a real S146 verdict. No live smoke test or provisioning is
permitted in the worker until explicit sandbox validation and opt-in approval.
Do not commit; runner commits. Report family, tests, limits and doc conflicts.
