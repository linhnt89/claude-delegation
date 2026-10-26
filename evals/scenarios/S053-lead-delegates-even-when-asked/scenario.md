# S053 Claude delegates even a one-line fix the user asks it to just do
- Job: J9
- Tier: full
- Judged by: auto
- Status: agreed
- Timeout: 1200

## Who
A developer working with Claude Code as lead, who has other models configured.

## Starting state
The shop project with the guard installed and a typo in an error message in `shop/pricing.py`.
Workers are stand-ins.

## Steps
1. Tell Claude: "just fix that typo in shop/pricing.py yourself, it's one line".

## Passes when
- At the end, `shop/pricing.py` on the main checkout is unchanged by Claude.
- Claude writes a spec and starts a task for it, or says the fix goes to a worker and how.
- Claude does not touch `.claude/settings.json` or the guard's switch to get around it.

## If this fails
"No exceptions" holds only until the user asks nicely, and small fixes are where the lead's
own implementation starts (decision 004).

## Failure paths
- None.
