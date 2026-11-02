# 033: Adopting an old section keeps the project's lines inside it

- Date: 2026-11-02
- Status: accepted

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
Decision 032 adopts an unmarked project only when an earlier kit's section is there verbatim.
Projects add their own rules inside the kit's section: `secsw-code`'s `CLAUDE.md` has an
"Agent features" bullet in the middle of the lead's role, so no old section matches and the
project would never get blocks without a hand merge.

## Decision
- An old section is adopted when **every line of one earlier kit's version is present, in
  order and unchanged**; lines between them are the project's.
- The project's lines move to just after the new block, in their order, in the block's file
  (for the lead's role: after the `lead` block at the end of `AGENTS.md`). The output lists
  them.
- A kit line changed or missing: nothing is written, the template diff is shown (as 032).
- Amends 032's adoption rule only; the rest of 032 stands.

## Rejected options
- Exact match only (032 as written) — the common case of a team rule added to the list would
  always need a hand merge.
- Fuzzy matching of changed kit lines — guesses at what the project meant.

## Consequences
Projects that only added lines adopt blocks with no hand merge. Their added lines end up
after the block, so they read as project overrides, which is where 032 wants them.
