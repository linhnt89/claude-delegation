FINDINGS: 0 high, 1 medium, 1 low

## 1. medium: `evals/lib/codex_identity_prepare.py:437-441`, a complete request is judged "incomplete >2s" when the owner's poll tick is late

`poll()` checks the 2-second deadline for each client before it reads that client's
ready data. A request that arrived in full within the deadline, but is still sitting
in the socket buffer, is dropped and `invalid` is set whenever the next `poll()`
comes more than 2s after accept. The spec says t131 calls `poll` on every loop tick
"including waiting for setup commands", so a slow tick (a Herdr RPC, a provisioning
command, a TUI wait) is expected. In the live run this rejects Codex's genuine
`/v1/responses` POST and makes setup invalid. The spec only invalidates requests that
are *incomplete* after 2s. The stub should read what is available, then apply the
deadline. The existing deadline tests only use truly partial fragments, so they miss
this.

Reproduce: added failing test
`Stub.test_review_complete_request_survives_slow_owner_tick`. It connects, sends a
full valid POST, advances the injected clock by 2.5s, then polls. Result:
`invalid=True` and `request_count=0`.

## 2. low: `evals/lib/codex_identity_prepare.py:427,464`, `poll()` lets a raw `ValueError` escape when a descriptor is ≥ FD_SETSIZE

`select.select` raises `ValueError: filedescriptor out of range in select()` for any
fd ≥ 1024. `poll()` only catches `OSError`, so in an owner process with many open
descriptors (Herdr sockets, PTYs, pipes, evidence files) the exception escapes. The
owner loop gets an unredacted exception instead of `invalid=True`. The spec says
socket errors set invalid. Using `selectors`/`poll`, or catching the error, would
avoid this. It is unlikely at the expected descriptor counts, hence low.

Reproduce: added failing test `Stub.test_review_high_descriptor_poll_is_redacted_error`.
It raises RLIMIT_NOFILE, holds 1100 fds, creates a stub and calls `poll()`. It fails
with a `ValueError` from `select`.

## Checked with no finding
- Profile: port/bool rejection, root mode/uid/emptiness/symlink/ancestor checks, O_EXCL
  0600 files, chmod only on new directories, env allowlist (no os.environ), TOML matches
  `config.toml` (including the quoted trust key), Herdr config/socket paths.
- `resolve_managed_package`: start stdout fields/versions/pid parsed before any path use,
  exact symlink targets, executable reached through canonical no-follow components,
  digest/hardlink/uid/exec checks, private control dir, symlink drift recheck.
- `validate_hook` exact declaration (including the int-only timeout), script digest,
  link/FIFO/hardlink/uid/drift rejection. `assert_no_auth` uses lstat only.
  `validate_managed_version` is an exact stripped match.
- Stub: Authorization in any casing/empty/duplicate, Transfer-Encoding/Upgrade,
  duplicate or non-numeric Content-Length, pipelining, header/body limits, 64-request and
  8-connection caps, fixture-exact SSE with unique IDs, `/v1/models` not counted,
  idempotent close that keeps `invalid`.
- The acceptance unittest commands pass (25 tests, ~0.03s) without my two added tests.
  I did not run `scripts/check` because my added tests fail on purpose.
