# S146 fresh provisioning preflight (2026-11-04)

Lead-run setup investigation, not an eval verdict or worker permission to access
personal profiles. No product/protocol change. No authenticated profile accessed.

## Working installed interface

On Linux x86_64, resolved installed CLI (explicit absolute path) reports
`codex-cli 0.166.0`. SHA-256:
`b8a11fedf24b2dfb59a70c9e52b030f9a508dcd49cd2dd0d41956f4c785156b8`.

With a newly created exclusive 0700 root, fresh HOME/CODEX_HOME and XDG roots,
allowlisted environment and private config containing only:

```toml
check_for_update_on_startup = false
cli_auth_credentials_store = "ephemeral"
```

run the pinned CLI with `app-server daemon start`. This succeeds on an empty
profile; stderr explicitly says `Installing daemon from CLI version 0.166.0`.
This differs from `daemon update --from-cli --yes`, which cannot select a package
on an empty profile. Do not substitute that update command for initial preparation.

Two completed fresh-root repeats selected:
`packages/app-server-daemon/releases/0.166.0-x86_64-unknown-linux-musl/bin/codex`.
The selected managed executable reported `codex-cli 0.166.0` and its SHA-256 equaled
the pinned CLI digest above. One measured fresh start took 0.952s; this is an
observation, not a guaranteed provisioning time. Do not infer network isolation:
the private updater may run network subprocesses. The provisioning opt-in is still
mandatory; no cache/profile or package selection is borrowed from personal data.

The stdout start object contains `status`, `backend`, `pid`, `managedCodexPath`,
`managedCodexVersion`, `socketPath`, `cliVersion`, `appServerVersion`.
`status=started`, `backend=pid`; all three reported versions were 0.166.0.
Resolve `managedCodexPath` and require run-owned package containment, then verify
version AND digest before trusting this preparation. A version string alone is
not sufficient. A later check-author-owned patched-artifact policy is still needed.

## Cleanup, including unsuccessful probes

`app-server daemon stop` completed successfully (one observation: 0.437s), but
left the private `app-server daemon pid-update-loop` process alive. Its exact
PID/start ticks/boot ID, private CODEX_HOME, package executable and argument vector
were checked before SIGTERM; subsequent scan found no run-profile processes.
No `auth.json` was created. No default/global daemon was controlled.

The first probe's conservative cleanup refused to signal an unexpected transient
private child and aborted before writing its report. The lead then independently
validated the remaining updater against its JSON PID record, Linux boot ID/start
ticks, private environment, package executable and exact updater arguments, and
terminated it. A subsequent scan found no processes for that root either. Two
later fresh probes completed their normal cleanup with zero remaining processes.
This is evidence for treating unexpected children as explicit cleanup errors, not
permission to use process-name kills or ignore leftover updaters.

Temporary private scripts/reports, while present:
`/tmp/delegate-codex-provision-preflight.py` and
`/tmp/codex-provision-preflight.{_0ch69bo,xbh2az93,tgpky2db}/`.
They are diagnostic inputs, not a supported runner or reusable live check.
The last probe also captured simultaneous raw PID/proc records; redacted examples
are in `evals/lib/fixtures/codex-identity/processes.json`.

## Proposed budget and component split (not reviewed)

Keep the existing total active deadline <=180s and cleanup reserve <=60s, beneath
S146's 300s outer timeout. Provisioning is part of active time, not a separate clock:
cap it at 60s, preparation teardown at 10s, leaving <=110s for private Herdr/hook
setup and real A/B observation. Every phase also checks the shared absolute deadline.
If slow provisioning consumes that budget, report check_error; do not weaken oracle
dwell/coverage requirements or silently enlarge the outer timeout. The live gate
must demonstrate this budget is adequate before merge.

`t132` should create the private profile/config, validate pinned artifacts/hook,
and supply the localhost response stub. `t131` remains the sole lifecycle owner
of provisioning commands and every child, invoking preparation validation only
after preparation daemon/updater cleanup. A's real TUI must start the attribution
sequence's daemon; no preparation daemon may survive into A.
`t133` parses snapshots only; it neither starts/stops processes nor asserts that
a driver-supplied snapshot was safely acquired. Exact APIs are staged in its spec.

## Subsequent preparation and open gates

- Full interface captures are now recorded in codex-s146-live-interfaces.md and
  lead-owned fixtures: actual hook trust cues, SSE, serving-process rollout-fd
  binding and Herdr envelopes. Ordinary Herdr RPC is NOT persistent: the server
  closes after one response. Use bounded connect-per-request; no product change.
- User approved another one-off Opus spec review through
  pi:github-copilot/claude-opus-5.5@high and continuing the S146 check through green
  merges. Review t132/t133/t131 together before workers start; no workers.toml edits.
- This implementation wave is offline-only in workers. No real binary execution,
  provisioning, live opt-in, personal profile access or sandbox escape. The lead
  performs the reviewed unchanged live baseline gate after offline tests/review.

Independent split-spec review still blocks starts. The pure t130 oracle is
unchanged; S145 and S146 remain agreed without an accepted automated live verdict.
