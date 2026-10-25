# Users and jobs

<!-- From the /evaluate interview with the project owner, 2026-10-25. Approved by them. -->

**Users:** Anyone who works the way the owner does: Claude Code leads on a git repo, and they
have other models (Codex, Pi, OpenCode, Gemini, ...) they want doing the implementation work.
Small personal repos and work codebases alike. Two kinds of user drive `delegate`: the person
(approves, decides) and Claude as lead (writes specs, runs and reviews tasks through the skills).

| Job | What "it worked" means to them |
|---|---|
| J1 Set up a project | One `delegate init` and I'm ready, with nothing overwritten. A missing tool (bwrap, a harness) is named clearly |
| J2 Hand off a ready task | Claude shows me the spec and the tier, then starts it without waiting for a yes. Later I get a commit whose checks passed, without watching the worker |
| J3 Trust what comes back | Merge refuses failing checks, missing reviews and high-severity findings. Review comes from a different model family. Workers can't touch my checkout, branches or hidden checks |
| J4 Keep going when a model fails | A model with no quota or broken auth is skipped for the next one. A failed round retries one tier stronger |
| J5 Run several tasks at once | Parallel tasks don't step on each other, and `delegate list` shows where each one stands |
| J6 Research and plan review | A read-only research run returns a report and changes no code. Plan review uses a model that isn't Claude and finds real problems before work starts |
| J7 The repo stays the shared memory | Decisions and scenarios are kept in the repo. After merge or discard, no task state is left behind and none ends up in git history |

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
- Open: a retry after failed checks gets a stronger model, but it isn't given the failing
  check output unless Claude writes it into the spec. Expected to fail its scenario today.

## Hidden
<!-- Ids only; content lives in <repo>.delegate/_evals/hidden/scenarios/. -->
One per job, chosen from scenarios that pass today, so fix tasks can still see what they fix:
J1 P003 · J2 S005 · J3 P002 · J4 P004 · J5 P008 · J6 S018 · J7 P006
