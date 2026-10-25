//! Delegate task specs to worker models by role and tier. Task state never lives in the repo.

mod commands;
mod config;
mod harness;
mod init;
mod prompts;
mod spec;
mod status;
mod statusline;
mod task;
mod wait;
mod watch;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::QuestionPhase;
use crate::config::Tier;
use crate::task::Ctx;

/// Delegate task specs to worker models (Pi, Codex, Claude Code, `OpenCode`, Gemini).
///
/// Workers per role and tier: `~/.config/delegate/workers.toml`. Project rules: `.delegate.toml`.
/// Task state: `$DELEGATE_HOME` or `../<repo>.delegate/<id>/`. All harnesses except codex run in
/// a bwrap sandbox (`DELEGATE_SANDBOX=0` disables it).
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Add the project files (AGENTS.md, skills, rules, ...) and your worker config if missing
    Init {
        /// Project directory (default: current)
        dir: Option<PathBuf>,
    },
    /// Create a task spec from the template
    New {
        id: String,
        /// Use the stale-doc pruning spec instead of the blank one
        #[arg(long)]
        prune: bool,
    },
    /// Implement: question phase, then the work, then the acceptance checks
    Run {
        id: String,
        #[arg(long, value_enum)]
        tier: Option<Tier>,
        /// One-off worker override, e.g. pi:openai-codex/gpt-6.1-sol@high
        #[arg(long)]
        worker: Option<String>,
        /// Force the question phase on a later round
        #[arg(long, conflicts_with = "no_ask")]
        ask: bool,
        /// Skip the question phase
        #[arg(long)]
        no_ask: bool,
        /// Show tier and candidates without running
        #[arg(long)]
        dry_run: bool,
        /// Read-only research task: the report is the deliverable
        #[arg(long, conflicts_with_all = ["ask", "no_ask"])]
        research: bool,
        /// Detach the run into its own session and return immediately
        #[arg(long)]
        background: bool,
    },
    /// Rerun the spec's acceptance commands on the committed branch
    Check { id: String },
    /// Adversarial review by a model family other than the implementer's
    Review {
        id: String,
        #[arg(long, value_enum)]
        tier: Option<Tier>,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Review PLAN.md and task specs against the code, by a non-planner family
    PlanReview {
        /// Task ids to include (default: all tasks with a spec)
        ids: Vec<String>,
        #[arg(long, value_enum)]
        tier: Option<Tier>,
        #[arg(long)]
        worker: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read the current phase, progress, outcome and next action
    Status {
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Block until a task finishes, is interrupted or stalls
    Wait {
        /// Tasks to watch (default: every running task)
        ids: Vec<String>,
        /// Give up after this long, e.g. 90s or 25m, and exit 3
        #[arg(long)]
        timeout: Option<String>,
    },
    /// Follow task progress (not built yet)
    Watch {
        id: Option<String>,
        #[arg(long, default_value_t = 2.0)]
        interval: f64,
    },
    /// Claude Code status line: one line of running tasks on stdin's project
    Statusline,
    /// Show every task's round, tier, checks and review
    List,
    /// Squash-merge once checks, project rules and review pass; delete the task state
    Merge {
        id: String,
        /// Merge despite high-severity review findings (only when the user accepts them)
        #[arg(long)]
        accept_findings: bool,
    },
    /// Delete the task's worktree, branch and folder without merging
    Discard { id: String },
}

fn dispatch(cmd: Cmd) -> Result<ExitCode> {
    if let Cmd::Init { dir } = &cmd {
        let dir = dir.clone().map_or_else(std::env::current_dir, Ok)?;
        init::init(&dir)?;
        return Ok(ExitCode::SUCCESS);
    }
    // The status line also runs outside a project, so it builds its own context.
    if let Cmd::Statusline = &cmd {
        statusline::run();
        return Ok(ExitCode::SUCCESS);
    }
    let ctx = Ctx::discover()?;
    if let Cmd::Wait { ids, timeout } = &cmd {
        return wait::run(&ctx, ids, timeout.as_deref());
    }
    match cmd {
        Cmd::Init { .. } | Cmd::Wait { .. } | Cmd::Statusline => unreachable!("handled above"),
        Cmd::New { id, prune } => commands::new(&ctx, &id, prune),
        Cmd::Run {
            id,
            tier,
            worker,
            ask,
            no_ask,
            dry_run,
            research,
            background,
        } => {
            let questions = match (ask, no_ask) {
                (true, _) => QuestionPhase::Always,
                (_, true) => QuestionPhase::Never,
                _ => QuestionPhase::Auto,
            };
            commands::run(
                &ctx,
                &commands::RunArgs {
                    id,
                    tier,
                    worker,
                    questions,
                    dry_run,
                    research,
                    background,
                },
            )
        }
        Cmd::Check { id } => commands::check(&ctx, &id),
        Cmd::Review {
            id,
            tier,
            worker,
            dry_run,
        } => commands::review(&ctx, &id, tier, worker.as_deref(), dry_run),
        Cmd::PlanReview {
            ids,
            tier,
            worker,
            dry_run,
        } => commands::plan_review(&ctx, &ids, tier, worker.as_deref(), dry_run),
        Cmd::Status { id, json } => status::run(&ctx, id.as_deref(), json),
        Cmd::Watch { id, interval } => watch::run(&ctx, id.as_deref(), interval),
        Cmd::List => commands::list(&ctx),
        Cmd::Merge {
            id,
            accept_findings,
        } => commands::merge(&ctx, &id, accept_findings),
        Cmd::Discard { id } => commands::discard(&ctx, &id),
    }?;
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match dispatch(Cli::parse().cmd) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
