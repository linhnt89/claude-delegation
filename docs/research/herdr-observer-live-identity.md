# Private live identity probe (2026-11-04)

Lead-run feasibility evidence, not an eval pass or production integration.

## Isolation and method
- Installed Herdr 0.9.5 launched as a child `herdr server` with unique temporary
  HOME, XDG config/data/cache/state roots, private config/API socket and disposable repo.
- `herdr integration install pi|claude|codex` wrote only those private roots.
  No personal agent configuration, credentials or Herdr session was used or changed.
- Separate private workspaces ran actual installed harness binaries under `timeout 10`;
  snapshots polled `PaneInfo.agent_session`. The daemon child was terminated and reaped
  in a finally block. No global server-stop or process-name kill was used.
- Pi TUI was started idle with an explicitly unused placeholder API key and an explicit
  model, without submitting a prompt. Claude used a single print-mode startup prompt
  without credentials. Codex was tried both exec and interactive without credentials.
- Outputs and temporary session paths remain private/untracked; no actual session ids or
  terminal transcripts are committed. This document records only source/kind outcomes.

## Observed
| Harness/mode | Reported source / agent / kind | Scope of evidence |
| --- | --- | --- |
| Pi TUI | herdr:pi / pi / path | Actual startup via the installed integration; non-Claude identity works |
| Pi RPC | none | Expected: pinned Pi integration explicitly gates on ctx.mode == tui |
| Claude print startup | herdr:claude / claude / id | Actual SessionStart hook works before authenticated model completion |
| Codex exec / interactive | none | Credential-free probe did not establish identity; NOT proof of lack of support |

Codex exec created a thread but encountered authentication errors; interactive startup
also provided no observed identity within the probe. Do not label this passing Codex
coverage, substitute a synthetic id, or silently use its pane id as conversation identity.

## Consequence
Proceed with t123's explicitly unknown-safe headless adapter: two real harness sources
are feasible. Before native t124 scope is finalized, use the delivered adapter against
private live Pi/Claude leads and record Codex coverage as unresolved unless a safe,
authenticated isolated probe becomes available. Requiring personal credentials or
choosing a production identity fallback is a user/design decision, not a worker workaround.

Full S137 requires all named harnesses; this probe does not pass it. The temporary probe
script was exploratory, not a checked-in automated scenario. Reproducible opt-in live
commands belong to t123's deliverable and must not be confused with automatic checks.

## Lead-run adapter gate after implementation

The final reviewed t123 executable was exercised against another unique private live
Herdr 0.9.5 daemon, disposable git checkout, HOME/XDG roots and private integrations.
No personal credentials or configuration were used. Raw output remains private.

- Real Pi TUI produced a path reference and real Claude startup produced an id reference.
  `observer capture` read both through the live UNIX API; `create` persisted those origins.
- Wrapped stand-in research runs completed for both tasks. One successful snapshot showed
  exactly two live tasks with the respective captured source-qualified origins.
- A wrapped Pi-task retry preserved both task-instance tokens; a new snapshot remained
  consistent. Wrapped discards followed by a successful snapshot produced an empty task set.
- Codex interactive startup again supplied no observed identity in the credential-free
  polling window. It remains unknown/unverified, not an accepted Codex integration.
- Only the owned daemon child was terminated and reaped. Wrapped task state was discarded.
  Private directories/logs were retained temporarily for inspection; none are committed.

## Authenticated isolated Codex follow-up (2026-11-04)

User authorized a fresh isolated login and completed device authentication. Codex
0.166.0 reported logged in with ChatGPT in a new private HOME/CODEX_HOME; no personal
credentials or configuration were copied. Herdr 0.9.5 ran as an owned private daemon
with a disposable checkout and private installed Codex integration (`hooks = true`).

- Authenticated `codex exec` completed the minimal no-tools prompt with `OK` and
  printed a session id. Polling `pane.get` observed no `agent_session`.
- Initial interactive attempts stopped at disposable-folder trust and hook-review
  prompts. These attempts do not establish a running integration.
- After trusting only the disposable checkout and the installed Herdr hook in this
  private profile, interactive Codex completed the same prompt with `OK`. Polling
  still observed no `agent_session` during the bounded run.
- Thus authentication/model completion works, but real Codex-to-Herdr conversation
  identity reporting remains unverified. Missing credentials alone do not explain
  the remaining gap. Hook execution/payload/reporting needs investigation; no root
  cause or fallback has been established.
- Only owned private daemon children were terminated/reaped. Private evidence and
  authenticated temporary profile remain outside the repository; no raw identities,
  credentials or transcripts are committed. This was exploratory evidence, not a
  passing S137 or native observer acceptance run.

## Codex hook tracing and daemon-reuse reproduction (2026-11-04)

Further lead-run exploratory tracing used only the same authenticated private profile,
private daemon endpoint and disposable checkout. Instrumentation recorded the installed
hook's input, selected Herdr environment variables and API reply in private files;
no identity values or credentials are committed.

- Codex invoked the real `SessionStart` hook with nonempty `session_id` and
  `transcript_path`, `source=startup`, and the expected Herdr environment.
  `pane.report_agent_session` returned `result.type=ok`.
- The hook's `HERDR_PANE_ID` named an earlier private test pane, not the pane
  currently running Codex. Codex 0.166.0 had retained a managed app-server daemon
  under this private CODEX_HOME across CLI invocations and private Herdr restarts.
- Stopping only that private Codex daemon with the isolated environment, then
  launching interactive Codex directly (without a timeout wrapper), produced a
  real `herdr:codex` / `codex` / `id` identity on the correct pane. Its value
  matched the real hook payload. Authentication and real Codex integration can work.
- A controlled two-pane repeat started with a stopped private Codex daemon. The
  first interactive session reported correctly. After exiting that TUI, a second
  pane launched a distinct real Codex session using the same private CODEX_HOME.
  Its hook still targeted the first pane. The second pane had no identity, while
  the first pane exposed the second session's real payload identity. Both minimal
  prompts completed with `OK`. This reproduces incorrect attribution, not merely
  a missing identity.
- The trace localizes the failure to stale pane-scoped environment in the reused
  Codex app-server/hook execution context. The precise upstream implementation
  responsible and a safe production remedy still need source-level review.
- Cleanup stopped the private Codex app-server daemon via isolated CODEX_HOME and
  terminated/reaped only the owned private Herdr child. No personal daemon was
  stopped. The temporary authenticated profile remains private.

This supersedes the inference that authentication alone might resolve the Codex
identity gate. Fresh-daemon single-pane startup is verified; shared-daemon multi-pane
ownership is unsafe in this tested configuration. Do not restart personal daemons,
change production fallback rules, or treat the single-pane result as S137/P030/P032
acceptance. Next: define a regression scenario for this actual two-pane reuse case
before scoping any implementation, and review the Codex/Herdr attribution boundary.

The earlier Pi/Claude adapter gate establishes transport, wrapped retry and retirement.
None of these probes establishes native sidebar/rendering, per-fallback attempt
markers, hidden observer properties, or full S137–S144/P030–P033 acceptance.
