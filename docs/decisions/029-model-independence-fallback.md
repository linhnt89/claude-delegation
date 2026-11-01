# 029: Another model of the same family when no other family is available

- Date: 2026-11-01
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Every independence rule is by model family: review and plan review skip the implementer's or
planner's family; implementers skip every family that wrote their checks (023); check fixes
come only from the writers' families (024). A user often has only Claude and GPT, or one
family. Then review and plan review can't run, and since each fix adds a writer family, checks
soon have every available family as writer and nothing can implement the tasks they judge.
t086 already skipped Claude and DeepSeek, and t099 and t102 needed hand-made exceptions to 024
while Hyper was out.

## Decision
- **Per-machine setting:** `[defaults] independence = "family"` (default) or `"model"` in
  `~/.config/delegate/workers.toml`. Which accounts exist is a fact of the machine, so
  `delegate setup` asks.
- **Family first.** A candidate from an allowed family is always picked when the tier has one.
- **Model fallback.** With `"model"`, when the family rules leave no candidate, a candidate of
  an excluded family may run if its model differs from every model it must be independent of
  (the checks' writers, the implementer, the planner). If none is left, nothing starts and the
  message names what's missing, as today.
- **Check fixes.** With `"model"`, when the writers' families have no candidate, a
  `fixes_checks:` task may use any available family, with a warning.
- **Recorded.** A merge that used a fallback carries an `Independence: model` trailer; `list`
  and the review show it.
- **Hidden scenarios required.** A task that used a fallback merges only after the hidden
  scenarios of its jobs pass at review.

## Rejected options
- Blocking, as today — with one or two families it stops delegation altogether.
- Falling back silently — weaker separation would be invisible in the history.
- A per-run flag — lost on retries and in autopilot, which must run unattended.

## Consequences
Same-family blind spots can come back where the fallback is used; hidden scenarios, which never
enter a worker's sandbox, and the trailer bound and expose that. Amends 023, 024 and the review
and plan-review family rules. t106 writes its scenarios, t107 implements it.
