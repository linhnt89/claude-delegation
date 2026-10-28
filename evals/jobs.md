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
| J6 Research and plan review | A read-only research run returns a report and changes no code. Plan review uses a model that isn't Claude and finds real problems before work starts |
| J7 The repo stays the shared memory | Decisions and scenarios are kept in the repo. After merge or discard, no task state is left behind and none ends up in git history |
| J8 See what running tasks are doing | I start a run in the background and can see any time, from the terminal or Claude Code, each task's phase, what it is for, which session started it, its worker's steps as they happen, and whether it has stalled or died. Claude can check the same thing to decide what to do next |
| J9 Claude leads, workers build | Once a plan is agreed, Claude writes neither product code nor checks itself. Its attempts are refused with what to do instead, and workers are never slowed by it. Only I can turn that off. Once a wave is agreed, Claude starts all its ready tasks together and keeps working while workers run |

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

## Hidden
<!-- Ids only; content lives in <repo>.delegate/_evals/hidden/scenarios/. -->
Rebuilt 2026-10-26 after the repo moved and the first hidden set was lost. One or two per job,
chosen from scenarios that pass today, mostly properties and failure paths:
J1 P012 (new feature: fails until built) · J2 S007 · J3 P001 · J4 S014 · J5 S016 · J7 S021 ·
J8 P010 S028. J6 has none: its one scenario, S017, stays visible.
Added for decision 007 (new feature: fail until built): J7 P015 P016.
Added for session-scoped views (new feature: fails until built): J8 P018.
Added for the lead guard (new feature: fails until built): J9 P017.
