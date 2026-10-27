# S063 upgrade stops showing tasks twice in the terminal
- Job: J1
- Tier: fast
- Judged by: auto
- Status: automated

## Who
A developer whose project was set up by an older kit, which put `delegate statusline` in
`.claude/settings.json`, so the terminal shows the tasks twice (status line and the row).

## Starting state
Two projects set up by an older `delegate init`: in one the status line is still exactly the
kit's `delegate statusline`; in the other the user changed it to their own command that
calls `delegate statusline` among other things.

## Steps
1. `delegate upgrade` in each.

## Passes when
- In the first, the status line the kit added is gone; every other setting is unchanged.
- In the second, the user's status line and every other setting are unchanged.
- `upgrade` says what it removed, or why it left the status line alone, in one line.

## If this fails
Existing users keep seeing every task twice in the terminal, or lose a status line they made
themselves.

## Failure paths
- A settings file that isn't valid JSON: left alone, with a message saying so.

## Properties
- Covered by P013 (upgrading twice changes nothing).
