# Codex attribution redesign — review draft

Design only. No product patch, protocol approval, or accepted S145 verdict.
Pinned Codex: `rust-v0.166.0`, `6ff1e1ca3b30fc8b914720681fcf5fe62e95f856`.
Pinned Herdr: `2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01`.
See `herdr-observer-live-identity.md` for reproduced evidence and rejected t129.

## Independent review disposition — blocked

The approved one-off Opus review completed: **4 high, 7 medium, 4 low**.
Full findings: `codex-attribution-redesign-review.md`. This draft is not a
review-approved implementation plan; the sections below retain the reviewed proposal.
No further one-off Opus invocation is authorized by this document.

Priority corrections before task specs:
- **Credential isolation:** existing sandbox policy does NOT hide the retained
  authenticated `/var/tmp` profile. The reviewer checked readability, not contents.
  A prose prohibition is not isolation. The user subsequently approved retirement;
  the exact temporary CODEX_HOME was deleted after ownership/process checks and
  verified absent. See `codex-credential-free-attribution.md`. This resolves that
  specific exposure, not general sandbox policy; never introduce another authenticated
  profile into a worker-readable location. Codex workers still bypass bwrap.
- Credential-free local Responses probes against real installed baseline binaries
  reproduced wrong-pane reporting with the genuine hook. The user approved S146;
  oracle/driver specs and their detailed contract are staged for renewed independent
  review. This does not replace authenticated S145 or alter its agreed scenario.
- Separate fresh run-owned output from a prepared persistent private profile and
  pre-trusted checkout. Verify installed integration read-only; describe Codex's
  permitted profile writes. Do not claim all profile files are immutable.
- Use exit 77 for absent opt-in/prerequisites; opted-in broken setup returns 3.
  Add an internal deadline below 300 seconds and tested signal-safe cleanup,
  including updater ownership. Prove BOTH threads used the same recorded daemon,
  not merely that an unused daemon remained alive.
- Set explicit pinned baseline/patched binary inputs and a patch delivery/build
  contract. Remedy acceptance needs source-level two-client recording-hook tests
  workers can run, with authenticated S145 as a separate lead gate.
- Resolve family capacity before scheduling: configured deep implementers are all
  GPT. Do not put GPT-authored checks in `checks_by` on a GPT-only deep remedy.
  No worker configuration changes or model overrides are implicitly authorized.
- The oracle/driver specs and new concurrent/resume/mixed-version scenarios still
  need preparation/approval and independent review. This review had zero task specs
  and cannot certify them. Request another independent review when those exist.
- Any generic hook-declared caller-variable allowlist is only an architecture
  candidate, not an approved transport or security contract. Same-thread concurrent
  resume and omission behavior remain unsettled. No protocol patch may start.

## 1. Replacement check contract (first implementation wave)

Do not salvage the rejected 1,400-line live driver. Split pure verdict logic from
resource ownership and real-interface driving. Workers never receive the private
authenticated profile. Lead runs the authenticated gate only after offline review.
No modification to S145's agreed intent; no new dependencies.

### Inputs and ownership
- An explicit opt-in supplies a dedicated temporary authenticated CODEX_HOME, never
  a default/fallback profile. Login is interactive by the user; never copy credentials.
- Create an unpredictable, exclusive 0700 run directory for each invocation, under a
  validated private parent. HOME/XDG roots, socket, disposable repo and evidence
  belong to this run. Never reuse output paths, overwrite existing files, chmod
  preexisting entries, or accept a caller-supplied evidence filename.
- Before invoking any daemon-control command, validate the complete pinned Codex
  control-file layout, including PID/socket/lock entries and ancestry: reject
  symlinks, unexpected hardlinks, wrong owners or permissions. Determine the actual
  layout from pinned source; directory checks alone are insufficient. Reject a
  profile shared with any other launch; document the exclusive-profile assumption.
  If safe exclusion/control cannot be established, return check error before stop.
- Track owned Herdr process handles; never kill by name or use global server-stop.
  Only the explicitly private Codex daemon may be stopped, once before A and once
  during final cleanup. No restart between A and B.
- Private evidence stays outside git, mode 0600, in the exclusive run directory;
  create files exclusively. Public summaries redact ids, paths and transcripts.

### Real-interface execution
- Record exact binary versions, integration contents and effective private paths.
  Use the real installed SessionStart integration, no destination/payload overrides
  or manually published identity. Trust approval remains an explicit precondition.
- Drive two real interactive TUI launches in different actual Herdr pane ids with
  the same profile; A exits before B starts. Both minimal no-tools turns complete.
- Independently obtain each real thread id AND proof of its corresponding completed
  turn from Codex's own session evidence. Bind evidence to that exact launch, not
  newest-file heuristics or Herdr's report. Specify the pinned evidence format in
  the implementation spec after source review; unavailable evidence is setup error.
- Record daemon pid plus OS process-start fingerprint, with private control-file
  crosschecks, throughout both launches and the transition. Replacement/exit or
  incomplete fingerprint coverage is check error, never shared-daemon PASS.
- Use one synchronous owner/event loop (bounded nonblocking PTY/API operations),
  not an independently forgotten sampler. All subprocesses and readers must be
  joined/reaped before evidence is finalized. Cleanup failure prevents PASS.

### Verdict state machine
Record monotonic boundaries: A launched, A completed, A exited, B launched,
B completed, final observation. Every sample records actual pane id, timestamp,
API success/error, and qualified identity/unknown; do not discard read errors.

Predeclare polling cadence, maximum admitted observation gap and post-completion
observation duration. Require successful samples covering startup and post-reply
windows for BOTH running TUIs, plus A's post-exit state and both panes throughout
B. No empty slice, literal fixture pane id or offline-only fallback can satisfy
live coverage. Polling establishes bounded observational coverage, not proof that
no sub-cadence transient ever occurred; publish the bound with the result.

- PASS requires independently distinct completed threads, unchanged daemon,
  complete phase coverage, A=A while A runs and B=B while B runs after bounded
  startup reporting grace, plus no observed A=B at any point in B's phase.
- Any observed wrong-pane identity is a product failure even when later reads fail;
  incomplete observations must not erase concrete counterevidence.
- Valid setup with persistent missing/mismatched identity at the attribution
  deadline, including post-completion while still running, is product failure.
- Missing opt-in/auth/binary/trust/evidence prerequisites, observation gaps/API
  failures that prevent a verdict, daemon replacement or unsafe cleanup are check
  errors (exit 3), not pass. Preserve separately any observed product failure.
- An optional no-opt-in invocation must honestly report unavailable setup, never
  feature acceptance. No successful offline test substitutes for S145 live evidence.

### Reviewable work packages (not started)
1. **Check oracle + offline adversarial tests**: phase-labelled input and pure
   verdict only; no profile access, process spawning or product code. Test real
   UUID-like pane ids, missing post-reply windows for each pane, gaps/errors,
   transient wrong-pane observations, daemon replacement, identity distinctness,
   monotonic boundaries and cleanup failure. Independent family authors checks
   against the later driver via `checks_by`.
2. **Small live resource/driver layer** after oracle review: real CLI/API/PTY
   plumbing, exclusive evidence handling and private daemon lifecycle. Add synthetic
   safety tests for symlink/hardlink controls, preexisting destinations, partial
   startup, timeouts and cleanup. Review offline, then lead runs unchanged real
   S145 on the known-bad pins and requires wrong-pane failure before any remedy.

Exact filenames and executable Acceptance commands must be settled in task specs
before workers start. The source-dependent evidence/control preflight is part of
spec preparation, not a worker's permission to improvise authenticated operations.

## 2. Smallest remedy boundary

Source reads reconfirm `Hooks::new` in `hooks/src/registry.rs` snapshots server
`vars_os()`, and `reconfigured` retains it. `command_runner::build_command` clears
then replays that snapshot. TUI `thread_start_params_with_reviewer` and
`thread_resume_params_from_config` send per-call settings; current protocol params
have no dedicated caller hook-environment field. Shell-tool environment policy
is not the hook environment. Thus a hook-only change cannot recover the lost caller.

Candidate preference: an explicit optional caller-scoped hook context carried by
start/resume requests, bound to the requesting client and real thread before the
SessionStart event, then applied only to that client's launch reporting. Missing
context must suppress stale Herdr routing, not inherit daemon-global HERDR values.
Never mutate process-global env or persist pane/socket context as conversation
history. Same-thread concurrent resume must have an explicit policy, not last-writer
wins; unrelated thread hooks and background work must not migrate to a resumed pane.

This is a NEW PROTOCOL/SCHEMA CONTRACT, NOT YET AUTHORIZED. Field name/type,
allowlist, client authorization, connection-vs-thread lifetime, omission semantics,
resume/attach behavior, concurrent same-thread ownership, older-client behavior and
hook event routing must be designed before implementation. An arbitrary env map is
not approved: it could carry secrets or alter unrelated hook execution. A narrowly
scoped Herdr context also couples upstream Codex to Herdr and needs justification.

Alternatives:
- TUI-local reporter avoids daemon-global context, but requires a new integration
  entrypoint and disabling/fencing stale server-side reports; merely adding a second
  correct reporter still permits the first wrong report. It also changes the
  agreed real SessionStart integration path, so cannot silently replace S145.
- Herdr launch/protocol adapter must bind actual returned thread ids to the exact
  client; observing focused panes or shared logs is not a safe substitute. More
  machinery and no demonstrated existing safe channel.
- Existing `config` override transport is not proof of a usable hook context.
  Encoding pane routing in shell policy, hook commands or cwd would create hidden
  persistence/trust/concurrency contracts; reject absent source-backed proof.

Recommendation: obtain independent review of the check architecture and remedy
options now, then ask the user to approve the exact protocol design (or choose the
TUI-local alternative). No product implementation task is ready. A check-only wave
can proceed after its remaining source-dependent preflight/specs are settled.
Concurrency/resume and mixed-version tests are mandatory before broader remedy
claims. Native t124 remains blocked.
