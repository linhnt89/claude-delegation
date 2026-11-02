# S133 A project from before managed blocks gets them on upgrade
- Job: J1
- Tier: full
- Judged by: auto
- Status: agreed

## Who
A developer in a project set up with the last kit before managed blocks, where the lead's
role is the `## Claude's role in this project` section of `CLAUDE.md`.

## Starting state
`AGENTS.md` filled in for the project; `CLAUDE.md` as installed, plus a project section after
the kit's. All committed.

## Steps
1. `delegate upgrade`.
2. `delegate upgrade` again.

## Passes when
- The kit's old section is gone from `CLAUDE.md`; the project's section and `@AGENTS.md`
  remain, byte for byte.
- `AGENTS.md` ends with the current `lead` block; everything before it is unchanged.
- The output says the lead's role moved into a managed block in `AGENTS.md`.
- The second upgrade changes nothing.

## If this fails
Older projects keep the Claude-only lead rules forever unless someone merges them by hand.

## Failure paths
- The project added its own bullet inside the kit's old section: it is adopted as above, and
  that bullet moves to just after the `lead` block in `AGENTS.md`; the output lists it.
- A line of the kit's old section was changed or removed: neither file changes, the template
  diff is shown, and the output says how to add the block by hand.
