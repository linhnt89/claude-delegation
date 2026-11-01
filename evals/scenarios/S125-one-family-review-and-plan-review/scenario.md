# S125 With one family, review and plan review use another model of it
- Job: J3
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A user with only Claude accounts, `independence = "model"`, `planner_model = "opus"`. Decision 029.

## Starting state
t003 was implemented by `claude:sonnet` and its checks pass. The review tier lists
`claude:sonnet`, `claude:opus`; the plan-review tier lists `claude:opus`, `claude:haiku`.

## Steps
1. `delegate review t003`.
2. `delegate plan-review`.

## Passes when
- The review runs on `claude:opus` and says it fell back to another Claude model.
- The plan review runs on `claude:haiku`, not `claude:opus` (the planner's model), and says so.

## If this fails
A user with one family gets no review at all, so nothing they merge was checked by a second
model.

## Failure paths
- No `planner_model` set: a plan review that must fall back stops and says to set
  `planner_model` next to `planner_family`.
