# P021 A new task takes the first candidate whose runway, shared with its running tasks, covers it
- Job: J5
- Tier: fast
- Judged by: auto
- Status: agreed

## Who
Claude as lead starting tasks while others run. Decisions 014, 016, 017.

## Property
For any tier of candidates on 1–4 accounts with any quota left, any set of running tasks on
those accounts, and any task size, the new task runs on the first candidate, in tier order,
whose account runway divided by (its running tasks + 1) covers the size. No later candidate
is chosen while an earlier one qualifies, and none is chosen whose shared runway falls short
while another's covers it.

## If this fails
Parallel tasks either crowd an account that can't carry them all and stop partway, or skip
a healthy preferred account for no reason.
