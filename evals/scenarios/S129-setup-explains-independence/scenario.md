# S129 Setup tells the user when families are short
- Job: J1
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
A user setting up a new machine with `delegate setup`. Decision 029.

## Starting state
No `~/.config/delegate/workers.toml`. A second run has a workers.toml with only Claude and GPT
candidates and `independence = "family"` set by the user.

## Steps
1. `delegate setup`.
2. On the second machine, `delegate setup`.

## Passes when
- The first writes workers.toml with top-level `planner_model` beside `planner_family`, and
  `independence = "family"` under `[defaults]`, each with a comment saying what it does.
- The second says only two families have candidates, and how to set `independence = "model"`;
  it leaves the user's value unchanged.

## If this fails
Users who have fewer families find out only when work blocks.

## Failure paths
- A workers.toml with no `independence` key is not rewritten; setup prints the same hint.
