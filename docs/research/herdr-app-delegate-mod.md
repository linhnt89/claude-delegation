# Delegated tasks in herdr-app: feasibility (2026-11-04)

Research only, not a decision. Read from source and docs; nothing was built or run.
Revisit as a separate decision that amends 035 (which kept herdr-app/herdr-core out of
scope). The current route is decision 038/039: Herdr 0.9.5 + Sheprd, sidebar tokens and
`delegate watch --lead-pane` (t124 merged, t140 in progress).

## Sources
- `herdrdev/herdr-app` `93b097f` (release 0.1.13, 2026-11-04): `README.md`, `docs/mods.md`,
  `docs/security.md`, `defaults/AGENTS.md` (the mod guide), `defaults/herdr.d.ts`,
  `defaults/mods/agents/main.js`, `defaults/layouts/main.json`
- `herdrdev/herdr-core` `3965a46` (2026-11-04): `docs/commands.md`, `docs/templates.md`,
  `crates/herdr-core/src/{terminals,status,SKILL}.rs|md`, `client.rs`

## What herdr-app offers
- **Mods are trusted JavaScript drawing native GPUI, not sandboxed.** `exec(cmd, args,
  {cwd, on})` runs programs in the user's environment (also on remote machines).
  There are also `readTextFile`, `watchFolder`, `fetch`, `notify` and shared state.
  A mod can run `delegate status --json` itself.
- **Mod panes:** `pane.split {direction, mod, props}` puts a mod beside a terminal, saved
  with the tab. Layouts place mods anywhere, including sidebar blocks. Overlays and
  windows are also available.
- **Agents sidebar** is a shipped JS mod (461 lines). It nests sub-agents under their
  `parent` terminal and shows status, attention ("done" until seen) and notifications.

## What herdr-core offers (and lacks)
- Terminal record: `address` ("this terminal forever on this core", never reused),
  `parent` (set once at start), `labels` (`hc label t3 k=v`), `argv`, `created_ms`,
  `status`/`records` from OSC 7501, `turn`.
- Programs see `HC_TERMINAL_ID` (their address) and `HC_SESSION`.
- **No conversation identity.** OSC 7501 carries a status word, app, title and message,
  not a session id. Herdr 0.9.5's `agent_session` has no equivalent.
- `hc run --parent <t> --no-notify --outlive-parent --label k=v -- <cmd>` starts a
  child terminal of a lead.

## Likely design
- A personal "delegate" mod in a sidebar block lists tasks under their lead's terminal:
  unlimited clickable rows, coloured by state.
- Clicking a task splits a native task-detail mod pane beside the lead (decision 035's
  "lead left, task right"). Blocked or failed tasks can raise `notify`.
- The mod reads delegate through a JSON command exposing t124's bounded fallible reader
  (`read_tasks`), not by parsing task files itself.
- Origin capture (t139) gains a herdr-core path: record `HC_TERMINAL_ID` (+ machine),
  plus `CLAUDE_CODE_SESSION_ID` when present.
- Alternative considered: one read-only `delegate watch <id>` viewer terminal per task,
  started with `hc run --parent <lead> --no-notify`. Tasks would appear natively as
  sub-agents, but it leaves long-lived processes to clean up on merge or discard. The
  pure mod is cleaner. Neither changes worker execution.

## Comparison
| | Herdr + Sheprd (038/039) | herdr-app + delegate mod |
|---|---|---|
| Sidebar | Up to 4 text rows per lead (pane tokens) | Unlimited native, clickable rows |
| Task details | Terminal side pane | Native mod pane beside the lead |
| Click a task | Focuses the lead | Opens that task |
| Resume in another pane/terminal keeps tasks | Yes (`agent_session`) | No (terminal address only) |
| App changes | None | None (a personal mod) |
| Backend | Herdr daemon | Migrate to herdr-core |

## Open problems before adopting
1. **Resume (S142):** tasks follow the terminal address. A conversation resumed in a new
   terminal loses them, and nothing tells a mod which conversation is in a terminal.
   This needs herdr-core to report conversation ids, or an explicit acceptance that
   tasks follow the terminal.
2. **Migration:** a new terminal/session backend and app, not just a UI. Linux needs a
   working Vulkan driver. Telemetry is on by default (Settings → Privacy).
3. **Trust:** a mod has the user's full access. That's fine for our own mod, but nothing
   contains it.
4. **API churn:** 0.1.x with near-daily releases. The mod API is defined by type files
   rewritten on every start. That's cheaper than a Rust patch, but not free.
5. Scenarios S137–S151 would need herdr-app variants: a stand-in for `hc ls --json`
   and the mod host, not the Herdr socket.
