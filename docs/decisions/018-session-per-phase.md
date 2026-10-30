# 018: A lead session covers one phase and hands off through the repo

- Date: 2026-10-30
- Status: accepted

## Context
Every turn re-sends the session's whole context. A long session costs more each turn, and
after the prompt cache expires (the user away for a while) the first turn back pays for all of
it again. Auto-compaction is expensive and loses detail. Project starts are the worst case:
discussion, design and execution pile up in one session. Yet everything a lead needs already
lives in the repo (`PLAN.md`, specs, decisions) and the delegate home.

## Decision
- A lead session covers one phase: a discussion (ends in `/distill`), planning (scenarios,
  contracts, specs), or one wave (run, review, merge, close the loop).
- At the end of a phase the lead writes a handoff into `PLAN.md`: what's next and what's
  waiting for the user. Nothing needed later lives only in the conversation.
- The lead suggests `/clear` or a new session after the handoff: at the end of a wave, when its
  context has grown large, or before the user leaves for longer than the cache lasts.
- A new session opens with a short catch-up brief from `PLAN.md` (and an autopilot run report,
  decision 019): what was merged, what waits for the user, what's ready.
- Wide reading (surveys, research) goes to `--research` tasks or subagents, not the lead's context.

## Rejected options
- Long sessions kept alive by auto-compaction — costly, lossy, and still pay full price after
  the cache expires.

## Consequences
Sessions are cheap to throw away, so the handoff has to be complete; a fact only in the
transcript is lost. The user restarts sessions more often.
