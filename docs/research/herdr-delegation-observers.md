# Herdr delegation observers: feasibility research

Research only; no approved implementation contract or product changes.

## Target experience

- Active lead conversation on the left, read-only delegated-task dashboard on the right.
- Sidebar nests delegated tasks under the lead conversation that created them.
- Dashboard groups all visible tasks by launching lead conversation; expands task activity.
- Different lead/worker harnesses work identically. A task stays one entry across retries.
- Observing, collapsing, or closing a view does not control a worker.

## Sources and versions

Inspected installed commands and public source, cloned read-only under
`/tmp/delegate-herdr-research/` (not project dependencies):

| Component | Inspected source |
| --- | --- |
| delegate | This repository, `eda91e3` |
| Herdr daemon/TUI | [herdrdev/herdr](https://github.com/herdrdev/herdr), `v0.9.5`, `2ecf19a92fc3a7fff75b6ce3655ef04f4625ec01` |
| Installed desktop client's source | [penso/herdr-gpui](https://github.com/penso/herdr-gpui), `v20261103.1`, `063e7e1533d0a8e52d1df0f6e2774ba5c1cc8c7a` |
| Separate desktop project, comparison only | [herdrdev/herdr-app](https://github.com/herdrdev/herdr-app), `a8c25ce239c057208fdb1caec6eda7d4581abd9f` |
| Separate backend, comparison only | [herdrdev/herdr-core](https://github.com/herdrdev/herdr-core), `ed25eee49ce617d48bac434b8915e3c003dafa72` |

The installed programs report `herdr 0.9.5` and GPUI `20261103.1`.
GPUI reports `browser_tabs: false` on this Linux machine. Native GPUI widgets and
terminal splits do not require embedded browser support.

Important correction to the exploratory discussion: `herdrdev/herdr-app` is not
this machine's GPUI client. Its features must not be attributed to `penso/herdr-gpui`.
The two projects have different backends and extension systems.

Documentation entry points: [Herdr](https://herdr.dev/llms.txt),
[Herdr UI](https://herdrui.dev/llms.txt). The latter identifies the correct desktop repository.

## Findings in delegate

1. **A lead-ownership mechanism exists but is Claude-specific.**
   `delegate/src/commands.rs:claim_owner` reads only `CLAUDE_CODE_SESSION_ID`.
   It assigns an unowned task on creation or first run and does not move an existing owner.
   `delegate/src/task.rs:Owner` contains `session` and `dir`.
   `delegate/src/status.rs:TaskView` exposes `session` and `session_name`; the latter is
   derived from the checkout directory, not a conversation title.
   Model-neutral lead instructions do not yet imply model-neutral runtime attribution.

2. **Useful read-only data is already public.**
   `delegate status [id] --json` exposes title, goal, owner, phase/outcome, worker,
   tier, activity time, stall state, changed files, steps, and merge readiness.
   `delegate/src/watch.rs` already renders refreshing all-task or one-task views.
   `status.rs:collect` returns only five recent steps in an all-task query; an individual
   query returns the collected task steps. Full-depth queries also compute diffs.
   A dashboard should not repeatedly request expensive full detail for every task.

3. **Activity is a summary, not a full output transcript.**
   `delegate/src/events.rs:Adapter` emits read/edit/run steps with
   `{id, at, kind, target, result}`. Command completion updates an existing step by id.
   Detailed command stdout is not part of this normalized contract. `stream.jsonl`
   contains harness-specific raw stdout; `run.log` includes plain output and stderr.
   The mockup's per-command output lines therefore need additional extraction or a
   separately labelled log view; they cannot be promised from normalized steps alone.

4. **Full round history and permanent completed history do not exist in this feed.**
   `events.rs:reset` truncates event/raw stream files at a new run, and `Stream::new`
   truncates them for a new worker attempt. Step ids restart with a new adapter.
   `task.rs:State.rounds` preserves round metadata, not all previous activity streams.
   Merge/discard removes task state (`commands.rs:cleanup`). A viewer must handle reset,
   disappearance, and id reuse; keeping old detail requires an explicit retention policy.

## Findings in Herdr 0.9.5

- Existing CLI/API supports splitting and running an observer beside the lead, without focus.
- Pane processes inherit `HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`,
  `HERDR_SOCKET_PATH`, and `HERDR_BIN_PATH`. These locate the UI context, not a durable
  identity for a particular conversation occupying that pane.
- `src/api/schema/agents.rs:AgentSessionInfo` exposes source, agent, reference kind,
  and session value. `panes.rs:PaneInfo` exposes that information through API snapshots.
  It can help attribute non-Claude leads when their integrations report identity.
  Identity may be absent; a defined fallback is necessary.
- `pane.report_metadata` supports display titles, labels, and tokens. It is not a
  delegated-task registry, and pane lifecycle reporting must not replace the lead's authority.
- `src/protocol/wire.rs:ClientShellAgent` and `src/server/client_shell.rs` define/build
  the desktop-facing agent projection. It does not carry `agent_session` or task-parent
  relationships, even though the public socket API exposes agent session information.
- Snapshot/event subscription primitives already exist. Documentation prescribes
  subscribe-before-reconcile and authoritative rereads after reconnect; events are
  invalidations, not a guaranteed replay log.

## Findings in the actual GPUI client

Paths below are relative to `penso/herdr-gpui` at the pinned release.

- `crates/herdr-gpui/src/sidebar/render/nested_agents.rs` already renders agents nested
  under their **workspace**, not under a parent agent/conversation.
- `sidebar/render/agent_rows.rs`, `sidebar/agents.rs`, and `sidebar/render.rs` are the
  principal sidebar integration points. Task rows need their own identity and click action;
  existing agent rows generally navigate to a pane.
- `crates/herdr-protocol/src/wire.rs:ClientShellAgent` mirrors the daemon projection.
  Changes to that projection require coordinated daemon/client protocol work and defaults
  or version gating for older peers.
- `crates/herdr-gpui/src/chat/existing.rs` already queries public snapshot information
  containing `agent_session`, independently of the shell projection, for existing
  conversations. Session lookup does not necessarily require changing the wire projection.
- `crates/herdr-gpui/src/mods.rs` and `mods/panel.rs` already provide a right-hand mod panel.
  Native panel placement is therefore not the hard part.
- `crates/herdr-mods/src/state.rs` exposes only hosts, workspaces, agents, focus, usage,
  and pull request topics. Agent values lack session identity and task-parent relationships.
  Sandbox mods cannot read files, spawn processes, or access sockets. A custom Luau mod
  alone cannot implement this integration today; the trusted host must supply task data.
- Native Claude/Codex sub-agent transcript activities in `herdr-acp` are not delegate
  tasks. Reusing their appearance must not confuse their identities or ownership.

## Separate upstream work worth borrowing, not assuming

`herdrdev/herdr-app/defaults/mods/agents/main.js` already implements a parent tree
using terminal `parent` links. `src/window.rs:root_agent` walks parent chains.
Its ordinary sub-agent selection uses `terminal.peek`, which can replace the shown
terminal; that differs from our requirement to keep the lead visible.

`herdr-core` supports `hc run --parent`, `--no-notify`, and `--outlive-parent`.
Core tests in `crates/herdr-core/tests/m6.rs` cover parent relationships and lifetime.
These are useful design references, not APIs available in the installed stack.
Do not replace delegate's worker execution with `hc run` just to obtain a tree:
that changes process lifetime, notifications, sandboxing, and ownership.

## Recommended integration boundary (proposal)

```text
Lead conversation in Herdr
        |
        | capture launching conversation identity
        v
delegate task state + normalized activity
        |
        | host-side, versioned, read-only adapter
        v
GPUI task model
    |                       |
    v                       v
sidebar lead/task tree      right grouped dashboard
```

- Keep task lifecycle, checks, retries, and worker sandboxing entirely in delegate.
- Keep tasks distinct from real terminal/agent sessions. A task observer need not have
  a fake agent process or one terminal pane per task.
- Establish stable origin identity when the task is created/first claimed; retain it
  when resumed from another pane. Distinguish host/backend namespace, checkout id,
  conversation identity, task instance, and worker attempt. Pane ids alone are insufficient.
- Preserve legacy owner compatibility and support tasks created outside Herdr in an
  explicit unassigned/by-hand group. Do not guess ownership by directory.
- For a first prototype, the trusted GPUI host can query a read-only delegate CLI feed
  per registered checkout and cache selected-task detail. Use existing public Herdr
  snapshot identity where available. This can avoid initial Herdr protocol changes.
- For durable discovery, reconnect, and TUI/GUI parity, consider a generic external-task
  registration/read model in Herdr. That is a proposed extension, not an existing API.
  Agree whether this belongs in Herdr or solely in GPUI before fixing schemas.
- A native GPUI task module is the most direct route to the exact UI. A Luau dashboard
  becomes an option after adding a bounded read-only task topic to the mod host.

## Safety and lifecycle requirements

- Bridge runs outside worker sandboxes. Never give a worker a new Herdr control socket
  or widen task-home access to make the viewer work.
- Worker processes inherit environment today; `harness.rs:sandbox` does not explicitly
  clear Herdr variables. Investigate inherited hooks/socket access so a worker cannot
  accidentally report itself as the lead. This is a review requirement, not an exploit
  demonstrated by this research. Removing environment hints alone is not a security boundary.
- No shell interpolation of task titles, no rendering terminal escapes as controls, and
  no raw stream/log exposure by default. Bound output and treat worker content as untrusted.
- UI failure/restart must not interrupt work; task selection sends no worker input.
- Do not interpret terminal `done/idle` as delegate success: retain delegate's phases and
  outcomes, including checking, blocked, interrupted, ready-to-merge, and discarded.
- Detect attempt/file resets and reject stale updates. Collapse/close only changes visibility.
- Decide history policy after merge/discard and lead closure; avoid an unbounded shadow archive.

## Decisions needed before specs

1. Target the installed `herdr` + `penso/herdr-gpui` stack (recommended for this request),
   or deliberately migrate to the separate `herdr-core` + `herdr-app` stack?
2. Group all sessions in this checkout, all workspaces on the selected host, or all hosts?
3. Require real harness conversation IDs, or support an explicitly provisional identity
   when an integration cannot report one? Define new-conversation and resume behavior.
4. Keep completed tasks only until merge/discard, retain compact summaries, or archive logs?
5. First deliver normalized read/edit/run activity, or include bounded command output too?
6. Should closing a lead retain an archived parent group for its still-running tasks?

## Validation needed before claiming implementation readiness

No production panes or settings were changed; no source was built or end-to-end prototype run.
Code-level feasibility is established, not runtime acceptance.

Before implementation, define user scenarios for: two leads in the same checkout; Pi,
Claude and Codex ownership; retries/model handoffs retaining one task row; conversation
replacement in the same pane; reconnect; observer closure without cancellation;
merge/discard removal; missing identity; and malformed/oversized output.

Then run an isolated proof of concept using synthetic task data and a separate Herdr
session: parent attribution, sidebar task selection while preserving the lead, and a
right-hand activity view. Validate protocol compatibility separately if extending Herdr.

## Doc conflicts / corrections

- The earlier discussion described full round activity and completed-task inspection as
  if available from existing records. Current streams are attempt-local, and merge/discard
  deletes task state; these are proposed features requiring retention decisions.
- Kit instructions support non-Claude leads, but runtime owner attribution remains
  Claude-specific. This is a capability gap, not evidence that all lead harnesses already
  produce usable session grouping.
- `herdrdev/herdr-app` is a separate project, not the current installed GPUI client's source.
