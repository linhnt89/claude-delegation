# S130 A lead that isn't Claude gets its plan reviewed by another family
- Job: J6
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer whose Claude quota is out, leading from Pi with GPT-6.1 Sol. Their machine's
`workers.toml` still says `planner_family = "claude"`.

## Starting state
A project with a spec ready; `plan_review` candidates are, in order, a Sol model, Claude Opus
and Claude Opus through Copilot.

## Steps
1. The lead runs `delegate plan-review --planner pi:openai-codex/gpt-6.1-sol@high --dry-run`.
2. Then `delegate plan-review --planner gpt --dry-run`.
3. Then `delegate plan-review --dry-run`, without the flag.

## Passes when
- Steps 1 and 2 say they skip the gpt family and list only the two Claude candidates.
- Step 3 skips the claude family as before and lists only the Sol candidate.

## If this fails
A plan written by Sol is reviewed by Sol, so the second opinion the user relies on is the
same model's, or the machine setting stops working for a Claude lead.

## Failure paths
- `--planner gpt` with only GPT candidates refuses, names the gpt family and says how to add
  a reviewer of another family.
- `--planner` with no value is refused by the CLI, naming `--planner` and pointing to usage or `--help`.

## Properties
- P028.
