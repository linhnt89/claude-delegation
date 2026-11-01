# P027 A picked worker is never a model it must be independent of
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Any user, under either `independence` value. Decisions 023, 024, 028, 029.

## Property
For any workers.toml (random tiers, routes and efforts over several families) and any history of
`Worker:` trailers on a task's Acceptance checks: the worker `delegate run` (or `review`) picks
never has the model key of a model it must be independent of; it is of an excluded family only
when `independence = "model"` and no allowed family has a candidate; and every such pick is
reported by the run and recorded as `Independence: model` at merge. With `"family"` no excluded
family is ever picked.

## How it is checked
Seeded random configs and histories through `delegate run --dry-run` and the real merge on a
throwaway repo. Prints the seed and the smallest failing config.

## If this fails
The independence the user relies on is broken in some configuration they may have, without a
word.
