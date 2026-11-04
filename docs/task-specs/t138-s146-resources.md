# Task t138: Trusted S146 acquisition and bounded one-shot RPC resources

size: medium

## Goal
Extract the resource primitives already reviewed in t131 into a reviewable module:
trusted file/proc collection, exact owned-process handles, and one-shot Herdr RPC.
The final driver keeps phase decisions, signals and cleanup policy.

## Context (read these, nothing more)
- `AGENTS.md`, `docs/decisions/036-codex-attribution-regression-boundary.md`
- `docs/research/codex-s146-check-contract.md`
- `docs/research/codex-s146-live-interfaces.md`
- `docs/research/codex-s146-split-review.md` (lead disposition included)
- `docs/task-specs/t131-s146-driver.md`
- `evals/lib/codex_identity_evidence.py` (merged t133 parser API)
- `evals/lib/fixtures/codex-identity/`

## Readiness / scope
Explicit `--tier deep`, after t133 merges. This is the extraction requested by
independent split-spec review, not new product behavior. Offline synthetic tests
only; no real Codex/Herdr binary, auth, provisioning or worker live opt-in.
- In: `evals/lib/codex_identity_resources.py`,
  `evals/tests/test_codex_identity_resources.py`
- Out: all peer modules/tests, fixtures, scenario/check wrappers, product/Rust/kit,
  worker config/dependencies. No owner-phase loop, hook approval or verdict oracle.

## API
Standard library only, `ResourceError.code` stable/redacted. No import side effects.
Use merged t133 PID/stat parsers/attest_process, never duplicate their parsing.
All timing accepts injected monotonic clock in tests; real default time.monotonic.

- `read_owned(root: Path, relative: str, limit: int) -> bytes`: no-follow directory
  fd traversal from a non-symlink canonical root, regular file, current uid, nlink=1,
  finite bounded read, pre/post dev/ino/size/mtime stability. Reject absolute relative
  paths, '..', symlinks, directories, hardlinks, drift and overflow. Never mutate.
- `Subreaper()`: Linux PR_GET_CHILD_SUBREAPER / PR_SET_CHILD_SUBREAPER via stdlib
  ctypes prctl, fail closed if unsupported. Call only AFTER opt-in/platform gate,
  BEFORE any run child. Preserve original flag; require no preexisting children
  or record/exclude them so unrelated harness children cannot become owned.
  `spawn(argv:list[str], *, env:dict[str,str], cwd:str) -> subprocess.Popen` records
  child handle immediately; stdout/stderr PIPE nonblocking. No inherited env or shell.
  `descendants() -> list[int]`: recursively enumerate /proc/<pid>/task/*/children
  from this process, excluding preexisting children; detached descendants reparent
  to this driver thanks to subreaper. Require same uid and stable parent/start
  association before inspecting env/argv. No arbitrary personal environ scan.
  `reap() -> None`: nonblocking waitpid only known/adopted descendants; never another
  caller's child. `close()`: restore prior subreaper flag only after no owned children
  remain, otherwise ResourceError. Do NOT auto-kill from close or implement driver
  cleanup order here. Test in isolated subprocess to avoid modifying unittest host.
- `ProcessHandle(record: dict, *, executable:str, codex_home:str, kind:str,
  expected_digest:str)`: input record is normalized parse_pid_record result. Open
  pidfd, collect stable boot/start/PID/exe/env/argv and validate via t133. Compare
  /proc/PID/exe SHA-256 against expected pinned digest ONCE per instance on a held
  descriptor, with stable file fstat before/after; cache held exe identity. Do not
  hash the 319MB executable each sample. No permission solely from PID JSON digest.
  `fingerprint` is the private (boot,pid,start) tuple.
  `sample(rollout: Path | None = None) -> dict`: verify pidfd liveness, boot/start,
  exact pinned exe device/inode/size/mtime, env/argv and PID record identity from
  trusted private control file; no rehash. With rollout, open stable owned session
  descriptor via no-follow walk, match exact fd target and fstat inode/device under
  /proc/PID/fd, recheck both identities; validate t133 snapshot and return it.
  Snapshot keys match attest_process, fd_targets contain only validated targets.
  No rollout means continuity/liveness only, not serving proof.
  `signal(sig:int)`: recheck identity/owner on held pidfd immediately before
  signal.pidfd_send_signal; no os.kill PID race, name/global kills or arbitrary pids.
  `close()` closes only descriptors, idempotent; it does not terminate a process.
- `RpcCall(socket_path: str, method: str, params: dict, *, clock=...)`: explicit
  absolute run-owned socket, new nonblocking AF_UNIX connection per call, random
  correlation ID; no CLI subprocess, reused RPC socket or global endpoint. Keep
  socket safely within 108-byte limit. Lifetime deadline constructor time+0.2s;
  `poll() -> dict | None` services bounded nonblocking connect/send/read and returns
  result object only after full newline response. Limit response to 1MiB. Reject
  wrong/missing id, error envelope, malformed/trailing JSON, EOF/timeout, duplicate
  response keys and type/binding mismatches. Methods limited to workspace.create,
  pane.get/read/send_input/send_keys, workspace.close. Validate expected result.type;
  pane.get/read must return exactly requested pane_id. `close()` idempotent.
  No `ok` key expected. Owner services pending RpcCalls together so cycles <=0.6s;
  it retains failed samples. RPC client cannot fabricate identities or samples.

ProcessHandle acquisition may hash once before the driver's A reply boundary; the
owner continues pane sampling around expensive setup and detects any >1s coverage
gap as check_error. Required-window continuity samples never rehash. All handles
are tracked even if acquisition fails halfway; close partial FDs on error.
Owned package daemon/updater may be terminated via ProcessHandle; transient owned
network children may be waited/reaped using subreaper ancestry and exact child
handles, never killed by env string alone. No process control of external profiles.

## Tests / acceptance
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_resources.py' -v`
- [ ] `python3 -m unittest discover -s evals/tests -p 'test_codex_identity_*.py' -v`
- [ ] `scripts/check`
- [ ] Synthetic subprocess/socket/fixture tests cover safe reads and link/race/size
  failures, partial acquisitions, daemon start/PID/boot/exe/env/argv mismatches,
  rollout path-vs-inode mismatch, held pidfd signaling and changed/dead instance,
  digest once (not on repeated samples), adopted detached child enumeration/reap,
  no env access before ancestry/uid ownership, preexisting-child exclusion,
  one-response-per-connection framing, partial IO, timeout/EOF/error/wrong binding,
  bounded buffers and no leaked descriptors/children.
- [ ] Independent implementation review has no unresolved high findings.

Inject clock for deadline tests, no multi-second sleeps or real binaries. Synthetic
subprocess integration tests have short hard deadlines; full module suite target
<5s normal, no timing correctness assertion against loaded-machine wall time.
No live S146 verdict claimed. Stop for contract ambiguity. Runner commits; report
family/tests/limits/doc conflicts, no raw private evidence.
