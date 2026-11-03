# Private live identity probe (2026-11-04)

Lead-run feasibility evidence, not an eval pass or production integration.

## Isolation and method
- Installed Herdr 0.9.5 launched as a child `herdr server` with unique temporary
  HOME, XDG config/data/cache/state roots, private config/API socket and disposable repo.
- `herdr integration install pi|claude|codex` wrote only those private roots.
  No personal agent configuration, credentials or Herdr session was used or changed.
- Separate private workspaces ran actual installed harness binaries under `timeout 10`;
  snapshots polled `PaneInfo.agent_session`. The daemon child was terminated and reaped
  in a finally block. No global server-stop or process-name kill was used.
- Pi TUI was started idle with an explicitly unused placeholder API key and an explicit
  model, without submitting a prompt. Claude used a single print-mode startup prompt
  without credentials. Codex was tried both exec and interactive without credentials.
- Outputs and temporary session paths remain private/untracked; no actual session ids or
  terminal transcripts are committed. This document records only source/kind outcomes.

## Observed
| Harness/mode | Reported source / agent / kind | Scope of evidence |
| --- | --- | --- |
| Pi TUI | herdr:pi / pi / path | Actual startup via the installed integration; non-Claude identity works |
| Pi RPC | none | Expected: pinned Pi integration explicitly gates on ctx.mode == tui |
| Claude print startup | herdr:claude / claude / id | Actual SessionStart hook works before authenticated model completion |
| Codex exec / interactive | none | Credential-free probe did not establish identity; NOT proof of lack of support |

Codex exec created a thread but encountered authentication errors; interactive startup
also provided no observed identity within the probe. Do not label this passing Codex
coverage, substitute a synthetic id, or silently use its pane id as conversation identity.

## Consequence
Proceed with t123's explicitly unknown-safe headless adapter: two real harness sources
are feasible. Before native t124 scope is finalized, use the delivered adapter against
private live Pi/Claude leads and record Codex coverage as unresolved unless a safe,
authenticated isolated probe becomes available. Requiring personal credentials or
choosing a production identity fallback is a user/design decision, not a worker workaround.

Full S137 requires all named harnesses; this probe does not pass it. The temporary probe
script was exploratory, not a checked-in automated scenario. Reproducible opt-in live
commands belong to t123's deliverable and must not be confused with automatic checks.

## Lead-run adapter gate after implementation

The final reviewed t123 executable was exercised against another unique private live
Herdr 0.9.5 daemon, disposable git checkout, HOME/XDG roots and private integrations.
No personal credentials or configuration were used. Raw output remains private.

- Real Pi TUI produced a path reference and real Claude startup produced an id reference.
  `observer capture` read both through the live UNIX API; `create` persisted those origins.
- Wrapped stand-in research runs completed for both tasks. One successful snapshot showed
  exactly two live tasks with the respective captured source-qualified origins.
- A wrapped Pi-task retry preserved both task-instance tokens; a new snapshot remained
  consistent. Wrapped discards followed by a successful snapshot produced an empty task set.
- Codex interactive startup again supplied no observed identity in the credential-free
  polling window. It remains unknown/unverified, not an accepted Codex integration.
- Only the owned daemon child was terminated and reaped. Wrapped task state was discarded.
  Private directories/logs were retained temporarily for inspection; none are committed.

This establishes actual adapter transport plus Pi/Claude attribution, wrapped retry and
retirement. It does not establish native sidebar/rendering, worker model completion,
Codex integration, per-fallback attempt markers, hidden observer properties, or a pass
for the full agreed S137–S144/P030–P033 set.
