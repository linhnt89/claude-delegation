# S006 Claude delegates a ready task on its own and says the tier
- Job: J2
- Tier: full
- Judged by: auto
- Status: automated
- Timeout: 1200

## Who
A developer working with Claude Code as lead, who has other models configured.

## Starting state
A project with `PLAN.md` holding a clear, ready implementation task and an agreed scenario for it. Claude Code is the lead.

## Steps
1. Ask Claude to get on with the plan, without saying `/delegate`.

## Passes when
- Claude writes the spec with acceptance commands, including `evals/run <id>`.
- It tells the user the task and tier, like `t003: implement @ standard`, and starts the run without waiting for a yes.
- It does not implement the task itself.

## If this fails
Claude does the implementation itself, so the user's other models sit unused, or it runs work without saying which model tier.

## Failure paths
- An ambiguous or architectural task: Claude keeps it and says why instead of delegating.

## Properties
- None.
