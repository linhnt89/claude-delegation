# S071 See how much quota every account has left
- Job: J4
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer with several subscriptions behind their workers, before starting a batch. Decision 014.

## Starting state
Logins for Claude Code, Pi's `openai-codex`, `github-copilot` and `hyper`, as the harnesses
store them. Stand-in vendor endpoints report: Claude 5-hour 58% used and weekly 87% used,
ChatGPT 25% and 36%, Copilot 76% of its monthly premium requests, Hyper 228 of 250 credits.

## Steps
1. `delegate quota`.
2. `delegate quota --json`.

## Passes when
- One line per account in use by `workers.toml`, each with what's left per window, when it
  resets, its runway and which login it read. The tightest window is the one that counts.
- An account whose login is missing or expired says "unknown", why, and what to run to fix it.
- `--json` carries a `schemaVersion` and the same facts.
- No token, refresh token or key appears in either output, and no login file changed.
- It answers within a few seconds, reading the accounts at the same time.

## If this fails
The user can't tell which subscription is about to run out, so tasks land on one that
stops halfway.

## Failure paths
- A vendor that times out or answers with something unexpected: that account is unknown, the
  rest still show.

## Properties
- See P020.
