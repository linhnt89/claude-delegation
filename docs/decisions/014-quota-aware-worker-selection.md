# 014: delegate reads each account's quota and picks workers that can finish

- Date: 2026-10-28
- Status: accepted

## Context
Candidates fall back only when they can't start. A worker whose subscription runs out
mid-task stops with an error or waits for the reset, and the lead switches by hand. The
user's accounts: Claude (the lead shares it), ChatGPT (Pi's `openai-codex`, Codex), GitHub
Copilot (through Pi, on a different GitHub account than the `gh` login) and Hyper (prepaid
daily credits, through Pi). quota-axi and the user's Omarchy bar already read these; both
were studied (t049).

## Decision
- **delegate reads quota itself**, natively (HTTP via `ureq` with rustls), no external tool.
- **Each candidate is checked on the login its own harness uses**: `claude:` → Claude Code's
  credentials; `pi:openai-codex/` → Pi's `openai-codex` login, `codex:` → Codex's; `pi:github-copilot/`
  → Pi's `github-copilot.refresh` (GitHub user token) against `api.github.com/copilot_internal/user`;
  `pi:hyper/` → Pi's `hyper` login (expired access renewed in memory via `/token/exchange`)
  against `hyper.charm.land/v1/credits`. `[accounts]` in `workers.toml` can override the mapping.
- **Login files are never written and Claude/Codex logins never refreshed.** No secret is ever
  printed. An expired or missing login, a timeout or an odd response makes the account
  **unknown**, which counts as available with a warning; never as zero, never as healthy data.
- **Live read before every run**, one read per account even when runs start together; reuse
  is a few minutes at most and only for that.
- **Runway, not just "% left"** (as quota-axi): projected from the pace so far in each window;
  `exhausted_now`, `projected_exhaustion` (with a time) or `through_reset`. Hyper's window is
  the day, from `[quota.hyper] daily_cap` and `resets_at` (its API gives neither).
- **A spec's `size: small | medium | long`** (default medium) needs the runway set in
  `[quota.runway]`. A candidate whose account is exhausted or would run out sooner is skipped
  like one that can't start, with the reason printed.
- **A worker that hits its limit mid-round** (quota error, non-zero exit): the runner commits
  its work, marks the account exhausted until its reset, and starts the next round at the same
  tier on the next candidate, told to continue. Quota rounds don't count toward step-ups.
  Every worker that committed is on the merge's `Worker:` trailers (decision 013).
- **Every candidate out**: stop before starting, naming each account and the earliest reset.
- **`delegate quota`** (and `--json`, versioned) shows each account's windows, runway, reset
  and source, or why it is unknown and what to do.

## Rejected options
- quota-axi as an external tool — the user wants delegate self-contained; it also misses Pi's
  Copilot login and Hyper.
- A saved quota record as the source — goes stale when accounts are used outside delegate.
- Probing with a one-word prompt — costs quota and seconds; the usage endpoints are free.
- Waiting for a reset when all are out — the lead decides; the runner says when.
- `curl` for HTTP — depends on an outside binary and on keeping tokens off its command line.

## Consequences
- New dependency `ureq` (rustls). The readers use undocumented vendor endpoints: a vendor
  change makes that account unknown, never blocks a run.
- Checks need a stand-in for each vendor: base URLs are overridable by environment variables,
  honoured only for `http://127.0.0.1`/`localhost`, so tokens can't be sent elsewhere.
