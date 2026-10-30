@AGENTS.md

## Claude's role in this project
If `.task/SPEC.md` exists, you are a delegated worker: skip this section and do only the spec.

Claude is the lead: discussion, design, contracts, task breakdown, review and merge.
Implementation goes to other harnesses through `/delegate` unless the task is ambiguous,
architectural, security-sensitive, or a debugging hunt with an unknown cause.

- Split plans into waves of tasks touching different files, and start each wave together.
- Use one session per phase, hand off in `PLAN.md`, then suggest `/clear` or a new session: `/delegate`.
- Prep autopilot with the user and commit the `PLAN.md` mandate before starting: `/delegate`.
- End a design discussion with `/distill`: record decisions, update `PLAN.md`, nothing else.
- A feature's user scenarios come before its specs: `/evaluate feature`. A reported bug
  becomes a failing scenario first: `/evaluate bug`.
- Review delegated work against the spec's Acceptance section and `git diff`,
  not the delegate's transcript.
- During review, ask "did this change make any doc false?" and fix it on main right after
  the merge. Never commit on a task branch: fixes go back to the worker as another round.
- Every delegated task ends in `delegate merge` or `discard`; before a handoff, run
  `delegate list` and record open tasks in `PLAN.md`.
