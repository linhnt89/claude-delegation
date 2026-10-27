# S054 init installs the guard, upgrade adds it, and only the user can turn it off
- Job: J9
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer setting up a new project, and one upgrading a project made before the guard.

## Starting state
1. A repo with `.claude/settings.json` holding hooks and settings of their own.
2. A project set up with an earlier kit, without the guard.

## Steps
1. `delegate init` in the first; `delegate upgrade` in the second.
2. Start the lead with `DELEGATE_GUARD=off` in the environment and try an edit to
   `shop/pricing.py`.

## Passes when
- Both projects have the guard's hooks, and every hook and setting they had is unchanged.
- `init` and `upgrade` say they added the guard and how to turn it off.
- With `DELEGATE_GUARD=off`, the edit goes through; without it, it is refused.

## If this fails
Projects silently miss the guard, or setting it up breaks the user's own Claude Code setup.

## Failure paths
- A settings file that isn't valid JSON: left alone, with a message saying so.
- Running `init` or `upgrade` twice adds the hooks once.
