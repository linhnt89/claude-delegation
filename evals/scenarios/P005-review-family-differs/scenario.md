# P005 Reviews never come from the family that did the work
- Job: J3
- Tier: fast
- Judged by: auto
- Status: automated

## Who
Anyone using `delegate`, directly or through Claude as lead. From: S010, S018.

## Property
For any worker config and any implementer, `review` uses a model of a different family, and `plan-review` never uses the planner's family (Claude); when none is configured it refuses with a clear message.

## How it is checked
Random `workers.toml` files mixing families, read through `review --dry-run` and `plan-review --dry-run`. Seeded; prints the seed and the smallest failing case.

## If this fails
The second opinion the user relies on comes from the same kind of model.
