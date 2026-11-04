# Pinned S146 live interfaces and acquisition contract

Lead credential-free captures, Codex 0.166.0 / Herdr 0.9.5, 2026-11-04. These are
interface evidence, NOT an automated S146 verdict. No authenticated data or personal
daemon/profile was used. Component specs are t132/t133/t131 in docs/task-specs/.

## Critical correction: Herdr is NOT a persistent ordinary RPC socket

A live workspace.create succeeded with a normal response; sending pane.get on that
same connection raised BrokenPipeError. Pinned Herdr src/api/server.rs
handle_connection_with_stop reads one initial request, dispatches and returns for
ordinary methods. Only subscriptions/other specialized operations stream.

Use ONE nonblocking Unix connection per ordinary request, newline-delimited JSON.
Response envelope is `{id,result}` or `{id,error}`, NOT `{ok:true,...}`. Verify
correlation, response type and actual returned pane id. EOF before full response,
wrong id, extra uncorrelated data or missing/malformed result is a read error.
Connection + write + read together have <=0.2s absolute operation budget; two pane
reads plus process sampling <=0.6s cycle budget. Do not introduce a public API
change, fake persistent support, CLI subprocess per sample or weaken the 1s gap rule.

Captured methods/fields:
- workspace.create: `{cwd, label, focus:false}`, response result.type workspace_created,
  actual pane from result.root_pane.pane_id; keep workspace_id for disposal.
- pane.send_input: `{pane_id, text, keys:["Enter"]}` for shell command submission;
  result.type ok. `herdr pane run` uses this same method, not a pane.run RPC.
- pane.read: `{pane_id,source:"visible",strip_ansi:true}`;
  result.type pane_read, result.read.text. Read keys, shape and limits must be checked.
- pane.get: `{pane_id}`, result.type pane_info, result.pane.agent_session contains
  qualified identity when present. Absent or null agent_session on a successful,
  correctly bound pane response is explicit unknown; malformed identity is error.
- pane.send_keys `{pane_id,keys:["C-c"]}` for cancellation. A_exit requires observed
  actual TUI child exit and shell return, not a successful send-keys acknowledgement.

Redacted fixture: evals/lib/fixtures/codex-identity/herdr-api.json. It includes actual
create/get/send envelopes, exact hook-review screen, and the previously observed
qualified identity projection (not a fabricated successful get response).

## Hook trust and profile

Install via explicit pinned Herdr binary `integration install codex` in the fresh
allowlisted environment. Before approval, verify script bytes/hash and exactly the
sole SessionStart declaration. Real screen: Hooks need review, 1 hook new/changed,
option 2 Trust all and continue. Send `pane.send_input` with text `2`, keys Enter
only after seeing those exact cues AND revalidating the one genuine hook. Approval
is at most once per profile; timeout/mismatched screen is error, not blind input.
The source asset is included unchanged as herdr-agent-state.sh fixture; pinned
Herdr source commit 2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01 (Apache-2.0; copied
license in the fixture directory). Never execute this fixture as a replacement hook. Live installation must
come from Herdr, and script digest must match the approved fixed value.

All writable paths, including TMPDIR for the genuine hook's mktemp, must be below
the exclusive run root. New root and environment are t132's API; t131 owns commands.
Private shell invocation uses /bin/bash --noprofile --norc. Do not inherit account,
proxy, HERDR_ENV/HERDR_PANE_ID or CODEX_THREAD_ID; real Herdr pane launch supplies
its actual pane context to the TUI and subsequently the genuine hook.

## Model stub

Captured POST /v1/responses (one body 52869 bytes); no Authorization header.
Three ordinary SSE events response.created, response.output_item.done, and
response.completed produce LOCAL_STUB_OK. Response/message ids are local model
objects, NEVER conversation identities. See local-responses.json for exact wire
text; raw model request bodies are intentionally not retained in public fixtures.
An auxiliary title request may also occur. Bind completion only from real rollout
records, not number of stub requests, title output, terminal text or Herdr reports.

t132's stub is nonblocking and serviced by t131 every loop tick, including command
waits. No background thread, remote provider, credential handling or request dump.
Detailed request limits and rejection behavior are in t132 spec.

## Independent serving evidence and trusted acquisition (t131)

The lead captured one completed real rollout, then a simultaneous daemon.pid and
/proc snapshot showing that same app-server held the exact rollout inode/device
open. See serving-fd.json for redacted projected relation. Distinct real A/B
completed rollouts and same-serving-process evidence were also observed in the
prior investigation; this new capture is a separate run, not an extra A/B verdict.

Collectors must:
- Acquire run-owned regular uid-owned nlink=1 control/rollout files with descriptor
  no-follow traversal; reject link/type/owner and unexpected-layout hazards.
- Read PID record through a stable descriptor; get boot ID and /proc stat start
  ticks before/after collecting exe/cmdline/environ and fd targets. Reject changes,
  permission denial, process exit, truncated reads or unsupported records.
- Open /proc/PID/exe and compare its SHA-256 to the preparation-validated pinned
  managed executable; never trust executableIdentity.digest as SHA-256 proof.
- For serving proof, open the chosen rollout, fstat dev/ino, then fstat matching
  /proc/PID/fd/N and compare dev/ino + exact canonical target to that descriptor.
  Recheck rollout link/owner and PID/boot/start after acquisition. A matching path
  string alone is not evidence of the same open file.
- Pass in-memory parsed PID and proc snapshots to t133.attest_process. Tokenize
  returned (boot,pid,start) privately for t130 serving_instance/daemon_samples.
  t133 validates snapshots; it cannot establish that collection was trustworthy.
- Use pidfd handles where available (required on baseline Linux) for termination,
  acquired before final PID/start/exe/env/argv recheck. Never authorize kills merely
  from a prior successful parser return. Exact-owned child handles are also valid.
  Validate control files again before any CLI daemon stop dispatch. No global stop.

The last exploratory cleanup conservatively aborted on a transient non-package
child after terminating owned package processes; a subsequent independent /proc
scan confirmed zero processes for that CODEX_HOME. This is not a reviewed cleanup
loop. t131 must handle transient children by bounded waiting and report leftovers,
not signal arbitrary inherited-env processes. The previous preparation preflight
also demonstrated the updater survives normal daemon stop: handle it explicitly.
No auth.json was created in any capture. Source/private evidence while available:
/tmp/codex-interface-capture.yc7jf_5d/. Initial capture attempts aborted safely on
incorrect ok-envelope and persistent-connection assumptions, with no model turns;
all their private-profile processes also exited.

## Baseline artifact pins / permission

Explicit canonical inputs to the later lead live gate:
- Codex /home/linhnt/.local/share/mise/installs/codex/0.166.0/bin/codex,
  SHA-256 b8a11fedf24b2dfb59a70c9e52b030f9a508dcd49cd2dd0d41956f4c785156b8.
- Herdr /home/linhnt/.local/share/mise/installs/herdr/0.9.5/herdr,
  SHA-256 14d9e8bc8b4a9e0118d77c32266f76d461f09b40342eb23752960bd89f59929d.

Workers use offline synthetic processes/loopback only, NOT real binaries, profile
provisioning, personal data or sandbox escape. Lead runs reviewed unchanged driver
with both explicit opt-ins after offline checks and review. It must genuinely FAIL
on the wrong-pane baseline, not check_error or skip, before merge. This does not
change S145, approve a product/protocol remedy or claim authenticated acceptance.
