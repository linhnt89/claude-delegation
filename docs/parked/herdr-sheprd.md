# Parked: Herdr/Sheprd support (decision 040)

Stopped 2026-11-08. Everything below is what a later session needs to pick it up again.
Code, scenarios, prototype and eval libraries: tag `archive/herdr-sheprd` (`954d3f3`).
Why it was built: decisions 035–039. Why it stopped: decision 040.

## How to resume
1. Write a decision that supersedes 040 and says which slice comes back.
2. Restore from the tag, e.g. `git checkout archive/herdr-sheprd -- delegate/src/origin.rs`,
   or revert the parking commit for everything. Hidden S149/S150 are in the vault's history
   (commit before "Retire S149 S150 ..."); add them back with `delegate hidden add`.
3. Re-probe Herdr first: every check and fact below was against private Herdr 0.9.5.
4. Re-run `scripts/check` and the restored scenarios before planning new work.

## What was built and merged (all at the tag)
| task | commit | what |
|---|---|---|
| t123 | e6e6ec5 | `prototypes/herdr-observer/`: opt-in headless attribution/status adapter spike |
| t139 | 673fb8e | `delegate new` records the Herdr lead origin and task instance (`origin.rs`). The worker `HERDR_*` strip from this task stays on `main` |
| t124 | a4145f5 | `watch --lead-pane`: lead's tasks as Herdr sidebar tokens, bounded fallible reader, signals (`herdr_view.rs`, `signal-hook` 0.4.6) |
| t140 | ca5fe04 | Interactive side pane, `watch --beside` launcher, `--sidebar-config`, skill text |
| t130/t132/t133 | 6039a94, 03a861a, 4924f26 | Codex S146 pure oracle, preparation/stub, evidence readers (`evals/lib/codex_identity_*`, `evals/tests/`) |

Scenarios at the tag: S137–S144 (human walkthroughs, exit 2), S145/S146 (Codex, no
accepted live checks), S147/S148/S151 (automated, passed at merge), hidden S149/S150.
Properties P030–P033 were approved but never written as checks.

## Verified state when parked
- S147, S148, S151 and hidden S149/S150 passed; 361 Rust tests at t124's merge.
- t140 isolated walkthrough with a private Herdr daemon and two real Pi leads: 17/17
  (origins, unfocused side pane, per-lead tokens, expand, `/new` and exit clearing rows,
  resume in another pane restoring rows, `q` clearing them).
- Never verified: a Claude lead in the walkthrough (no credentials in isolation), and how
  Sheprd itself renders the sidebar rows.

## Facts about Herdr 0.9.5 (from `docs/research/herdr-sidebar-tokens-probe.md`)
- Tokens survive a conversation change in the pane; any source can overwrite any key;
  values capped at 80 chars; bidi and U+2028 are not stripped; TTL works.
- Workers inherit `HERDR_*`, so a worker's hook can overwrite the lead pane's identity.
  That's why `main` still strips them.
- Ordinary Herdr RPC closes after one response. Unix socket paths need short private dirs.
- Codex 0.166.0 under a shared app-server daemon publishes B's identity on A's pane
  (reproduced with and without credentials). No product fix exists.

## Known gaps found before parking
From read-only surveys t142/t143 (2026-11-04, code reading only, nothing ran live):
- A failed frame write returns before the final clearing report (`herdr_view.rs:394-397`
  at the tag), so rows linger until the TTL; so does SIGKILL.
- Keys: SS3 arrows, PgUp/PgDn, Home/End are dropped. Width is read once, no rewrap on resize.
- `--interval` is accepted with `--beside` but unused (fixed 2 s).
- A known lead shows other leads' tasks only as counts, not browsable.
- A harness without an event adapter (Gemini) shows "No activity yet" in the pane instead
  of saying activity isn't available (`steps_available` never read).
- The side pane maps outcome `research` to `done`.

Still true on `main`, not Herdr-specific (moved to PLAN's known issues): research runs show
phase `implement` throughout, and decision 002 lists fewer phases/outcomes than the code.

## Open questions left
- Production identity: fallback-attempt identity, retention after merge/discard or lead
  closure, visibility across checkouts/workspaces/hosts (was t127, needed user design).
- Independent property checks P030–P033 (was t125).
- Does a Pi lead's `path` identity survive resume unchanged? (S142's resume step.)
- Side pane scope was one checkout (the lead's cwd).
- Alternative: herdr-app plus a delegate mod (native clickable rows), needing a
  herdr-core migration; see `docs/research/herdr-app-delegate-mod.md`.
- Codex remedy boundary: caller-scoped Codex context, TUI-local reporting or a Herdr
  adapter using real thread responses. No transport/schema approved.

## Parked Codex attribution wave (decision 037 had already paused it)
t138 trusted acquisition/owned-process handles · t131 S146 owner loop · t134 pinned patched
Codex build · t135 safe authenticated S145 · t136 concurrent/resume/mixed-version evaluation
· t137 patched-binary digest policy · t128 shared-daemon remedy. Specs and reviews:
`docs/task-specs/t13*-s146-*.md`, `docs/research/codex-*.md`.
Rules that still apply if resumed: never fix attribution by per-launch daemon restarts,
credential copying, per-pane login profiles, synthetic identities or unknown-as-success.
Prior one-off Opus reviews are consumed; GPT-authored check components block a GPT-only
remedy pool. Rejected diagnostics in `/var/tmp/ho-codex.OAzcs8/rejected-t129/` and
`/tmp/delegate-codex-deferred.pnZRzk/` (t138) are unsafe inputs, not patches.

## Local setup undone on 2026-11-08
The sidebar block appended to the user's Herdr `config.toml` was removed (the
pre-delegate backup is `config.toml.bak.pre-delegate`), and `~/.local/bin/delegate` was
rebuilt from the parked `main`. `delegate watch --sidebar-config` at the tag prints the block.
