# 006: Kit upgrade details found while building it

- Date: 2026-10-26
- Status: accepted

## Context
Building `delegate upgrade` (t020) to decision 005 turned up two points 005 got wrong or left open.

## Decision
- **The kit id and file hashes are computed at runtime**, once, from the templates embedded
  in the binary, not at build time. Decision 005's point stands: the id derives from the
  templates, so nobody bumps a version.
- **Deletions in a project without a record:** a kit path with no record entry that isn't on
  disk was deleted if the project's git knows it (it is in `HEAD`, or any commit touched it):
  `upgrade` reports it `missing` and doesn't restore it. Only a path git has never seen is
  `added`.

## Rejected options
- Hashing in `const` evaluation — every build warned "constant evaluation is taking a long
  time", and the `long_running_const_eval` lint would break the build as templates grow.
- Deciding deletions from which earlier kit the project matches — a recordless project with
  edits matches several kits, and hidden P012 caught `upgrade` restoring a deleted pane file.

## Consequences
Upgrade reads the project's git history for unrecorded paths. A project whose history was
squashed after a deletion gets that file back as `added`; it shows in the output and `git diff`.
