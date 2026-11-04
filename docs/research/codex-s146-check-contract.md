# S146 check contract — proposed implementation boundary

S146 is agreed. The actual t130/t131 specs received the authorized independent Opus
review (`codex-s146-spec-review.md`); this revision addresses its oracle ambiguities
and splits the later driver into smaller waves. No product/protocol changes. S145
stays untouched. t130 is the first implementation wave; live packages remain blocked
until the fixtures/provisioning preflight and their split specs are complete.

## Packages and gates

- t130: pure trace oracle/adversarial tests and standing test gate, explicitly
  `--tier standard`, no live processes/profiles. Stop on a GPT author/step-up;
  record actual selected family before merging.
- t132: local SSE stub, fresh profile/provisioning and genuine hook validation.
- t133: pure rollout/PID/proc evidence readers, tested against lead-redacted fixtures.
- t131: single-owner live loop, signals/cleanup, wrapper and docs, explicitly
  `--tier deep`; depends on merged t130/t132/t133 and their actual Worker trailers.
  `checks_by: t130` enforces oracle/driver family separation; never start while t130
  is unmerged, because the runner otherwise only warns and runs without the rule.
- Specs are staged in `docs/task-specs/`. Only t130 is currently implementation-ready;
  do not run the superseded oversized t131 spec. The later-wave specs/fixtures need
  completion before any live task starts. Credential-free live opt-in in a worker
  sandbox is permitted only after the lead validates its explicit pinned binary paths,
  provisioning input and sandbox access; no credentials or sandbox escape allowed.
- Configured standard candidates start with DeepSeek/Muse; deep candidates are GPT.
  Do not pin a model or edit workers.toml. If t130 falls back to GPT, t131's deep
  candidate pool will be empty: stop and ask, never bypass `checks_by`.
- A later remedy must exclude ALL check-author families. A GPT-authored t131 means
  the current GPT-only deep remedy pool is blocked. Ask for independent non-GPT
  deep capacity before that wave; no remedy worker is scheduled by this plan.

## Driver inputs and boundaries

No opt-in: exit 77 before reading any profile, resolving binaries, spawning processes
or making connections. Non-Linux also exits 77 before side effects even if opted in.
Opt-in is `DELEGATE_CODEX_LOCAL_PROBE=1`. Missing prerequisites AFTER opt-in on Linux
are broken setup (exit 3), matching S146's opted-in-broken-setup rule.

Required explicit absolute executable paths: `DELEGATE_CODEX_BIN` and
`DELEGATE_HERDR_BIN`. Resolve symlinks once, record canonical path, file digest and
version. Require Codex 0.166.0 / Herdr 0.9.5 for the initial baseline. Never silently
resolve `latest`, PATH defaults or a patched binary under the baseline label.
Future patched-build support needs an explicit reviewed version/digest policy.
Opted-in missing/mismatched inputs are check errors, not skips.

Create an exclusive 0700 run directory under EVALS_WORK (never accept an existing
CODEX_HOME or arbitrary output filename). Everything writable belongs below it:
HOME/CODEX_HOME/XDG roots, repo, sockets, evidence, session files and logs. Use a
minimal explicit environment with no inherited credentials, proxy/provider config,
HERDR context, shell init or account settings. Use a shell without user rc files.

### Provisioning (explicit, not claimed offline)

The installed Codex managed-daemon package is distinct from the CLI. A fresh root
can need network provisioning; never reuse a personal package/profile. Require
`DELEGATE_CODEX_LOCAL_PROVISION=1` before permitting the initial private daemon
start/download. Without it return 3 before startup; don't accidentally use network.
All tests except the deliberately opted-in live gate are credential- and network-free.

Lead preflight: `daemon update --from-cli --yes` on an empty private profile fails
with 'no daemon package is selected'; do not rely on it as initial provisioning.
Allow only initial private preparation start/stop before A. Verify the selected
managed executable's canonical path lies under the run-owned package root and its
version matches 0.166.0. Record its digest. Fully stop/reap the initial daemon and
updater before A; then A's real TUI creates the shared daemon with A's actual pane
context. Never stop/restart it during the A→B sequence. A version drift is error.
Other incidental network (e.g. Herdr definition refresh) is not model traffic;
report it honestly, never claim air-gapped execution. Model traffic goes only to
the allocated loopback endpoint; reject any Authorization header and auth.json.

Install the real integration only into the fresh private profile. Require installed
script SHA256 `c34709efe76f4d83614b47f5b3f5880b621a0f53bf96022393db0d4686b29340`
(pinned Herdr asset). Inspect hooks.json structurally: exactly that SessionStart
command, script path inside the run, no extra hooks. Trust only that hook via the
private TUI (no trust-bypass feature). Trust only the disposable repo via its exact
absolute config key. No rewriting script, payload or report destination.

### Independent thread/completion evidence

Before each launch inventory session files; bind exactly one new rollout to that
launch using its session_meta id/cwd, launch time and completed user turn. Refuse
ambiguous/new auxiliary sessions instead of selecting 'newest'. Parse event_msg
`task_complete`/`turn_complete` for the actual submitted minimal turn, not screen
text or an unrelated auxiliary title response. A completion must have matching
turn-start/user-prompt evidence and the expected assistant reply. Lead preflight
confirmed baseline records: session_meta has id/cwd/cli_version/model_provider;
event_msg task_started and task_complete carry the same turn_id/root_turn_id;
response_item message with role=user/input_text carries the exact submitted prompt;
assistant/output_text and task_complete.last_agent_message both contain LOCAL_STUB_OK.
There can be preceding system-context user messages: do not mistake them for the prompt.
Require the expected sequence for one turn; unknown format is check error.
Thread ids NEVER come from the local model stub or Herdr's report.

At each completion prove the actual managed app-server holds that thread's rollout
open through /proc/<pid>/fd. Verify process exe/cmdline, private CODEX_HOME, PID
record and Linux boot id/start ticks agree, then compare A/B instance fingerprints.
Sample daemon liveness/fingerprint throughout transition too. Both threads must
belong to the same serving process, not an alive but unused daemon.
Actual observed controls are daemon.pid / daemon-updater.pid (JSON with processIdentity),
not older app-server.pid fixtures. Reject unsupported layout rather than guessing.

## Trace interface (eval-private JSON, version 1)

`t130` supplies `evaluate(trace: dict) -> dict` in
`evals/lib/codex_identity_oracle.py`. Result keys: `verdict` (`pass`, `fail`,
`check_error`), `reason` (redacted stable text), `issues` (redacted codes).
No IDs or paths in result text. Malformed input returns check_error, not traceback.
Trace is a complete in-memory private record, never a live fake-event input mode.
The live CLI must not accept supplied traces, fake executables or synthetic panes.

Trace fields:
- `version: 1`, `panes: {A: actual_id, B: actual_id}` (distinct opaque nonempty strings).
- `threads: {A: {id, source: 'herdr:codex', agent: 'codex', kind: 'id',
  started, completed, evidence_valid, serving_instance}, B: ...}`.
  Times are monotonic seconds; independent ids are distinct. For each label,
  started equals its launch boundary and completed equals its reply boundary:
  these are driver-observed times, not wall-clock rollout timestamps.
  evidence_valid is set only after driver checks the independent evidence above.
- `boundaries: {a_launch, a_reply, a_exit, b_launch, b_reply, end}`; finite,
  strictly ordered: a_launch < a_reply < a_exit < b_launch < b_reply < end.
  a_exit means confirmed TUI process exit/foreground-shell return, not sent Ctrl-C.
- `samples: [{time, pane, ok, identity}]`; pane is the actual opaque string from
  panes.A or panes.B, NEVER the role label 'A'/'B'. Unknown/unbound pane strings are
  malformed; the oracle maps actual values back to roles. identity is null (explicit
  unknown) or `{source, agent, kind, value}`. Failed reads have ok=false; retain them.
- `daemon_samples: [{time, ok, instance}]`; instance is a redacted private token
  bound by the driver to boot id/PID/start ticks. No empty instance is valid.
- `setup_valid: bool`, `cleanup_ok: bool`, `interrupted: bool`.

Fixed oracle policy (not caller-tunable): reporting grace 10s after each launch;
required fully attributed post-reply dwell 3s before A exit / final end; specifically,
a_exit >= max(a_reply, a_launch + 10) + 3 and end >= max(b_reply, b_launch + 10) + 3.
Require correct qualified identity on every successful sample of each running pane
from max(its reply, its launch + 10) to its exit/end, not merely a historical match.
Sample target cadence 0.2s, maximum admitted gap 1s including edges.
Cover both panes from B launch to end;
cover A from A launch through its post-exit transition to B. Daemon coverage is
required on the closed interval [a_reply, end]. Require successful
post-reply samples for both running TUIs. Every failed read in a required window
prevents pass unless a concrete independently attested product failure is present
anywhere in the trace (before OR after that read error).

Known qualified wrong-pane publication is failure even during startup grace:
A publishing B's full qualified identity after A exits, or B publishing A's full
qualified identity during B's phase. Compare source/agent/kind/value as one tuple;
colliding string values in another namespace are not that counterexample.
Any foreign/mismatched qualified identity while a TUI is running after grace is failure. Explicit unknown
continuing through reporting deadline is failure only when successful observations
attest it; absence of samples is error. A startup mismatch during grace cannot hide
wrong-pane publication. A may retain A/be unknown after exit; it must never become B.
Correctness is source/agent/kind/value equality, not merely equal value.

The pure oracle checks only distinct nonempty opaque pane strings; the driver,
not the oracle, proves they came from real pane-create responses.

Verdict precedence (independent of error/counterexample temporal order):
| Preconditions / observation | Verdict |
| --- | --- |
| Malformed schema/types/nonfinite or inconsistent time/binding data | check_error |
| setup_valid=false, or either independent thread evidence_valid=false | check_error, even if a sample appears wrong |
| Valid setup/schema/bindings/completed threads plus concrete wrong qualified identity (including A=B) | fail, retaining coverage/daemon/cleanup/interruption issues |
| Valid evidence plus explicit unknown on a successful read at/after that pane's reporting deadline while running | fail, retaining additional issues |
| No concrete counterexample, but required coverage/serving-instance proof/continuity/cleanup missing or interrupted | check_error |
| All required coverage, identity, daemon and cleanup conditions satisfied | pass |

For fail precedence, serving-instance continuity can be broken (record the issue),
but independent completed thread identities and valid pane binding must still be
established. Unknown before the deadline or after A exits is not counterevidence.
A successful wrong report after deadline needs no persistence inference; missing
samples are never explicit unknown. A is running on [a_launch,a_exit), not at the
observed exit boundary itself; unknown at exactly a_exit is permitted. B remains
running through the inclusive final end boundary. Required identity/post-reply
checks use those same half-open A and inclusive B windows. Authorization headers, auth.json, hook changes
or leaked credential prerequisites invalidate setup: check_error, not a Codex
attribution failure. Bounded polling cannot prove absence of sub-cadence transients.

## Resource and timing policy

Single owning synchronous event loop, no forgotten sampling threads. Nonblocking
PTY/API/process IO over persistent sockets; each operation <= 0.2s and the entire
sample cycle (both panes plus daemon) <= 0.6s. This leaves margin under the 1s max
gap; enforce cycle deadlines rather than adding independent timeout allowances.
Use monotonic absolute deadlines. Active run <= 180s, reserve up to 60s for cleanup,
so final result precedes the scenario's 300s outer timeout. SIGTERM/SIGINT enter
the same bounded cleanup path and cannot yield pass. Test interruption at each
resource-acquisition and A/B phase, including cleanup.

Exclusive evidence files: no truncation/chmod of preexisting entries, no caller
path reuse. Validate symlink/hardlink/owner hazards on control files before every
daemon-control dispatch; only the new owned profile is ever controlled. Process
termination uses exact owned handles or validated PID+start+exe+private-env identity;
never process-name kills, default sockets or global server-stop. Verify and terminate
the run's private updater too. Reap owned children; stop and join the localhost
stub; verify no run-profile processes remain before finalizing evidence/verdict.
No guessed identity or manually published API report is allowed even for cleanup.

## Acceptance boundary

Offline oracle/resource tests must pass in normal runner sandboxes, using only
synthetic temporary processes/files and local fixtures. Missing opt-in must skip
honestly through evals/run S146. These are not a live scenario pass.

Lead then runs the reviewed driver unchanged against the explicitly pinned known-bad
binaries with both opt-ins: evals/run S146 must report FAIL with wrong-pane evidence,
not check error, skip or only missing identity. Capture redacted evidence and verify
cleanup. This is the expected baseline regression, not a remedy acceptance result.
Only after that evidence may the lead mark S146 automated. A product remedy remains
blocked on its own design, build policy, independence and plan review gates.
