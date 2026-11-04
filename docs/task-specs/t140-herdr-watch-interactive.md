# Task t140: Interactive side pane, `--beside` launcher and sidebar config snippet

size: medium

## Goal
Finish variant B of decision 038 on top of t124's reporter. A lead opens the side pane
next to itself with one command, then selects and expands tasks there with the keyboard.
`--sidebar-config` gives the user the Herdr layout that draws t124's tokens. The skill
says how to use it.

## Readiness
- t124 merged.
- S151 agreed and automated (fails until built).

## Context (read these, nothing more)
- `AGENTS.md`; decisions `038`, `039`
- `docs/research/herdr-sidebar-tokens-probe.md` (sidebar layout keys and rule syntax)
- `docs/task-specs/t124-herdr-watch-beside.md`, section "Contract"
- `delegate/src/watch.rs`, `herdr_view.rs`, t139's Herdr client module, `main.rs`
- `.agents/skills/delegate/SKILL.md` and its `delegate/templates/project/` mirror
- `evals/scenarios/S151-*/scenario.md`, `S138-*/scenario.md`

## Scope
- In: `delegate/src/watch.rs`, `delegate/src/herdr_view.rs`, the Herdr client module
  (add `pane.split` and `pane.send_input`, from Herdr 0.9.5's API),
  `delegate/src/main.rs`, `.agents/skills/delegate/SKILL.md` and its template mirror,
  `delegate/templates/kit-history.txt` (run `scripts/kit-history` after the skill
  change, before `scripts/check`), unit tests
- Out: t124's reporter and reader rules, dependencies, Sheprd/Herdr sources, the user's
  Herdr config, `status --json`

## Contract
### Keys in `--lead-pane` on a terminal
- Rows keep t124's attention order. The selection starts on the first row.
- `↑`/`↓` move the selection over this lead's tasks, then the collapsed groups.
- `enter` expands or collapses the selected task: delegate's steps from t124's
  `read_tasks` with a 50-step budget, worker, tier, changed files and `next`. "No activity yet"
  if there are no steps.
- `tab` moves to the next group; `q` quits (t124's exit path).
- Selection is keyed by (task id, instance). A selected task that disappears shows "task
  no longer available" and nothing else is selected automatically.
- Keys never reach any other pane or process.

### `delegate watch --beside`
- Run from a lead pane (`HERDR_PANE_ID` and `HERDR_SOCKET_PATH` set, not sandboxed).
- It sends one `pane.split` with `target_pane_id` = the lead pane, `direction: right`,
  `focus: false`, `cwd` = the lead's checkout, and
  `env = {DELEGATE_LEAD_PANE: <lead pane>, DELEGATE_BIN: <absolute current exe>}`.
- Then one `pane.send_input` to the new pane with the fixed text
  `exec "$DELEGATE_BIN" watch --lead-pane "$DELEGATE_LEAD_PANE"` and keys `["Enter"]`.
  Herdr 0.9.5 has no run-argv method; this is what `herdr pane run` does. The text never
  contains paths, ids or task text.
- Prints the new pane id. Without the variables, it fails saying to run it from a Herdr
  lead pane. It never sends input to the lead pane.

### `delegate watch --sidebar-config`
- Prints a `[ui.sidebar.agents]` snippet that parses as TOML. Its `rows_by_agent`
  entries for `pi` and `claude` keep Sheprd's default first rows, then add `$dlg_t1`…
  `$dlg_t4` and `$dlg_more`.
- Colour rules use `starts_with`: failed, blocked, interrupted and unavailable red;
  stalled amber; ready green; running and checking blue.
- It also prints a line saying where to paste it (Herdr's `config.toml`) and that delegate
  never writes it.

### Skill
- A short "Watch beside the lead (Herdr)" section in the delegate skill:
  - `delegate watch --beside`
  - pasting the snippet once
  - what the rows mean
  - the limits: at most four rows, clicking a row focuses the lead, rows exist only while
    the side pane runs

## Acceptance
- [ ] `scripts/check`
- [ ] `evals/run S151 S148`
- [ ] Unit tests cover:
  - key handling (selection, expand, tab and disappearance)
  - `--beside` request arguments and its refusal without Herdr
  - the snippet parsing and its rule keys matching Sheprd's (`docs/research/…`)
- [ ] Lead-run walkthrough before merge: S137 and S138 steps with real Pi and Claude leads
      in installed Sheprd, recorded in the review (HUMAN, not counted as a product pass).

## Constraints
- No new dependencies. No `unsafe`, `anyhow`, clippy pedantic clean.
- No shell strings built from paths or task text.

## If unsure
STOP. Don't guess. Write the question under "Open questions" in `.task/REPORT.md` and finish.

## When done
- Do not commit; the runner commits for you.
- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max.
