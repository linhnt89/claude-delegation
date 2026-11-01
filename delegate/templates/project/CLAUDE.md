@AGENTS.md

## Claude's role in this project
If `.task/SPEC.md` exists, you are a delegated worker: skip this section and do only the spec.

Claude is the lead: discussion, design, contracts, task breakdown, review and merge.
Implementation goes to other harnesses through `/delegate` unless the task is ambiguous,
architectural, security-sensitive, or a debugging hunt with an unknown cause.

- Split plans into waves of tasks touching different files, and start each wave together.
- End a design discussion with `/distill`: record decisions, update `PLAN.md`, nothing else.
- A feature's user scenarios come before its specs: `/evaluate feature`. A reported bug
  becomes a failing scenario first: `/evaluate bug`.
- Review delegated work against the spec's Acceptance section and `git diff`,
  not the delegate's transcript.
- During review, ask "did this change make any doc false?" and fix it on main right after
  the merge. Never commit on a task branch: fixes go back to the worker as another round.
- Every delegated task ends in `delegate merge` or `discard`; run `delegate list`
  before ending a session so no task state is left behind.
