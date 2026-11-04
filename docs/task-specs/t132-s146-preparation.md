# Task t132: Fresh S146 profile, local Responses stub and artifact validation

size: medium

## Goal
Prepare credential-free inputs for the final single-owner S146 loop without owning
its Herdr/TUI/daemon lifecycle or reimplementing the oracle/evidence readers.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-s146-provision-preflight.md`
- `docs/research/codex-credential-free-attribution.md`
- `docs/research/codex-s146-live-interfaces.md`
- `evals/lib/fixtures/codex-identity/README.md` and its fixtures
- `evals/scenarios/S146-codex-local-provider-pane-attribution/scenario.md`

## Readiness
Explicit `--tier deep`, after independent review of t132/t133/t131 specs.
Offline tests only; no credential access, binary execution or provisioning in this
worker. Lead performs the final unchanged live driver gate later.

## Scope
- In: `evals/lib/codex_identity_prepare.py`,
  `evals/tests/test_codex_identity_prepare.py`
- Out: lead-owned fixtures, t130 oracle/tests, t133 readers, t131 owner loop/wrapper,
  scenario.md, all product/Rust/kit code, dependencies/worker configuration.

## API
Standard library only. `PreparationError` has a stable redacted `code`; messages
contain no caller paths, IDs, environment values or raw exception text. Do not
mutate inputs. No module-import side effects. Do not spawn processes or threads.

- `create_profile(run_root: Path, port: int, herdr_bin: Path) -> dict`: existing,
  exclusive 0700 root owned by current uid, empty and not a symlink. Reject bool or
  non-1..65535 port. herdr_bin is an explicit absolute canonical regular executable.
  Create fresh 0700 directories home, home/.codex, config, data, cache, state, tmp,
  checkout and config/herdr; exclusive 0600 files only. Return
  `{root, codex_home, repo, socket, env}`, paths as absolute strings, env dict[str,str].
  env consists only of HOME/CODEX_HOME/XDG_CONFIG_HOME/XDG_DATA_HOME/XDG_CACHE_HOME/
  XDG_STATE_HOME/TMPDIR (below root), PATH (pinned Herdr parent + /usr/bin:/bin),
  LANG=C.UTF-8, TERM=xterm-256color, SHELL=/bin/bash, HERDR_CONFIG_PATH and
  HERDR_SOCKET_PATH. Never inherit os.environ, auth/proxy/provider/Herdr context.
  Empty private Herdr config; socket path config/herdr/s. Codex TOML matches captured
  local configuration: gpt-5.4/local, update checks off, ephemeral credential store,
  hooks=true, shell_snapshot=false, exact absolute repo trust key, local responses
  provider at http://127.0.0.1:port/v1, requires_openai_auth=false and
  supports_websockets=false. No git subprocess; t131 initializes repo.
- `resolve_managed_package(codex_home: Path, start_response: bytes,
  expected_digest: str) -> dict`: BEFORE executing any path from stdout, parse
  actual start stdout;
  require status=started/backend=pid, positive pid, cliVersion/appServerVersion/
  managedCodexVersion all 0.166.0. Resolve only installed current/release symlinks
  staying under this fresh CODEX_HOME/packages; regular executable, uid match,
  no hardlinks, digest equals expected_digest. Require socketPath lies within the
  private app-server-control directory. Return `{executable, sha256}`.
  t131 supplies the pinned CLI digest, never a PID-record digest or version-only
  substitute. No execution here; malformed paths, drift or unexpected layout error.
  Exact start/layout fixture: provisioning.json. Only AFTER this validation may
  t131 execute the returned path with --version.
- `validate_managed_version(version: str) -> None`: require exactly the stripped
  version 'codex-cli 0.166.0'; otherwise PreparationError. Pure string check.
- `validate_hook(codex_home: Path) -> None`: inspect exclusive regular uid-owned,
  nlink=1 hooks.json and herdr-agent-state.sh. Exactly the captured declaration:
  sole hooks.SessionStart list of one entry containing sole hooks list of one
  command with type command, timeout integer 10, command exactly
  `bash '<absolute run script>' session`. No extra keys/events/handlers.
  Script SHA-256 must be
  c34709efe76f4d83614b47f5b3f5880b621a0f53bf96022393db0d4686b29340.
  Never write/patch hook/config during validation or create a trust bypass.
  t131 owns real integration installation and TUI approval.
- `assert_no_auth(codex_home: Path) -> None`: reject any auth.json entry, even a
  symlink, directory, empty file or broken link. Never read its contents.

Filesystem safety for all above: walk ancestors with directory descriptors and
O_DIRECTORY/O_NOFOLLOW (or equivalent no-symlink traversal), open files O_NOFOLLOW,
fstat regular/type/uid/nlink, compare pre/post identity/size/mtime for reads/hashing.
No chmod/truncation of existing entries. Managed package symlink resolution is the
only deliberate exception: verify each target under the private package root,
then open canonical components without symlink traversal. Race/drift errors are
PreparationError, not a claimed atomic defense against a hostile same-uid process.
Do not inspect personal files or scan /proc. Existing-root failure may leave only
new files from this invocation; do not recursively remove arbitrary caller paths.

## Local Responses stub

`LocalStub(*, clock=time.monotonic)` binds nonblocking IPv4 127.0.0.1 port 0. Properties `port: int`,
`invalid: bool`, `request_count: int`; methods `poll() -> None`, `close() -> None`.
No thread/process: t131 calls poll every loop tick, including waiting for setup
commands. poll never waits for IO and does bounded work (one <=64KiB read/write per
ready connection, <=8 connections). close idempotently closes listener/all clients.

- HTTP/1.0 or 1.1, CRLF framing, close each response connection. POST /v1/responses
  with one nonnegative Content-Length <=1MiB and valid UTF-8 JSON object. No chunked
  transfer, upgrades, conflicting/duplicate Content-Length or pipelining. Header
  bytes <=16KiB. Unknown/malformed requests set invalid=true, close/reject; never
  forward requests. Header/body incomplete >2s is invalid. At most 64 requests,
  >8 simultaneous clients or body/header overflow marks invalid. No raw body/header
  logging or retained request bodies. Authorization in any casing (even empty or
  duplicated) always invalidates setup. Count only fully accepted POSTs.
- Each accepted POST returns Content-Type text/event-stream with proper
  Content-Length, exactly the captured 3-event pattern in local-responses.json,
  assistant/output_text LOCAL_STUB_OK, unique response/message IDs per request,
  no thread IDs. Repeated/auxiliary requests receive the same text; a request count
  is NOT evidence of a completed user turn.
- GET /v1/models may return JSON {"data":[]}; no auth, body or transfer encoding.
  It is a supported auxiliary path, not counted as a completed model request.
- close does not silently clear invalid. Socket/parse errors set invalid, never
  leak request contents in messages. Tests use only loopback fixtures, not real
  Codex/Herdr binaries.

## Ownership and timing

t131 is sole owner of all commands/children and lifecycle transitions. Initial
provisioning is pinned CLI `app-server daemon start`, only after explicit provision
opt-in. Validate selected version/digest, stop preparation daemon AND exact updater,
then A's real TUI creates the sequence's daemon. No restart during A->B.
<=60s provisioning and <=10s preparation teardown are inside <=180s total active
time; <=60s final cleanup reserve. t132 never starts its own independent deadline
that can extend the owner's absolute deadline.

## Acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_prepare.py' -v`
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `scripts/check`
- [ ] Tests cover every API's success/error, env leakage, exclusive/link/ancestor
  hazards, malformed declarations/digests/versions, no-auth entry detection without
  content reads, stub Authorization rejection, bounded partial/malformed HTTP,
  repeated/auxiliary requests, connection limits and idempotent descriptor teardown.
- [ ] Independent review has no unresolved high findings.

Deadline/partial-request tests use injected monotonic time, not 2-second sleeps.
Module suite target <5s normal; no loaded-machine wall-time correctness assertions.

Offline infrastructure gates for agreed S146 only, NOT a live scenario pass.
No worker live opt-in, dependencies, product/protocol or scenario intent changes.
Stop on contract ambiguity. Do not commit; runner commits. Report family, tests,
limits and doc conflicts in `.task/REPORT.md`.

## Round 2
Apply the CURRENT .task/REVIEW.patch first; it contains the independent review's
regression tests. Fix both findings in .task/REVIEW.md: service available ready
request bytes before declaring a request still incomplete after 2s (preserve bounded
work/overflow constraints); high descriptors must not leak raw ValueError from
select. Prefer scalable selectors/poll or fail redacted/invalid, never raw traceback.
Keep injected-clock tests fast and all original security guards. Rerun Acceptance.

## Round 3
Apply NEW current .task/REVIEW.patch first (round 2 tests), not the older review
patch. Fix both findings: a complete request >64KiB must drain over bounded ticks
even if owner polling resumed late. Keep one bounded read/tick; don't expire a tick
that makes read progress or still has readable queued bytes. Treat the 2s request
timeout as no-progress/inactivity; owner's absolute deadline still bounds the run.
Never permit unbounded reads, body/buffer growth or background blocking IO.
For hook validation reject group/world WRITABLE mode bits on hooks.json and genuine
script, while allowing installed read/execute bits (0644/0755) beneath the private
0700 run root. Do NOT chmod/rewrite the genuine live script. Add regression tests
for 0666 hooks and 0777 script; preserve digest/type/uid/nlink/race checks.
Rerun all Acceptance and report the clarified inactivity rule.

## Round 4
Apply NEW .task/REVIEW.patch first (round 3 trust-key regression). Fix both low
findings as spec compliance: TOML project key containing DEL must either correctly
escape it (while preserving valid Unicode paths) or reject with PreparationError
before publishing a malformed profile. Do not introduce surrogate-pair TOML errors.
Replace path chmod(follow_symlinks=False) on NEW directories with verified no-follow
directory descriptor + fchmod, keeping exclusive creation/no chmod of preexisting
entries. Redact NotImplementedError paths as well; simulate unsupported operations
in tests. Preserve mode/uid/link/ancestor guards. Rerun all Acceptance.

## Round 5
Apply NEW current .task/REVIEW.patch first (round 4 tests), then:
1. Remove ALL tracked bytecode/__pycache__ artifacts from this branch. Final diff
   against main must contain ONLY the two scoped .py files, never generated files.
   Use PYTHONDONTWRITEBYTECODE=1 while testing and remove generated caches before
   finishing; do not add unrelated .gitignore or touch oracle source/tests.
2. Reject ':' in the canonical Herdr parent before profile creation, to preserve
   the exact pinned absolute PATH and avoid relative entries.
3. Clarify source-binary vs private-artifact policy: herdr_bin is a canonical regular
   executable, permitted current-uid OR root ownership and multiple hardlinks. It
   is an explicit externally pinned source binary (t131 separately validates digest),
   NOT a private run-created artifact. Keep no-follow canonical traversal and stable
   reads; do not relax uid/nlink=1 rules for run-created managed package/hooks/control
   artifacts. Adjust the earlier source-binary hardlink test to this clarified rule;
   retain ancestor/symlink/FIFO/other-user rejection. Add root-owned source coverage.
Rerun all Acceptance and verify final git diff --stat has no binary/cache files.
