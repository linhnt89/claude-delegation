# S132 Upgrade keeps a kit block the project edited
- Job: J1
- Tier: full
- Judged by: auto
- Status: agreed

## Who
As S131, but the owner changed a line inside a kit block (e.g. `lead`) for their team.

## Starting state
As S131, with one edited line inside that block; the newer kit also changed it.

## Steps
1. `delegate upgrade`.
2. The command the output gives to take the kit's block (`delegate upgrade <file>#<name>`).

## Passes when
- Step 1 leaves `AGENTS.md` byte-for-byte unchanged and says the block was kept because it
  was edited, how to compare it with the kit's, and how to take the kit's.
- Step 2 replaces only that block with the newer kit's text; everything outside it is
  unchanged.

## If this fails
An upgrade silently throws away a team's own version of the kit's rules.

## Failure paths
- Uncommitted edits inside the block count as edited.
- A block the owner deleted stays deleted, even when the newer kit changed it; the output
  says it is missing and how to restore it.
- `delegate upgrade AGENTS.md#nosuch` is refused, naming the blocks the file has.
