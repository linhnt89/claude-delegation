# 017: Parallel tasks share an account while its runway covers them all

- Date: 2026-10-28
- Status: accepted

## Context
Decision 016 said an account with no running task is preferred over a busy one in the same
tier. The user wants the tier's order kept: a healthy first account should carry a whole
batch, and other accounts are fallbacks, not load to balance.

## Decision
This replaces 016's "Quota spreads across accounts" point; the rest of 016 stands.
- When picking a candidate, an account's runway is divided by (tasks running on it + 1).
- Candidates are tried in tier order as before: the first whose shared runway covers the
  task's `size:` runs. A busy account is not skipped while its shared runway is enough.

## Rejected options
- Preferring idle accounts (016) — spends fallback subscriptions early while the preferred
  one has quota to spare.

## Consequences
A batch stays on one subscription until its runway gets short, then moves down the tier.
