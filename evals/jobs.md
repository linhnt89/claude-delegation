# Users and jobs

<!-- From the /evaluate interview with the project owner, 2026-10-25. Approved by them. -->

**Users:** Anyone who works the way the owner does: Claude Code leads on a git repo, and they
have other models (Codex, Pi, OpenCode, Gemini, ...) they want doing the implementation work.
Small personal repos and work codebases alike. Two kinds of user drive `delegate`: the person
(approves, decides) and Claude as lead (writes specs, runs and reviews tasks through the skills).

| Job | What "it worked" means to them |
|---|---|
| J1 Set up a machine and a project, and keep them current | One `delegate setup` per machine and one `delegate init` per project, and I'm ready, with nothing overwritten. A missing tool (bwrap, a harness) is named clearly. When the kit improves, one `delegate upgrade` brings the kit's own files up to date and keeps my files and my edits |
| J2 Hand off a ready task | Claude shows me the spec and the tier, then starts it without waiting for a yes. Later I get a commit whose checks passed, without watching the worker |
| J3 Trust what comes back | Merge refuses failing checks, missing reviews and high-severity findings. Review comes from a different model family. Workers can't touch my checkout, branches or hidden checks |
| J4 Keep going when a model fails | A model with no quota or broken auth is skipped for the next one. A failed round retries one tier stronger |
| J5 Run several tasks at once | Parallel tasks don't step on each other, are told when their files overlap, share an account only while its quota can carry them all, and `delegate list` shows where each one stands |
| J6 Research and plan review | A read-only research run returns a report and changes no code. Plan review uses a model that isn't of the lead's family and finds real problems before work starts |
| J7 The repo stays the shared memory | Decisions and scenarios are kept in the repo. After merge or discard, no task state is left behind and none ends up in git history |
| J8 See what running tasks are doing | I start a run in the background and can see any time, from the terminal or Claude Code, each task's phase, what it is for, which session started it, its worker's steps as they happen, and whether it has stalled or died. Claude can check the same thing to decide what to do next |
| J9 Claude leads, workers build | Once a plan is agreed, Claude delegates product code and checks instead of writing them itself (a habit taught by the kit, not enforced since decision 030). Once a wave is agreed, Claude starts all its ready tasks together and keeps working while workers run |

## What would make them give up
- It claims success when it didn't succeed: a merge that went through with broken checks, or a commit that's empty.
- It loses or wrecks their work: a worker edits the main checkout, or a merge clobbers changes.
- A worker hangs or burns quota with nothing to show, and nobody notices.
- Error messages that don't say what to do.
- Babysitting: re-explaining tasks or fixing by hand so often that doing it themselves would be faster.
- Leftover clutter: stale worktrees, `task/*` branches and task folders after tasks end.

## Notes from the interview
- Claude delegates a ready task on its own: it announces the spec and tier and starts, no yes needed.
- The sandbox is a job requirement, not an extra: a worktree alone doesn't stop a worker
  editing the main checkout, moving branches or reading hidden checks.
- A retry after failed checks gets a stronger model, and the runner hands it the failing
  check output (decision 001). Fails today; planned as t003.

## Herdr observer evaluation (decision 035)
- User approved S137–S144 for J8 on 2026-11-04: model-neutral conversation attribution,
  task selection preserving the lead, retries, read-only lifecycle, reconnect, identity
  boundaries, task-id reuse, and untrusted activity.
- Their executable checks print human walkthroughs and exit 2. They are not automated
  product verdicts; an isolated real observer is still needed for validation.
- User approved derived properties P030–P033 on 2026-11-04. No property checks or hidden
  observer scenarios exist yet; production identity fallback and retention remain open.
- User approved S145 and proceeding through checks/investigation on 2026-11-04.
  It covers the lead-reproduced Codex shared-daemon wrong-pane identity report.
  Its opt-in live check is pending: t129's draft reproduced the failure in lead-run
  live tests but was discarded after safety/oracle review. No accepted automated
  check exists; exploratory evidence is not feature acceptance.
- User approved S146 on 2026-11-04: credential-free sibling using real Codex/Herdr,
  genuine SessionStart hooks and one shared daemon, with only model replies stubbed
  locally. Exploratory probes reproduce the wrong-pane report without credentials;
  pure oracle, preparation/stub and evidence readers are merged after independent
  review (193 infrastructure tests on main). No live scenario check or accepted
  automated verdict exists. S146 does not replace authenticated S145 or
  concurrent/resume/native acceptance.
- User approved decision 037: prioritize a bounded Pi/Claude native observer and
  defer Codex check/remedy work. Codex attribution is unsupported/unknown, never
  guessed. S137–S146/P030–P033 intent is unchanged; reduced-harness evidence must
  not be reported as full observer/property acceptance. Map native-slice checks
  explicitly during t124 scoping.
- User approved decision 038 and S147/S148 on 2026-11-04: `delegate new` records the
  Herdr lead's origin (Pi/Claude supported, Codex unsupported, workers get no `HERDR_*`),
  and a `delegate watch --beside` side pane puts only its lead's tasks in the sidebar,
  clearing them on a conversation change. Their approved failure paths became S149/S150
  (same wording) and are hidden. All four checks use the stand-in Herdr socket
  (`evals/lib/herdr_standin.py`, modelled on the lead's private probe). They fail until
  t139/t124 are built. The lead validated the S148/S150 checks against a throwaway
  contract-following fake, and against broken variants that the checks rejected.
- Decision 039 (user's choice after plan review): unreadable tasks are isolated, so hidden
  S150 now also expects `unavailable · <id>`, and makes its outage an unlistable task store.
- User approved S151 (t140) on 2026-11-04: `--beside` launch, keyboard inspection, inert
  task text, terminal restore, sidebar snippet. Its launch bullet was reworded to match
  Herdr 0.9.5 (a fixed typed command; paths go through the split's cwd and env).
  Checks never reach the lead's real Herdr: `common.sh` unsets every `HERDR_*`.

## Hidden
<!-- Ids only; content lives in <repo>.delegate/_evals/hidden/scenarios/. -->
Rebuilt 2026-10-26 after the repo moved and the first hidden set was lost. One or two per job,
chosen from scenarios that pass today, mostly properties and failure paths:
J1 P012 (new feature: fails until built) · J2 S007 · J3 P001 · J4 S014 · J5 S016 · J7 S021 ·
J8 P010 S028. J6 has none: its one scenario, S017, stays visible.
Added for decision 007 (new feature: fail until built): J7 P015 P016.
Added for session-scoped views (new feature: fails until built): J8 P018.
Added for decisions 016, 017 (new feature: fails until built): J5 P021.
Added for decision 020 (new feature: fails until built): J3 S094.
Retired by decision 030 (in git history, out of the vault): J9 P017 · J10 P022 S088 S116 P027 S127 · J3 P025 P026.
Added for decision 031 (new feature: fails until built): J6 P028.
Added for decisions 032, 033 (new feature: fails until built): J1 P029.
Added for decision 034 (new feature: fails until built): J1 S136.
Added for decision 038 (new feature: fails until built): J8 S149 S150.
