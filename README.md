# Multi-model cooperation kit

Claude leads: discussion, design, plans, specs, decisions. Worker models do the
implementation loops, checks and reviews, picked by **role × tier** from one config file.
The repo is the only memory they share.

```
 discuss with Claude ──/distill──▶ docs/decisions/, PLAN.md
                                        │
                         delegate new <id>  → Claude writes specs
                                        │
                         delegate plan-review   (other family than Claude; for batches/deep)
                                        │
                         delegate run <id> [--tier]
                           ├─ question phase: READY, or questions → answer in spec → run again
                           ├─ work phase (same session), runner commits
                           └─ runner runs the spec's acceptance commands  ── fail → next run steps up a tier
                                        │
                         delegate review <id>   (other family than the implementer; when risky)
                                        │
                         Claude decides ──┬── merge   (gates: checks, project rules, review)
                                          ├── another round
                                          └── discard
```

## Install
`delegate` is one Rust binary (Linux only for now; the sandbox uses bwrap). Build it once per
machine and put it on your PATH:
```bash
cd delegate && mise exec -- cargo build --release && cp target/release/delegate ~/.local/bin/
```
Then, in each project (a git repo):
```bash
delegate init
```
`init` adds the project files (templates are built into the binary) and creates
`~/.config/delegate/workers.toml` if missing; it never overwrites. It also sets up Claude
Code's status line (unless you have your own) and the `/delegate-pane` side pane, which show
running tasks live. Then fill in `AGENTS.md`
(commands first) and any `.delegate.toml` rules.

Runtime needs: `git`, `bwrap`, and the harnesses you configure. Nothing else.

## Commands
| Command | What it does |
|---|---|
| `delegate init [dir]` | Add project files and your worker config, never overwriting |
| `delegate new <id> [--prune]` | Create a spec from the template (or the stale-doc pruning spec) |
| `delegate run <id> [--tier T]` | Question phase, work phase, commit, acceptance checks |
| `delegate run <id> --research` | Read-only research; the report is the result |
| `delegate check <id>` | Rerun acceptance checks after a fix committed in the worktree |
| `delegate review <id>` | Adversarial review; failing tests come back as `REVIEW.patch` |
| `delegate plan-review [ids]` | Review `PLAN.md` and specs against the code before work starts |
| `delegate run --background <id>` | Same, detached: returns at once, keeps going after the shell or Claude turn ends |
| `delegate status [id] [--json]` | Where each task stands now: phase, worker, changed files, latest steps, stalled, next step |
| `delegate watch [id]` | `status`, refreshing in a terminal |
| `delegate wait [ids] [--timeout D]` | Block until a task finishes, is interrupted or stalls (Claude runs it in the background) |
| `delegate statusline` | One line for Claude Code's status line (`init` configures it) |
| `delegate list` | Status of every task: round, tier, checks, review |
| `delegate merge <id>` | Squash-merge if the gates pass, then delete all task state |
| `delegate discard <id>` | Delete all task state without merging |

Options for `run`: `--no-ask` skips the question phase, `--ask` forces it on a later round,
`--worker harness:model@effort` overrides the config once, `--dry-run` shows the plan.

## Roles × tiers: `~/.config/delegate/workers.toml`
User-level, because models, quotas and accounts are yours, not the project's. Each role and
tier lists candidates as `harness:model@effort`, tried in order; the next is used only when
one fails to start. Effort (`low | medium | high | xhigh | max`) is mapped to each
harness's own flag.

| Role | fast | standard | deep |
|---|---|---|---|
| implement | DeepSeek V4.1 Flash 1017 → GPT-6 Luna | GPT-6.1 Sol → DeepSeek 1017 → Sonnet 5.5 | Opus 5.5 → GPT-6.1 Sol xhigh |
| research | DeepSeek 1017 → GPT-6 Luna | GPT-6.1 Sol → Sonnet 5.5 | – |
| review | – | Sonnet 5.5 → GPT-6.1 Sol → DeepSeek 1017 | Opus 5.5 → GPT-6.1 Sol xhigh |
| plan review | – | GPT-6.1 Sol xhigh → DeepSeek 1017 | – |

GitHub Copilot is always the last candidate. The picks follow the Artificial Analysis
intelligence-vs-cost Pareto line; recheck it monthly and edit the lines, nothing else changes.

**Model families:** review skips the implementer's family, and plan review skips
`planner_family` (Claude). Family is inferred from the model name (gpt, claude, deepseek,
gemini, ...).

**Tier choice:** the default comes from `[defaults]`. Claude picks `fast`/`deep` by the table
in the delegate skill and tells you before running. After failed checks, the next run steps
up one tier automatically. Project rules can raise it (below).

## Project rules: `.delegate.toml`
Committed with the code, because they're about the code:
```toml
[[rules]]
paths    = ["src/auth/**", "migrations/**"]
min_tier = "deep"       # tier raised up front if the spec's Scope matches; merge refused below it
review   = "always"     # merge refused until a review ran on the final commit
```
The runner checks rules against the paths the task **actually changed**, so they hold even
if the tier picked at spec time was wrong.

## Merge gates
`delegate merge` refuses unless:
- the spec's acceptance commands ran on the final commit and all passed,
- every matching project rule is met (tier, review),
- a review on the final commit has no high-severity findings (`--accept-findings` overrides,
  only when you accept them).

It then squashes the task into one commit with the spec's title and Goal as the message:
```
t005: Add divide

Add a divide function that raises ValueError on division by zero,
so callers get a clear domain error instead of ZeroDivisionError.
```
Status, check results, reports and reviews are review-time state and never reach git
history. Anything worth keeping goes into a decision record or `AGENTS.md` before the merge.

## Where task files live
Specs, reports, checks, reviews, logs and worktrees live **outside the repo** in
`<main checkout>.delegate/<id>/` beside the main checkout, even when the lead runs from a
worktree (override with `DELEGATE_HOME`), and are deleted on merge or discard. Inside its
worktree a worker sees only `.task/` (its spec and templates), which git ignores via
`.git/info/exclude`.

## Sandbox
All harnesses except Codex (which has its own sandbox) run in bwrap. The filesystem is
read-only; the main checkout and every task folder, including the running task's own, are
hidden. Writes are allowed only to the worktree, the harness's state dir (`~/.pi`,
`~/.claude`, ...) and `~/.cache`. Git objects are read-only, so workers can't commit or move
branches. Acceptance checks run in the same sandbox. Network stays on.
`DELEGATE_SANDBOX=0` turns it off.

Still visible to a worker: the rest of your files (read-only) and git history, including
other tasks' unmerged `task/*` branches.

## Harness notes
| Harness | Invocation | Session kept across question → work phase |
|---|---|---|
| pi | `pi -p --approve --session-id <uuid> --model ... --thinking ...` | yes |
| codex | `codex exec --json -c sandbox_mode=...`, then `codex exec resume <thread>` | yes |
| claude | `claude -p --setting-sources project --strict-mcp-config ...` | yes (`--resume`) |
| opencode | `opencode run --auto --session <id>` | yes |
| gemini | `gemini --approval-mode auto_edit -p` | no (work phase starts fresh) |

The `claude` harness spends Claude quota: it's in the config only where Sonnet/Opus sit on
the Pareto line. Your user settings, hooks, MCP servers and skills are left out, and a
system prompt plus a line in `CLAUDE.md` keep it in the worker role. Pi workers keep their
sessions in `~/.pi`, Claude workers in `~/.claude/projects/`.

## What `delegate init` adds to a project
| File | Lifetime | Purpose |
|---|---|---|
| `AGENTS.md` | living, ≤150 lines | Read by every harness: commands, layout, rules, "code wins" |
| `CLAUDE.md` | living | Imports `AGENTS.md`, adds Claude's lead role (and the worker exception) |
| `PLAN.md` | living, pruned | Current goal, task table, open questions |
| `docs/decisions/` | append-only | Why X over Y; superseded, never edited |
| `.delegate.toml` | living | Project rules: minimum tiers and required reviews by path |
| `.claude/skills/delegate` | – | `/delegate`: plan review → spec → run → review → merge or discard |
| `.claude/skills/distill` | – | `/distill`: end a discussion, save only decisions and plan changes |
| `.claude/skills/evaluate` | – | `/evaluate`: build and keep runnable user scenarios, before the code they check |
| `evals/` | living | User scenarios and properties, each with an executable check; `evals/run [fast\|full]` |

## Source layout (`delegate/`)
| Path | Contents |
|---|---|
| `src/config.rs` | Tiers, harnesses, efforts, workers and families; `workers.toml` and `.delegate.toml` |
| `src/spec.rs` | Reading Acceptance commands, Scope paths, title and Goal from a spec |
| `src/task.rs` | Repo context, git, per-task state (`STATE.json`) |
| `src/harness.rs` | Worker commands per harness, sessions, bwrap sandbox |
| `src/commands.rs` | `new`, `run`, `check`, `review`, `plan-review`, `list`, `merge`, `discard` |
| `src/init.rs`, `src/prompts.rs` | `init` with embedded templates; worker prompts |
| `templates/` | Spec, report, prune and worker-config templates; `project/` files for `init` |

Strict by design: `unsafe` is forbidden and clippy `all` + `pedantic` are errors (see
`Cargo.toml`). Rust is pinned in `delegate/mise.toml`. Before committing changes:
```bash
cd delegate && mise exec -- cargo clippy --all-targets && mise exec -- cargo test
```
