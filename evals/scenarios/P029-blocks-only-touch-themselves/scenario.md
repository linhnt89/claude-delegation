# P029 Upgrade changes only pristine kit blocks
- Job: J1
- Tier: full
- Judged by: auto
- Status: agreed

## Who
Anyone upgrading a project with managed blocks. From: S131, S132, S133.

## Property
For any `AGENTS.md`/`CLAUDE.md` content around the blocks, and any mix of pristine, edited,
deleted and newly added kit blocks: upgrade never changes a byte outside the blocks, replaces
exactly the pristine blocks with the kit's text, appends exactly the new ones, leaves edited
and deleted ones as they are, and a second upgrade changes nothing.

## How it is checked
Seeded random project text (headings, lists, code fences, text that looks like a marker but
isn't one) around the blocks, upgraded with a newer kit. Prints the seed and the smallest
failing case.

## If this fails
An upgrade can damage text the project wrote in a file every agent reads.
