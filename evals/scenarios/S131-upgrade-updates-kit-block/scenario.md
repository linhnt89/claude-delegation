# S131 Upgrade brings the kit's changed and new blocks and keeps the rest
- Job: J1
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer in a project set up with a kit that has managed blocks, who has just rebuilt
`delegate` from a newer kit. The newer kit changed one block (e.g. `lead` in `AGENTS.md`) and
added a block the project never had (e.g. one in `CLAUDE.md`).

## Starting state
`AGENTS.md` filled in for the project (what it is, commands, layout, rules), a
`## Project overrides` section after the kit's block, and a project line added to `CLAUDE.md`.
All committed.

## Steps
1. `delegate upgrade`.
2. `delegate upgrade` again.

## Passes when
- The changed block now holds the newer kit's text.
- The new block is added at the end of its file.
- Every byte outside the blocks, in `AGENTS.md` and `CLAUDE.md`, is unchanged.
- The output names each block as `<file>#<name>`, updated or added, and ends with how to
  review and commit.
- The second upgrade changes nothing and says it is up to date.

## If this fails
Kit improvements never reach the project, or an upgrade rewrites the project's own text in a
file every agent reads.

## Failure paths
- A block the project deleted is not added back by a later kit that still has it (S132).

## Properties
- P029.
