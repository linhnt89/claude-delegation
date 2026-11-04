# 038: delegate records the Herdr lead that created a task; a side pane and sidebar tokens show it

- Date: 2026-11-04
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Decision 037 scoped the native dashboard to a spike on t123's private adapter. That
adapter sees only tasks it created itself and runs only stand-in workers, so the
dashboard could never show the user's real `/delegate` tasks. The user chose to have
real tasks under real Pi/Claude leads, and accepted changing the earlier design to get
there. Decision 031 said delegate does not track who leads; decision 009 records a
Claude-only owner. The installed desktop client is now Sheprd `v20261104.1`
(`penso/herdr-gpui` renamed its crates `herdr-*` → `sheprd-*`).

## Decision
- **`delegate new` records the task's origin** when run from a Herdr pane outside any
  sandbox: it asks the pane's own Herdr API (`pane.get` on `HERDR_SOCKET_PATH` for
  `HERDR_PANE_ID`) for the conversation identity. The identity is source + agent +
  reference kind + value, validated as in t123. Only Pi (`herdr:pi`/`pi`) and Claude
  (`herdr:claude`/`claude`) count as supported. A Codex identity is recorded as
  *unsupported* without its value. Anything else, a failure, or a Claude session id that
  disagrees with `CLAUDE_CODE_SESSION_ID` records nothing. A missing origin never stops
  delegate from creating or running the task, and it is never filled in later from a
  pane, checkout, focus or model name.
- **Each task gets an instance id** (a random UUID) at `delegate new`, so a reused task id
  is recognisably a different task. Per-attempt identity stays with t127.
- `delegate status --json` adds `origin` and `instance`. Legacy `session`/`session_name`
  and decision 009's views are unchanged; origin is observation only and never chooses
  reviewers, workers or plan-review families (031's plan-review rule still applies).
- **Workers lose Herdr hints**: every worker, check and review process delegate starts
  runs without `HERDR_*` variables, so its harness hooks cannot report into the lead's
  pane. This is attribution hygiene, not a security boundary.
- **No Sheprd changes.** The user picked mockup variant B. `delegate watch --beside`
  opens a read-only side pane next to the lead. It shows this lead's tasks first, with
  selection and expandable activity. The same process reports up to four `<state> · <id>`
  pane tokens (`dlg_t1`…`dlg_t4`, `dlg_more`, with a TTL) onto the lead's pane. A Herdr
  `[ui.sidebar]` layout that the user pastes in draws them as task rows under the lead.
  Tokens survive a conversation change in the pane (probe:
  `docs/research/herdr-sidebar-tokens-probe.md`). So the reporter re-checks the pane's
  identity every cycle and clears its keys on any mismatch. Delegate never edits the
  user's Herdr config.

## Rejected options
- Keep the spike on t123's sidecar — never shows real tasks; its ownership store would
  be a second authority to throw away.
- Patch Sheprd with a native panel — per-task clickable rows and cards, but a fork to
  rebase on every near-daily release, and the packaged binary would drop it on update.
  Revisit through upstream: a mod data source with agent identity and external data.
- Luau mod — mods can't see conversation identity, read files, spawn or use sockets.
- Design all of t127 first — slowest, and its open questions need a working UI.
- Capture origin on the first `run`, as 009 does for owners — a run can come from another
  pane; creation is the one moment the launching lead is certain.
- Read the lead from harness env vars alone — 031's objection still holds; the pane API
  plus Claude's own id cross-check is explicit and checkable.

## Consequences
Partly supersedes 031 ("delegate does not track who leads") for observation metadata
only, and amends 037's spike-only scope. Task state gains two optional fields; older
tasks read as unknown. Retention after merge/discard, closed-lead presentation,
cross-host visibility, per-attempt identity and Codex support stay open (t127, t128).
Two Herdr daemons can't collide on conversation ids in practice; origin carries no
daemon namespace. Sidebar rows exist only while a side pane runs for that lead, and
there are at most four per lead. Clicking a row focuses the lead, not the task. Any
local process can overwrite the tokens, so they're display only, never data or trust.
A short window (one poll interval) can show the previous conversation's rows after a
pane changes hands.
