# Credential-free Codex attribution investigation (2026-11-04)

Lead-run exploratory evidence, not an automated eval verdict or a product fix.
S145 remains unchanged and still requires its authenticated lead gate.

## Approved profile retirement

The user approved deleting the retained temporary authenticated Codex profile.
Before removal, checked that the exact CODEX_HOME and its ancestors were not
symlinks, that the profile belonged to the current user, and that no process's
HOME/CODEX_HOME referenced it. Deleted only
`/var/tmp/ho-codex.OAzcs8/home/.codex`; confirmed it no longer exists. No credential
contents were opened or copied. Diagnostic evidence elsewhere in that private
parent was retained. This is filesystem deletion, not a claim of secure erasure
or server-side token revocation. No personal profile or daemon was changed.

The prior sandbox exposure finding was real: bwrap read-only visibility is not
credential isolation. Retiring this profile removes that specific exposure; it does
not fix the sandbox generally or t025.

## Credential-free method

Installed binaries reported Codex 0.166.0 and Herdr 0.9.5. An independently created
0700 directory contained a fresh HOME, CODEX_HOME, XDG roots, disposable git repo,
private Herdr config/socket and loopback Responses endpoint. Environment was built
from an allowlist, not inherited account/provider settings.

Provider configuration used the pinned upstream TUI test pattern
(`codex-rs/tui/tests/suite/worktree_stack.rs`):

```toml
model = "gpt-5.4"
model_provider = "local"
check_for_update_on_startup = false
cli_auth_credentials_store = "ephemeral"
[features]
hooks = true
shell_snapshot = false
[model_providers.local]
name = "Local diagnostic"
base_url = "http://127.0.0.1:<allocated-port>/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
```

The fresh disposable repo's absolute path was explicitly trusted in this private
config. Installed the genuine Herdr Codex integration in the fresh private profile;
inspected its sole SessionStart hook declaration before approving that hook through
the real TUI. The resulting shell script exactly matched pinned Herdr's
`src/integration/assets/codex/herdr-agent-state.sh`. No instrumentation, hook payload
or destination override, manual identity report, or fabricated conversation id.

The localhost endpoint returned ordinary SSE `response.created`,
`response.output_item.done` (assistant text `LOCAL_STUB_OK`) and
`response.completed` events, following upstream `core/tests/common/responses.rs`.
Only model responses were stubbed: real Codex TUI, managed app-server, session
storage, hooks, Herdr daemon, panes and public identity API were exercised.
This is credential-free, not a claim of complete network isolation: Herdr performed
an agent-detection definition fetch, and initial managed-daemon provisioning can
use the network. A future worker check needs an explicit provisioning/network policy.

## Observed

- Two real interactive TUIs ran sequentially in two distinct returned Herdr pane
  ids, sharing one fresh CODEX_HOME. A's TUI exited before B launched.
- Each launch created a distinct Codex rollout session with an independent
  `session_meta.id` and completed `event_msg` (`task_complete`/`turn_complete`).
  No thread id came from the stub or Herdr identity report.
- A reported its correct source-qualified `herdr:codex` identity.
- B completed, but pane B stayed unknown. Pane A exposed B's actual identity.
- The unchanged nonempty managed-daemon PID record was present at both completed
  observations. `/proc` independently showed the SAME managed app-server PID and
  OS process-start fingerprint holding the respective A/B rollout files open.
  Thus both completed threads have direct evidence of the same daemon serving
  them, stronger than just observing a daemon remain alive.
- Four local Responses requests completed (including auxiliary requests). None
  carried an Authorization header. No `auth.json` was created.

This reproduces the authenticated investigation's wrong-pane failure without any
credentials. It establishes feasibility of a worker-runnable sibling regression,
not full observational coverage or an accepted automated S145 failure. In particular,
the exploratory script sampled after completion rather than continuously attesting
all startup/transition windows; fingerprints were compared at completed observations,
not continuously monitored. Its private script is NOT the replacement check.

## Cleanup and operational findings

The first completed probe found that `codex app-server daemon stop` left its private
`pid-update-loop` updater running. After verifying its exact executable under this
fresh CODEX_HOME, arguments and CODEX_HOME environment, terminated only that process.
The refined repeat performed the same updater ownership check during cleanup.

Final repeat: private daemon stop completed, the owned Herdr child was terminated
and reaped, local HTTP server was shut down/joined, and `/proc` showed no remaining
process with this private CODEX_HOME. No global stop or process-name kill was used.

Installed daemon control files used **`daemon.pid` and `daemon-updater.pid`**, not
the older `app-server.pid` naming assumed by the first exploratory fingerprint
reader and some upstream CLI tests. The first completed probe therefore did NOT
prove daemon continuity. The repeat corrected this and added rollout-file ownership
and OS start-time evidence. Production check specs must validate actual binary/control
layout rather than extrapolate from a single source fixture.

Private script and raw evidence were retained temporarily under
`/tmp/codex-local-attribution.iSak6T/`; no raw ids, rollout contents, transcripts or
request bodies are committed. These artifacts are disposable diagnostics, not a
supported driver or reviewed security boundary.

## Next gate

The user approved S146 as a credential-free sibling with genuine hooks and real
shared-daemon attribution; scenario is now agreed. S145 remains intact. Oracle/driver
specs are staged in `docs/task-specs/`, with the detailed proposed contract in
`codex-s146-check-contract.md`. Renewed independent plan-review authorization is
required before that implementation wave. No further one-off Opus invocation or
product/schema patch is implied.
