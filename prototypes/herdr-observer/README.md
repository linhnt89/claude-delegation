# Private Herdr observer — headless slice, read contract v1

This is **not a native UI**, production ownership integration, permanent task archive,
or a pass for S137–S144/P030–P033. It is a closed-world Python-standard-library
experiment for the later t124 consumer and independent check writer. No service,
startup hook, installed configuration, production schema or eval has changed.

## Automatic gate (no Herdr daemon, GUI, network or credentials)

From this source checkout:

```sh
prototypes/herdr-observer/check
scripts/check
```

`check` offline-builds **this checkout's** release delegate with the real HOME before
isolation (`mise exec -- cargo build -q --release --offline`, 180-second limit).
For repeated runs only, `DELEGATE="$PWD/delegate/target/release/delegate"
prototypes/herdr-observer/check` reuses that source-built executable. No installed
binary fallback; another path is rejected. Required tools: Python 3, git, mise,
cached Rust dependencies/toolchain, bwrap with usable namespaces. Missing tools or
build/explicit setup-tool errors exit **3**. Product failures exit **1**, including
assertions, ordinary exceptions, bad JSON and missing expected rows/evidence; they
dominate mixed setup/product results. All required tests passing exits **0**.
There are no skips. Gate setup errors do not constitute product evidence.
Explicit private socket bind/listen setup and git fixture init/add/commit failures
are narrowly classified as setup errors; product init/register assertions and JSON
validation are not wrapped. The gate preflights basic bwrap namespace availability
with `bwrap --ro-bind / / -- /usr/bin/true`. Running inside another sandbox also
requires that host's nested namespace permissions; this basic probe does not prove
every production worker mount/namespace operation. Subsequent delegate run failures
remain product failures, not automatically relabelled host/setup errors.

All smoke operations invoke the actual `observer` executable. Tests use real
source-built delegate, deterministic stand-in Pi research workers under unchanged
production bwrap, disposable repos/HOME/XDG, a logging delegate launcher and a
private fixture-backed UNIX socket. `standin.py` is not Pi integration evidence.
The launcher injects deliberate status failures/oversize/malformed responses;
actual corrupt STATE and attempt-local events exercise delegate's real read path.
The no-control comparison runs **after stand-in exit**, comparing STATE.json,
status.json, events.jsonl and worktree HEAD. Dispatch auditing proves zero
non-status calls from snapshot, not whole-data purity or invented input records.
Worker stdin is null/EOF already in delegate. Smoke coverage includes concurrent
linked-worktree creation, crash publication, stale fallback, retirement/reuse,
identity scrub, inert data and whole-step replacement. Hostile opaque task ids
retain distinct machine keys while bounded control/bidi-safe display labels may
collide; live/stale/unavailable display paths are exercised. Init/register failures
and result-validation errors during test setUp remain product failures, not setup
errors. These are implementation regressions, not independent properties.

Test state lives in ignored `.scratch/` because production bwrap masks `/tmp`.
Fixture sockets also live in that private scratch root. Short explicit
`/proc/<test-pid>/fd/<directory-fd>/fixture.sock` aliases avoid the UNIX address-length
limit; socket addresses are intentionally not symlink-resolved. The worker sensitivity
control uses its own `/proc/self/cwd/` relative alias to the same socket
(cross-namespace parent-fd access is not assumed).
Temporary directories are removed after tests; build products stay in delegate's
ignored target directory. Do not commit scratch, captures, credentials or logs.

## Explicit setup and CLI

Only register repos created for this experiment. Registration intentionally writes
checkout identity/registry metadata. **Never supply a personal socket, working copy,
HOME or shared DELEGATE_HOME.** There is no discovery or ambient Herdr endpoint.
The caller must choose a private absolute state directory that does not yet exist,
an absolute source-built delegate and an explicit private UNIX API endpoint.
`init` does not connect; `create` and `capture` do. All delegate operations share
`STATE/home` and its XDG roots. Keep STATE outside `/tmp` for bwrap workers.

```sh
OBSERVER="$PWD/prototypes/herdr-observer/observer"
DELEGATE_BIN="$PWD/delegate/target/release/delegate"
# Set these explicitly to disposable paths; PRIVATE_SOCKET must be private.
: "${STATE:?absolute new private directory}" "${CHECKOUT:?absolute disposable git repo}"
: "${PRIVATE_SOCKET:?absolute private API socket}" "${PANE:?explicit private pane id}"
"$OBSERVER" --help
"$OBSERVER" --state "$STATE" init --delegate "$DELEGATE_BIN" --socket "$PRIVATE_SOCKET"
"$OBSERVER" --state "$STATE" register --checkout "$CHECKOUT"
"$OBSERVER" --state "$STATE" capture --pane "$PANE"
"$OBSERVER" --state "$STATE" create --checkout "$CHECKOUT" --id t-demo --pane "$PANE"
"$OBSERVER" --state "$STATE" run --checkout "$CHECKOUT" --id t-demo
"$OBSERVER" --state "$STATE" snapshot
# Retry is the same scrubbed, explicit run operation, never direct delegate run.
"$OBSERVER" --state "$STATE" run --checkout "$CHECKOUT" --id t-demo
"$OBSERVER" --state "$STATE" discard --checkout "$CHECKOUT" --id t-demo
```

`run` is deliberately **stand-in only**: it dispatches synchronous `delegate run ID
--research --worker pi:observer-standin@high` (60 seconds), not arbitrary commands,
models, credentials or interactive workers. `create` uses delegate's stock template;
the adapter never edits delegate specs or state. The stand-in writes its own
`.task/REPORT.md` and emits synthetic read/run events. To test different titles,
phases or event data, smoke fixture preparation acts separately from observation.
Direct `delegate run`, external lifecycle calls and manual id reuse are unsupported
and unprotected in the spike (tests deliberately simulate them to expose gaps).

Every command emits one JSON line on success. Exit **0** means the operation
completed; **3** is configuration/tool/read failure; argparse usage exits **2**.
`snapshot` returns 0 even when feeds fail: inspect source/task health. It returns 3
if private state cannot be locked/read/written or its global limit is exceeded;
there is then no authoritative JSON result. Errors never dump source stderr/logs.
Test-only `create ... --test-crash-after-new` exits **86** immediately after delegate
new succeeds, before publication; it is default-off and must never be used normally.

Success result shapes (exact keys; objects are unordered):

| Operation | JSON result |
| --- | --- |
| init | `{schema_version:1,state:absolute_path}` |
| register | `{checkout_namespace:uuid}` |
| capture | `{schema_version:1,origin:Origin_or_null}` |
| create | `{checkout_namespace,task_id,instance_token:uuid,origin:Origin_or_null}` |
| run | `{checkout_namespace,task_id,run_completed:true}` |
| discard | `{checkout_namespace,task_id,retired:true}` |
| snapshot | documented below |

`--state` is required before the subcommand. `register`, `create`, `run`, `discard`
require `--checkout` (absolute registered path/linked alias); controls additionally
require `--id`. `create` and `capture` require `--pane`. No options select ambient
state. `init` installs only private stand-in config/bin under STATE. If interrupted
initialization leaves a partial directory, inspect/remove that disposable directory
and initialize a fresh one; it is not automatically adopted.

Registration first runs `delegate status --json` to initialize the checkout, then
reads git's **common** directory marker `delegate-checkout-id`, corroborating its
UUID against the initialized task-home basename reported by `delegate where`.
The task home must be under STATE/home/.local/share/delegate/checkouts. `where`
may initialize `.delegate-kit.toml`'s separate **project** id; registration is
therefore setup, not observation. Linked-worktree aliases deduplicate to the same
checkout UUID; distinct checkouts with equal task ids remain separate. Registration
pins the first path as read location; create/run/discard instead execute from the
caller's explicitly registered linked alias (so run bases on that lead's branch).
Relocation/removal/replacement requires a new experiment, not background namespace
resolution. Re-registering a known alias with a changed/missing git-common-dir
UUID is rejected before delegate initialization; no second namespace is published.
Controls validate both the pinned read path and the caller's alias identity before
any delegate dispatch. Registration also validates the existing pinned path before
adding any alias to its namespace; a moved checkout cannot repair a missing read
path, nor can a surviving main alias bypass a removed first-registered linked path.
A newly proposed alias may receive initialization status before its namespace is
known, but no alias/namespace is published if that namespace's pinned path is invalid.
Each snapshot validates the pinned read path with bounded git/marker reads before
any delegate dispatch. Invalid paths
retain old detail only as stale/unavailable, never querying a replacement as the old
namespace. No scanning private task homes is needed.

## Attribution transaction and authority

All setup sidecar and snapshot/cache accesses use one bounded advisory flock.
Creation holds it across a successful **full** status absence preflight, selected
pane API capture, atomic pending write, delegate new, and atomic publication.
A duplicate or failed full read refuses before changing origin/token (also across
linked leads). A pending slot clears cached detail; failed creation or a crash
cannot restore an old origin. Publication after successful new supplies a fresh
UUID task-instance token. Pending reads have null token/origin, even if the task
exists. A later duplicate attempt cannot repair/recapture that unknown ownership.

Origin is exactly `{source:string,agent:string,kind:"id"|"path",value:string}` or
null. All four values form identity; equal values with different sources, agents
or reference kinds do not alias. Empty/missing/wrong-type/unsupported references
are null, never coerced to an id; paths must be absolute. Every source, agent,
kind and value must exclude Unicode category C characters (including controls and
bidi formatting), Zl line separators and Zp paragraph separators. Unsafe references
are rejected wholesale as null, never sanitized into another identity. The same
validation applies to captured and sidecar-loaded origins before emission.
Unknown is not a guessed
pane, checkout, model, display title or legacy session owner. Supported reference
kinds come from the pinned public enum; this does not assert that every possible
source/agent pair has a working live integration. Display context is not identity.

The sidecar is the **sole prototype ownership authority**. Delegate's legacy
`session`/`session_name` are neither fallback nor rewritten. Captured origin persists
across observer restarts, stand-in retries, pane replacement and lead resume. No
snapshot consults the current pane occupant. Before dispatching `discard`, setup
atomically marks the slot `retiring` (null origin/token) and clears cached detail.
Success removes the slot; failure/ambiguous results conservatively retain unknown
retiring attribution, never restore the old instance. The default-off test argument
`discard --test-crash-after-discard` exits 86 after successful removal but before
slot deletion; direct recreation still cannot inherit ownership. Retiring slots
share the 128-slot bound and are reclaimed by successful discard/full-feed absence.
A successful full read proving absence likewise
removes slot/cache; subsequent unwrapped reappearance has null ownership/instance.
Degraded reads never prove removal. Retired slots are deleted, not archived.

An entire unwrapped deletion/reuse between successful observations remains
undetectable; even online unwrapped lifecycle bypasses the creation transaction.
Status has **no stable instance or per-fallback-attempt marker**. Wrapper tokens
cover only this closed world; `attempt_id` is always null. Resetting step ids is
not proof of attempt identity. No full P030/P032 guarantees, history, command stdout,
retention after lead closure, or production missing-identity policy are claimed.
Decisions 009/031 remain unchanged; integrating 035 needs a separate decision.

## Read boundary for t124 and independent checks

`snapshot` invokes **only** `delegate status [ID] --json` at registered read paths,
never where/new/run/check/retry/review/merge/discard/input. Namespace resolution was
done at registration; bounded git/marker identity validation precedes reads (no
additional delegate verbs). The observer itself writes only its private config, sidecar,
lock and cache, never delegate task state. Nevertheless the existing delegate CLI
uses Ctx::discover on status startup and may maintain checkout registry, migration,
repair and sweep metadata. This is task-level no-control, **not filesystem purity**.

One bounded JSON document:

```json
{"schema_version":1,"generation":1,"sources":[{"checkout_namespace":"00000000-0000-4000-8000-000000000001","read_generation":1,"health":"live","mode":"all","tasks":[{"checkout_namespace":"00000000-0000-4000-8000-000000000001","task_id":"t-demo","display_task_id":"t-demo","task_id_limited":false,"instance_token":null,"origin":null,"attempt_id":null,"health":"live","title":"Synthetic title","phase":"not_started","outcome":null,"worker":null,"steps_available":false,"steps":[],"steps_truncated":false,"rejected_steps":0,"detail_limited":false}]}]}
```

All keys in that example are always present on task/source/snapshot records.
No raw log, stream, shell command execution or CLI control affordance is emitted.
Task/source arrays sort by task id/checkout UUID. Fields:

- `schema_version`: integer 1. Consumers must reject unsupported versions.
- `generation`: monotonically increasing snapshot sequence persisted under the lock,
  scoped to this STATE. Each invocation is serialized, including its source reads.
  Consumers must reject responses with an older generation; on a new experiment
  reset that fence. Failed publication does not return a usable generation.
- `read_generation`: per-source nonnegative successful-read counter. Increases once
  if a full listing or any fallback read succeeds, unchanged on complete failure.
  These counters are read sequencing, **not upstream revisions or attempt ids**.
- Source `mode`: `all` on a valid full listing, otherwise `registered_fallback`.
  A failed full read never becomes an authoritative empty list. Individual reads
  cover sidecar-registered ids only, not discovery of every external task.
- `health`: `live`, `stale`, `unavailable`. A task is live only after a fresh valid
  read; stale means last good cached detail retained; unavailable means no usable
  detail ever read. Source is live only with full listing and all task details live;
  otherwise stale if any cached detail exists, else unavailable. Healthy fallback
  tasks can be live while their source is stale. Null ownership is independent of
  health: an unavailable task can still have its captured origin.
- `checkout_namespace`: corroborated UUID string; `task_id`: exact opaque source
  string, bounded only by the subprocess/document byte limits, not a display label.
  `display_task_id`: at most 4,096 characters, with Unicode category C characters
  (including ESC/OSC/BEL and bidi controls), plus Unicode Zl/Zp line/paragraph
  separators, replaced by U+FFFD. Display fields are single-line.
  `task_id_limited`: boolean, true if that display was truncated or sanitized.
  These fields are always present, including stale/unavailable rows. Native
  consumers **must render display_task_id, never task_id**. Distinct opaque ids
  may share a display label; never use that label as lookup/selection identity.
  `instance_token`: wrapper UUID string or null. Task key is namespace + task id;
  selection identity additionally includes non-null instance token when available.
  Unknown instances must not be treated as a proven match across disappearance.
- `origin`: captured four-field object above or null; `attempt_id`: always null.
- `title`, `phase`: strings from delegate, null only with no usable detail.
  `outcome`, `worker`: delegate strings or null (unknown/not yet selected).
  Phase/outcome remain delegate's values: worker exit/idle is not success.
- `steps_available`: delegate boolean, or null if no detail. Empty steps alone do
  not imply completion/failure. `steps`: at most five **newest received** normalized
  records in source order, each `{id:nonnegative_integer,at:string,kind:string,
  target:string,result:null|"ok"|"failed"}`. Supported kinds: read/edit/run/message/
  think/error. Step ids are attempt-local; no cross-read accumulation/deduplication.
  Every successful read replaces the whole task detail/activity, including resets.
- `steps_truncated`: with n received records, all-task reads yield `true` if n > 5,
  `null` if n == 5 (upstream itself caps at five), otherwise `false`. Thus even an
  unexpected over-cap feed correctly reports adapter loss. Full single-task
  fallback yields `n > 5` (true/false). It is
  null with no detail. Never interpret null as a known upstream truncation flag.
- `rejected_steps`: count rejected within the adapter's latest five-record window,
  not a full upstream malformed-record count. `detail_limited`: bounded strings or
  rejected/unsupported activity occurred. Text bounds and control replacement
  apply to cached detail too; raw payloads are not kept. Delegate may already
  have dropped malformed events, which this adapter cannot count.

Malformed envelope/duplicate/bad ids/output overflow cause full-read failure;
registered tasks are tried independently. Bad required task fields are isolated
and retried individually if registered, otherwise unavailable/stale. A degraded
missing/unreadable task stays unavailable/stale; stderr is never parsed to decide
removal. Stale detail is labelled and is not merged with subsequent valid detail.
The next successful full read reconciles the set and retires proven absences.

## Environment, bounds and safety

The child environment is an **allowlist**, not os.environ minus a few hints:
HOME; XDG_CONFIG_HOME/DATA_HOME/CACHE_HOME/STATE_HOME under STATE/home;
PATH=STATE/bin:/usr/bin:/bin; LANG/LC_ALL=C.UTF-8; TERM=dumb;
GIT_CONFIG_NOSYSTEM=1; GIT_CONFIG_GLOBAL=STATE/home/.gitconfig;
GIT_TERMINAL_PROMPT=0; synthetic GIT_AUTHOR_NAME/EMAIL and GIT_COMMITTER_NAME/EMAIL.
Nothing else is forwarded (including all HERDR_*, conversation/session hints,
DELEGATE_HOME/WORKERS/SANDBOX/SANDBOXED, EVALS_HIDDEN/RESULTS and ambient XDG roots).
Production delegate itself adds isolated DELEGATE_HOME/DELEGATE_SANDBOXED inside
bwrap; that is not inherited personal configuration. Every stand-in run/retry uses
this path. The regression stand-in would report into the selected fixture pane
if reporting hints leaked; tests assert empty hints, unchanged attribution and a
later capture still finding the lead. A negative control deliberately injects
hints inside the private stand-in launcher, runs through the identical wrapper and
unchanged bwrap, and verifies that the fixture server applies the attempted report.
The launcher is then restored, the synthetic lead identity reset, and inherited-hint
runs must produce no reports. This proves sensitivity and scrubbing, **not
socket isolation**. No personal sockets or credentials are forwarded and no
sandbox permissions are widened. A malicious worker reaching an otherwise
accessible socket is not prevented merely by deleting environment hints.

Concrete prototype limits (not production performance guarantees):

- Eight registered checkout UUIDs; sixteen linked aliases per checkout; 128 slots
  per checkout; full status lists at most 128 records. Wrapper-created/control ids
  match `[A-Za-z0-9][A-Za-z0-9_.-]{0,79}`. Read-side ids follow delegate Task::load:
  nonempty, no leading `_`/`.`, no slash/backslash. Thus external ids such as
  `fix+login` are observable without blocking the full feed or creation preflight.
  Ids are exact opaque JSON strings, never shell-interpolated or rendered;
  only the bounded `display_task_id` is a display label. Sanitization collisions
  preserve distinct raw keys; the display limitation flag is not an identity.
- 2 MiB combined stdout/stderr per subprocess, concurrently drained; 10-second
  tool deadlines, 60 seconds for stand-in run, 15-second lock acquisition.
  Snapshot source reads share a 30-second budget: once exhausted, remaining
  registered reads are unavailable/stale, never assumed absent. Child process
  groups are killed on failure/timeout. Lock holders do no unbounded waiting.
- Herdr capture has a three-second total socket deadline and 2 MiB response cap.
- 4,096 characters per displayed text field; 16 KiB per considered activity
  record. Oversized activity is rejected; other long detail is visibly limited.
  Oversized/incomplete identities become null, never truncated into another id.
- JSON uses ensure_ascii and disallows nonfinite numbers. Control/format characters
  (including ESC/bidi), plus Unicode Zl/Zp line/paragraph separators, in all display
  text (including titles and step targets) are replaced with U+FFFD. Text stays data:
  no shell=True, eval, terminal rendering, or unrequested log/stdout exposure.
- Eight MiB global persisted sidecar/cache and emitted document cap. It is a hard
  failure, not an authoritative empty result; very many maximum-sized tasks can
  hit it before count caps. Reclamation happens on discard/full observed absence,
  not by retaining retired archives. Normalization errors isolate the record.

STATE must be private/trusted and disposable, not an adversarial symlink tree.
This prototype is not a hardened filesystem sandbox or multi-user service.

## Pinned public transport and fixture evidence

`fixtures/contract.json` pins the minimal used subset and field/type/nullability,
not every PaneInfo field. `fixtures/panes.json` is **synthetic**, not a recording.
The offline smoke validator checks fixtures against that local contract. Human
inspection of upstream Rust established the subset; **not machine-verified
upstream schema conformance** and not live Pi/Claude/Codex coverage.

Upstream: `herdrdev/herdr` at **2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01**:

- `src/api/schema.rs`: Request flattens Method; `pane.get` + params.
- `src/api/schema/common.rs`: PaneTarget `{pane_id:string}`.
- `src/api/schema/response.rs`: SuccessResponse `{id,result}`; tagged
  ResponseResult::PaneInfo serializes `{type:"pane_info",pane:...}`. Errors use
  `{id,error:{code,message}}` and are refused, not coerced into missing identity.
- `src/api/schema/panes.rs`: PaneInfo's required pane_id and optional/nullable,
  defaulted/omitted agent_session.
- `src/api/schema/agents.rs`: AgentSessionInfo required strings source/agent/value
  and `kind:AgentSessionRefKind`.
- `src/agent_resume.rs`: reference enum serializes as `id` or `path`.
- `src/api/client.rs`: write_request/read_json_line pin newline-delimited JSON,
  one response envelope. Client supplies explicit socket and checks request id.

Wire request: `{"id":"request-uuid","method":"pane.get","params":{"pane_id":"pane-a"}}\n`.
Wire response: `{"id":"request-uuid","result":{"type":"pane_info","pane":{"pane_id":"pane-a","agent_session":{"source":"herdr:claude","agent":"claude","kind":"id","value":"synthetic-reference"}}}}\n`.
Only the used subset is shown; arbitrary display labels do not participate.

## Later opt-in private live gate — NOT RUN here

The lead must run this gate **after t123 and before scoping t124**. Prior research
observed actual Pi TUI and Claude startup in isolation; credential-free Codex stayed
unknown. This task does not repeat, upgrade or broaden that evidence. Stop before
native work if required identity is unavailable; record missing integration or ask
for a separate safe fallback/credential decision. Never copy personal credentials.

Example private-daemon setup, explicitly opt-in, outside automatic acceptance.
Use a short private path so UNIX socket paths fit. Choose an explicit installed
Herdr 0.9.5 binary; these commands have not been executed by this task:

```sh
: "${HERDR:?absolute explicit Herdr 0.9.5 binary}"
LIVE=$(mktemp -d /var/tmp/ho-live.XXXXXX)
chmod 700 "$LIVE"
mkdir -p "$LIVE/home/.config/herdr" "$LIVE/home/.local/share" \
  "$LIVE/home/.cache" "$LIVE/home/.local/state" "$LIVE/checkout" \
  "$LIVE/home/.pi/agent/extensions" "$LIVE/home/.claude" "$LIVE/home/.codex"
# Align config parent and isolated XDG config dir; explicitly pin the API socket
# as well, rather than guessing it from an unrelated config path or ambient session.
printf '' >"$LIVE/home/.config/herdr/config.toml"
PRIVATE_SOCKET="$LIVE/home/.config/herdr/herdr.sock"
: "${PRIVATE_PATH:?explicit executable/runtime search path, e.g. /usr/bin:/bin}"
private() {
  env -i HOME="$LIVE/home" XDG_CONFIG_HOME="$LIVE/home/.config" \
    XDG_DATA_HOME="$LIVE/home/.local/share" XDG_CACHE_HOME="$LIVE/home/.cache" \
    XDG_STATE_HOME="$LIVE/home/.local/state" PATH="$PRIVATE_PATH" \
    LANG=C.UTF-8 TERM=xterm-256color SHELL=/bin/bash \
    PI_CODING_AGENT_DIR="$LIVE/home/.pi/agent" \
    CLAUDE_CONFIG_DIR="$LIVE/home/.claude" CODEX_HOME="$LIVE/home/.codex" \
    HERDR_CONFIG_PATH="$LIVE/home/.config/herdr/config.toml" \
    HERDR_SOCKET_PATH="$PRIVATE_SOCKET" "$@"
}
private "$HERDR" server >"$LIVE/server.log" 2>&1 &
DAEMON_PID=$!
# Cleanup ONLY the child owned by this shell; never a global server-stop/pkill.
trap 'kill "$DAEMON_PID" 2>/dev/null; wait "$DAEMON_PID" 2>/dev/null' EXIT
# Wait for this exact socket (abort if child exits or readiness times out).
for n in $(seq 1 100); do
  test -S "$PRIVATE_SOCKET" && break
  kill -0 "$DAEMON_PID" || exit 3
  sleep .1
done
test -S "$PRIVATE_SOCKET" || exit 3
private "$HERDR" integration install pi
private "$HERDR" integration install claude
private "$HERDR" integration install codex
STATE="$LIVE/observer"
CHECKOUT="$LIVE/checkout"
private git -C "$CHECKOUT" init -q
private git -C "$CHECKOUT" -c user.name=Synthetic -c user.email=test@example.test \
  commit --allow-empty -qm Initial
"$OBSERVER" --state "$STATE" init --delegate "$DELEGATE_BIN" --socket "$PRIVATE_SOCKET"
"$OBSERVER" --state "$STATE" register --checkout "$CHECKOUT"

# Explicit installed absolute binaries, including any runtimes in PRIVATE_PATH.
# Do not copy credentials. No credentialed model completion is promised.
: "${PI_BIN:?absolute Pi binary}" "${CLAUDE_BIN:?absolute Claude binary}" \
  "${CODEX_BIN:?absolute Codex binary}"
for mode in pi claude codex; do
  private "$HERDR" workspace create --cwd "$CHECKOUT" --label "probe-$mode" \
    --no-focus >"$LIVE/create-$mode.json"
  PANE=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["root_pane"]["pane_id"])' \
    "$LIVE/create-$mode.json")
  # pane run joins arguments as shell input; shlex.quote every chosen argument.
  COMMAND=$(python3 - "$mode" "$PI_BIN" "$CLAUDE_BIN" "$CODEX_BIN" <<'PY'
import shlex, sys
mode, pi, claude, codex = sys.argv[1:]
commands = {
    "pi": ["env", "OPENAI_API_KEY=observer-unused-not-a-credential", "timeout", "15", pi,
           "--provider", "openai", "--model", "gpt-4o-mini"],
    "claude": ["timeout", "15", claude, "--print", "Reply with OK."],
    "codex": ["timeout", "15", codex, "exec", "--skip-git-repo-check", "Reply with OK."],
}
print(shlex.join(commands[mode]))
PY
)
  private "$HERDR" pane run "$PANE" "$COMMAND"
  # Capture during startup, not only after timeout (identity may clear on exit).
  for n in $(seq 1 30); do
    private "$HERDR" api snapshot >"$LIVE/api-$mode-$n.json"
    "$OBSERVER" --state "$STATE" capture --pane "$PANE" >"$LIVE/capture-$mode-$n.json"
    sleep .2
  done
  "$OBSERVER" --state "$STATE" create --checkout "$CHECKOUT" --id "t-$mode" --pane "$PANE"
  "$OBSERVER" --state "$STATE" run --checkout "$CHECKOUT" --id "t-$mode"
  "$OBSERVER" --state "$STATE" snapshot >"$LIVE/observer-$mode.json"
  "$OBSERVER" --state "$STATE" discard --checkout "$CHECKOUT" --id "t-$mode"
done
# Keep daemon alive for the bounded probes before terminating only our child.
sleep 16
kill "$DAEMON_PID"; wait "$DAEMON_PID"; trap - EXIT
# After inspecting retained evidence, delete ONLY the explicitly disposable LIVE.
rm -rf -- "$LIVE"
```

These exact commands follow pinned `src/cli/{workspace,pane,api,runtime}.rs`,
`src/cli.rs` (JSON envelope), `src/integration/{env,targets}.rs` (required config
parents), `src/config/io.rs` (isolated config dir) and `src/session.rs` (socket
override/default). Config-file parent and XDG config dir intentionally coincide;
HERDR_SOCKET_PATH explicitly selects the same private endpoint for server and CLI.
They require no TUI/GUI client: `herdr client` is not a supported subcommand.
They have **not been run here**. Inspect the private capture series: a null origin,
startup failure or missing auth is **unknown**, not successful integration. Pi must
run TUI, not RPC. Claude/Codex prompt attempts here have no credentials; success is
not expected or required. Never replace absent Codex evidence with a synthetic id.
The final create captures the then-current occupant, possibly unknown; it does not
retroactively select an earlier non-null capture. Record each harness/mode separately.
A controlled reporter may instead use `pane.report_agent_session`, but label it
**explicit API reporting**, not real harness integration; fixtures are a third category.
Raw ids/paths/logs remain private. These probes would establish endpoint transport,
reported identity where available and stand-in attribution, not native rendering.
Keep available integration outcomes separate; do not substitute synthetic Codex
identity to claim S137 coverage. Ordinary cleanup similarly discards all wrapped
tasks before removing STATE and its disposable checkout; never remove an active
worker's home. Observer crashes do not control delegate workers.

## Future native gate — NOT RUN, t124 still required

Only after the live gate, the lead can explicitly fetch pinned native source into
a private scratch checkout and check/apply the later t124 patch:

```sh
: "${NATIVE_SRC:?new absolute private scratch checkout}"
: "${T124_PATCH:?absolute reviewed future native patch, not supplied by t123}"
git clone https://github.com/penso/herdr-gpui.git "$NATIVE_SRC"
git -C "$NATIVE_SRC" checkout --detach 063e7e1533d0a8e52d1df0f6e2774ba5c1cc8c7a
git -C "$NATIVE_SRC" apply --check "$T124_PATCH"
git -C "$NATIVE_SRC" apply "$T124_PATCH"
(cd "$NATIVE_SRC" && cargo build --locked --release -p herdr-gpui)
```

Those commands would establish patch applicability/build only, **not** UI behavior.
They may need network/dependencies/system GPUI libraries and are never in `check`.
There is no native launch/read/action hook in t123: t124 must pin the exact isolated
GPUI launch command and hooks against its actual patch before t125 independent
properties. Do not point an unconfigured desktop client at personal state or invent
an executable UI flag here. The required lead-run native demonstrations are two
conversation groups, selection preserving the lead, retry/reset/reconnect/id reuse,
read-only closure and hostile text through the real widgets. None have been run.
All-harness live verification, independent P checks, native build/rendering and the
production instance/attempt/decision contract remain follow-up gates, not HUMAN
or no-check commands silently counted as passes.
