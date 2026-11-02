# 032: Managed blocks in project-owned markdown

- Date: 2026-11-02
- Status: accepted; adoption amended by 033

<!-- Append-only. Never edit an accepted decision's substance: write a new one that
     supersedes it, and update only this Status line. Keep it under a page. -->

## Context
`delegate upgrade` never writes project-owned files (`AGENTS.md`, `CLAUDE.md`, `PLAN.md`,
`.delegate.toml`, `evals/jobs.md`): it prints the template diff once, then records it as
seen. The lead's rules live in those files and change often (waves, session per phase,
decision 031), so every change needs a hand merge in every project, and an ignored diff is
lost. Decision 031 moves the lead's role into `AGENTS.md`, another such change.

## Decision
- **Managed blocks.** Kit text inside an owned markdown file sits between
  `<!-- delegate:begin <name> -->` and `<!-- delegate:end <name> -->`. Upgrade replaces a block
  whose content matches what an earlier kit installed; keeps an edited block and reports it
  with its diff and `delegate upgrade <file>#<name>` to take the kit's version; leaves a
  deleted block deleted; never changes a byte outside the blocks.
- **Where:** `AGENTS.md` and `CLAUDE.md` only. The first block is `lead` in `AGENTS.md`: the
  lead's role (031). Each block says to put project overrides after it, not inside.
- **Adoption of older projects.** For each block, the kit records which text of each earlier
  kit's templates it replaces. When an unmarked file holds one of those texts verbatim,
  upgrade swaps it for the marked block; a block that moves file (the lead's role, from
  `CLAUDE.md` to the end of `AGENTS.md`) moves only when its old text matches, both files or
  neither. No exact match: nothing is written and the template diff is shown, as today.
- **Other files stay as they are:** `PLAN.md`, `evals/jobs.md` and `.delegate.toml` are the
  project's after `init`; upgrade never writes them. A missing config key always means its
  default; upgrade may mention a new key, never writes it.

## Rejected options
- Three-way merge of whole files (`git merge-file`) — conflicts wherever the project edited
  near a template change, so results depend on luck, and conflict markers would reach a file
  every agent reads.
- Locked blocks that upgrade restores over user edits — loses the user's work (J1).
- A kit-owned `LEAD.md` beside `AGENTS.md` — blocks keep one file per audience.
- Writing new keys into `.delegate.toml` — needs `toml_edit` to keep comments, and missing
  keys already mean defaults.

## Consequences
Kit text in `AGENTS.md`/`CLAUDE.md` reaches projects through a normal upgrade. Amends the
"project-owned templates are advice, never write targets" rule in `upgrade.rs`; S040 and
S082 change with it. Projects whose old sections were edited still merge once by hand.
