# Herdr sidebar tokens without patching Sheprd (probe, 2026-11-04)

Lead-run exploratory evidence for the no-patch route (decision 038, variant B).
It isn't an eval pass. Private Herdr 0.9.5 daemon child with an isolated HOME/XDG,
private socket and disposable checkout. Synthetic identities only; only the owned
daemon child was stopped. Script: session scratchpad `probe/token_probe.py`, not committed.

## Pinned sources
Herdr `v0.9.5` `2ecf19a9`; Sheprd (`penso/herdr-gpui`, installed as `herdr-gpui
20261104.1-1`) `v20261104.1` `3c12204b`. Sheprd was renamed from herdr-gpui; the files
read here are unchanged from `v20261103.1` except for the rename.

## Observed through the socket API (`pane.report_metadata`, `pane.get`)
| Case | Result |
| --- | --- |
| Set token | Shows in `pane.get` as `tokens` |
| 120-char value | Cut to 80 chars |
| ESC / C0 controls | Stripped by Herdr |
| U+202E bidi override, U+2028 line separator | **Kept**: the reporter must sanitize |
| `null` value | Clears the key |
| Another `source` writes the same key | **Overwrites it**: keys are one map per terminal, not per source |
| `ttl_ms` 1500 | Key gone after 2.2 s; reporting a key without a TTL cancels its expiry |
| Second `report_agent_session` (B) while A's identity is set, with or without `seq` | `ok`, but A's identity stays |
| Agent process exits | Identity cleared; **tokens remain** |
| New conversation C reported after the exit | Identity becomes C; **A's tokens still shown under C** |
| `workspace.report_metadata` | Workspace `tokens` work too |
| Key limit | 32 keys per pane (`metadata_token_limit` error after that) |

The CLI's `herdr pane report-metadata --source <value>` failed with `unknown option`
in this build; the socket API works.

## Read from Sheprd source (not live-rendered)
- `[ui.sidebar.agents]` in Herdr's `config.toml`: `rows`, plus `rows_by_agent` keyed
  by canonical agent (`pi`, `claude`). At most 16 rows, 16 tokens per row, custom
  `$name` tokens (≤32 chars, `[A-Za-z0-9_-]`).
- A custom token with no value is skipped, and a row with no tokens left is dropped.
  So clearing `$dlg_t3` hides that row.
- Styled entry: `{ token = "$dlg_t1", fg = "#rrggbb", bold, dim, rules = [...] }`.
  A rule has exactly one of `equals`/`contains`/`starts_with`/`gt`/`lt`, plus
  `ignore_case`, `fg`/`bold`/`dim` or `hide`. The first matching rule wins (≤16 rules).
- Clicking an agent row focuses that pane. Token text is drawn in the muted role.

## Consequences for the design
- The reporter owns clearing. Every cycle it rewrites all its keys, including nulls,
  and checks the pane's identity first. A changed or missing identity clears them.
- Every report carries a TTL (a few poll intervals), so a dead reporter's rows vanish.
- Use distinctive keys (`dlg_*`) and re-assert them each cycle, since any local
  process can overwrite them. Display only: this isn't a trust boundary.
- Put the state first (`failed · t141`) so `starts_with` colour rules can't be
  triggered by task text. Never put titles in tokens.
- Not verified live: how Sheprd truncates long tokens at narrow widths, and
  rendering with a real Pi/Claude lead. Check both in the S137–S144 walkthroughs.
