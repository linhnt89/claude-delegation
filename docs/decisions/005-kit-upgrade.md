# 005: Upgrading a project's kit

- Date: 2026-10-26
- Status: accepted

## Context
`delegate init` never overwrites, so a project keeps the skills, pane mod and `evals/run` it was
set up with; the only way to get a newer kit was to delete files by hand and rerun `init`.
Projects don't record which kit they have, `Cargo.toml` has said `0.1.0` since the first kit,
and some installed files are the project's own (`AGENTS.md`, `PLAN.md`...) while others are
the kit's. Every existing project was set up before any of this was recorded. Scenarios:
S038–S042, P012–P014.

## Decision
- **A separate `delegate upgrade`.** `init` keeps its rule (S002): it never changes a file.
  `upgrade` changes files only in the working tree and never commits; its output lists what
  was updated, added, removed, kept and missing, and ends with "review with git diff, then
  commit". `delegate upgrade <path>...` takes the current kit's version of those paths,
  edited or not, and touches nothing else.
- **Who owns what.** Project-owned: `AGENTS.md`, `CLAUDE.md`, `PLAN.md`, `.delegate.toml`,
  `evals/jobs.md`; `.claude/settings.json` is the user's and `upgrade` doesn't touch it. Every
  other file `init` installs is the kit's. `upgrade` never changes a project-owned file: when
  its template changed since the project's kit, it shows that template diff to carry over by hand.
- **The record: `.delegate-kit.toml`, committed, at the repo root.** `kit` = the kit id; `[files]`
  = path → sha256 of what was installed (for project-owned files, of their template). `init`
  writes it, recording only files it added; `upgrade` updates it for each file it changes,
  and for each project-owned file whose template diff it showed (so the next run is quiet,
  and an upgraded project's record equals a fresh `init`'s, P014).
- **The kit id is a content hash**, the first 12 hex of the sha256 over the kit's sorted
  `path sha256` lines, computed at build time from the embedded templates. Nobody has to bump a
  version, and any template change is a new kit.
- **The binary knows every earlier kit.** `delegate/templates/kit-history.txt` lists each past
  kit's id and `path sha256` lines, plus the template contents of project-owned files (for the
  S040 diffs). `scripts/kit-history` regenerates it from git history; a unit test fails when
  the current kit is missing from it, so it can't go stale.
- **Pristine vs edited.** A kit file is pristine if its working-tree content (uncommitted
  changes count) matches the record, or, for a path without one, any content that path ever
  shipped with. Pristine files are updated; files dropped from the kit are removed if
  pristine. Edited files are kept, named, and given the `delegate upgrade <path>` and
  `git diff --no-index` commands. A recorded kit file that's gone is reported missing, not restored.
- **Projects without a record** (all of them today) are matched to the earliest past kit whose
  files they contain unchanged. If no kit matches, `upgrade` still updates pristine files, shows
  each project-owned file's diff against the current template, and says it couldn't tell what's new.
- **A kit id the binary doesn't know** means the project was upgraded by a newer `delegate`:
  `upgrade` changes nothing, exits non-zero, and gives the rebuild command.

## Rejected options
- `init` updating kit files too — breaks S002's "init never changes an existing file", the one
  guarantee that makes `init` safe to rerun.
- A semver kit version — `0.1.0` was never bumped across 11 kits; a forgotten bump would make
  a new kit look old and roll projects back.
- Three-way merging edited kit files — skills are prose; a silent merge is worse than a named
  conflict the lead resolves with one command.
- Keeping the record in the delegate home (`<repo>.delegate/`) — it's per-machine, not shared
  with the team, and it's lost when the repo moves (how the hidden set was lost).
- Shipping the skills as a Claude Code plugin — it would update the skills, but not
  `evals/run`, the templates, or projects whose other harnesses read the files from the repo.

## Consequences
Each template change needs `scripts/kit-history` rerun in the same commit, and the test
enforces that. Projects get a new committed file. Rerunning `init` on an old project records
only what it adds, so `upgrade` still treats the old files as unrecorded and matches them
against history.
