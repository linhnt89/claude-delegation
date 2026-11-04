# Task t124: Show a lead's delegated tasks in the Herdr sidebar and a side pane, without patching Sheprd

size: long

## Goal
Let a developer watch real delegated tasks next to the Pi/Claude lead that created
them, in the installed Sheprd client with no Sheprd changes (decision 038, variant B):
- one sidebar row per task under the lead, from Herdr pane tokens
- a read-only `delegate watch` side pane with selection and expandable activity

Built on t139's `origin` and Herdr socket client.

## Readiness
- t139 merged (origin, instance, Herdr client, workers without `HERDR_*`).
- Scenario S148 is automated (fails until built). Hidden S149/S150 run at review.

## Context (read these, nothing more)
- `AGENTS.md`; decisions `035`, `037`, `038`
- `docs/research/herdr-sidebar-tokens-probe.md` (observed token behaviour; required)
- `docs/task-specs/t139-herdr-lead-origin.md`, section "Contract"
- `delegate/src/watch.rs`, `status.rs` (`TaskView`, `collect`), t139's Herdr client
  module, `main.rs` (`Cmd::Watch`)
- `evals/scenarios/S137`–`S144` `scenario.md` (what the user must see)

## Scope
- In: `delegate/src/watch.rs`, new `delegate/src/herdr_view.rs`, the t139 Herdr client
  module (add `pane.report_metadata`, `pane.split`/`pane.run` or their equivalents),
  `delegate/src/main.rs` (CLI args), `.agents/skills/delegate/SKILL.md` and its
  `delegate/templates/project/` mirror (a short "watch beside" section), unit tests
- Out: Sheprd and Herdr sources, the user's Herdr `config.toml` (never edited),
  `status --json` schema, task state, worker launching, checks, dependencies

## Contract
### `delegate watch --beside`
- Run from a lead's pane (`HERDR_PANE_ID` and `HERDR_SOCKET_PATH` set, not sandboxed).
- Splits a pane to the right of that pane without taking focus. It runs
  `delegate watch --lead-pane <that pane id>` in the new pane, with the lead's checkout
  as cwd, and prints the new pane id.
- If the variables are missing it fails with a message saying what to do. It never
  changes focus or sends input to any other pane.

### `delegate watch --lead-pane <PANE>` (the side pane)
- Every interval (default 2 s), it does these reads only:
  - `pane.get` for PANE
  - `delegate status`'s own collection for this checkout (in-process, same as `status --json`)
  - the selected task's detail
- No other delegate action. Stdin is read only for its own keys.
- **This lead** is PANE's current `agent_session` when it is a supported Pi/Claude
  identity, validated exactly like t139. Otherwise it is unknown, and the pane says why:
  "no conversation in this pane", "Codex: attribution unsupported" or "lead unavailable".
  It follows the pane: if the conversation in PANE changes, "this lead" changes with it.
- Layout:
  - Header: `This lead · <agent> · <6-hex digest of the identity value>` and the health
    (`live · read 2s ago` / `stale since …` / `unavailable`).
  - Then this lead's tasks, then collapsed groups: other leads (by digest), Codex
    (unsupported) and unknown lead, with counts.
  - Rows show id, title, delegate's phase/outcome and a stalled mark.
- Keys:
  - `↑`/`↓` move the selection
  - `enter` expands or collapses the selected task: delegate's steps for the latest
    attempt, worker, tier, changed files, `next`. "No activity yet" if there are no steps.
  - `tab` cycles groups, `q` quits.
  - Selection is keyed by (task id, instance). A task that disappears shows
    "task no longer available" and nothing else is selected automatically.
- Text: replace Unicode category C, Zl and Zp characters with U+FFFD, keep it to one
  line, bound it to the pane width and mark truncation. No raw logs or stdout.
- Raw keyboard input uses `stty` on the controlling terminal (no new crates). The
  terminal is restored on `q`, SIGINT, SIGTERM, SIGHUP and panics. When stdout isn't a
  terminal, it falls back to today's non-interactive frames. Those frames keep the
  header, so "stale" and "lead unavailable" still appear. Token reporting is the same
  in both modes.

### Sidebar tokens (reported by the side pane)
- Each cycle writes, in one `pane.report_metadata` call on PANE with
  `source = "delegate:watch"`, a rising `seq` and `ttl_ms = 3 × interval`:
  - `dlg_t1`…`dlg_t4`: this lead's first four tasks, ordered by attention (failed or
    blocked, stalled, ready to merge, running or checking, others), then id.
    Value: `<state> · <id>` where state is one of `failed`, `blocked`, `interrupted`,
    `stalled`, `ready`, `checking`, `running`, `waiting`, `done`. Never a title.
  - `dlg_more`: `+N more` or null
  - every unused key: null
- All keys are null when this lead is unknown or unsupported, or the status read failed
  for longer than the TTL. On `q`/signal exit, one final report clears them all.
- Values are sanitized (above) and at most 80 chars.
- `delegate watch --sidebar-config` prints a ready-to-paste `[ui.sidebar.agents]
  rows_by_agent` snippet for `pi` and `claude`. It keeps the default first rows and
  adds `$dlg_t1`…`$dlg_t4`, `$dlg_more`, with `starts_with` colour rules (failed,
  blocked and interrupted red; stalled amber; ready green; running and checking blue).
  It is printed only, never written.

### Failure behaviour
- If Herdr is unreachable, the pane shows "lead unavailable" and keeps showing tasks
  by origin. It never assigns tasks to another lead.
- If a status read fails, the last good list is labelled stale with its age. It never
  becomes an empty list.
- Closing the pane or exiting never touches workers or task state.

## Acceptance
- [ ] `scripts/check`
- [ ] `evals/run S148 S055 S056 S057 P009`
- [ ] Unit tests with a fixture Herdr socket cover:
  - a supported lead's four rows and `+N more`
  - attention ordering
  - nulls for unused keys
  - all keys cleared for a Codex identity, a missing identity, an identity change
    (the probe's "C inherits A's tokens" case) and the final exit report
  - TTL and seq present on every report
  - state-first values with no title text
  - sanitizing bidi, U+2028 and over-long values
  - stale after a failed status read
  - selection keyed by instance across id reuse
  - `--beside` refusing without Herdr variables and never changing focus
  - the `--sidebar-config` snippet parsing as TOML with valid rule keys
- [ ] S148 (fast, automated):
  - a stand-in Herdr socket and two stand-in leads
  - the side pane's reports for lead A carry only A's tasks
  - after a conversation change in A's pane, a clearing report follows within one interval
  - no request other than `pane.get`/`pane.report_metadata` is sent
- [ ] Review: no Herdr config is written, no input is sent to panes, and the side pane
      runs only reads plus its own metadata reports.

## Constraints
- No new dependencies. `anyhow`, no `unwrap()` outside tests, clippy pedantic clean.
- Never infer a lead from pane position, focus, checkout name or model. Only
  `agent_session` equality with t139's recorded origin counts.
- Reports are display-only. Don't treat any token you read back as data.

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
