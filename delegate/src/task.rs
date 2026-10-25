//! Repository context, git, and per-task state outside the repo.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Tier;

/// Paths every command needs.
#[derive(Debug)]
pub struct Ctx {
    /// Main checkout.
    pub root: PathBuf,
    /// Task state: `$DELEGATE_HOME` or `../<repo>.delegate`.
    pub dhome: PathBuf,
    /// Shared git dir (`.git` of the main checkout).
    pub gitdir: PathBuf,
}

impl Ctx {
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let root = PathBuf::from(
            git(&cwd, &["rev-parse", "--show-toplevel"]).context("not inside a git repository")?,
        );
        let dhome = std::env::var_os("DELEGATE_HOME").map_or_else(
            || {
                let name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                root.parent()
                    .unwrap_or(&root)
                    .join(format!("{name}.delegate"))
            },
            PathBuf::from,
        );
        let gitdir = PathBuf::from(git(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        Ok(Ctx {
            root,
            dhome,
            gitdir,
        })
    }
}

/// Run git and return trimmed-right stdout, failing with git's stderr.
pub fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundStatus {
    /// The worker stopped with blocking questions; nothing was built.
    Questions,
    /// The work phase ran and its changes are committed on the task branch.
    Done,
    /// No candidate could start.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Pass,
    Fail,
    NoCommands,
}

impl CheckOutcome {
    pub fn label(self) -> &'static str {
        match self {
            CheckOutcome::Pass => "pass",
            CheckOutcome::Fail => "FAIL",
            CheckOutcome::NoCommands => "none in spec",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checks {
    pub outcome: CheckOutcome,
    /// Commit the checks ran on.
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Round {
    pub number: u32,
    /// None when an explicit `--worker` bypassed the tiers.
    pub tier: Option<Tier>,
    pub worker: Option<String>,
    pub status: RoundStatus,
    pub checks: Option<Checks>,
}

impl Round {
    pub fn tier_label(&self) -> String {
        self.tier.map_or_else(|| "custom".into(), |t| t.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewState {
    pub worker: String,
    pub tier: Option<Tier>,
    /// Commit the review looked at.
    pub commit: String,
    /// High-severity findings; None when the reviewer's first line couldn't be read.
    pub high: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub base: Option<String>,
    pub rounds: Vec<Round>,
    pub review: Option<ReviewState>,
}

#[derive(Debug)]
pub struct Task {
    pub id: String,
    pub dir: PathBuf,
    pub wt: PathBuf,
    pub branch: String,
    pub spec: PathBuf,
    pub state: State,
}

impl Task {
    pub fn load(ctx: &Ctx, id: &str) -> Result<Self> {
        if id.is_empty() || id.starts_with(['_', '.']) || id.contains(['/', '\\']) {
            bail!("bad task id '{id}' (use something like t007)");
        }
        let dir = ctx.dhome.join(id);
        let state_file = dir.join("STATE.json");
        let state = if state_file.is_file() {
            serde_json::from_str(&std::fs::read_to_string(&state_file)?)
                .with_context(|| format!("reading {}", state_file.display()))?
        } else {
            State::default()
        };
        Ok(Task {
            id: id.to_string(),
            wt: dir.join("worktree"),
            branch: format!("task/{id}"),
            spec: dir.join("SPEC.md"),
            dir,
            state,
        })
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(
            self.dir.join("STATE.json"),
            serde_json::to_string_pretty(&self.state)? + "\n",
        )?;
        Ok(())
    }

    pub fn spec_text(&self) -> Result<String> {
        std::fs::read_to_string(&self.spec).with_context(|| {
            format!(
                "{} not found (create it with: delegate new {})",
                self.spec.display(),
                self.id
            )
        })
    }

    pub fn base(&self, ctx: &Ctx) -> Result<String> {
        match &self.state.base {
            Some(b) => Ok(b.clone()),
            None => git(&ctx.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        }
    }

    pub fn head(&self, ctx: &Ctx) -> Result<String> {
        git(&ctx.root, &["rev-parse", &self.branch])
    }

    pub fn changed_paths(&self, ctx: &Ctx) -> Result<Vec<String>> {
        let range = format!("{}...{}", self.base(ctx)?, self.branch);
        Ok(git(&ctx.root, &["diff", "--name-only", &range])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn last_round(&self) -> Option<&Round> {
        self.state.rounds.last()
    }

    pub fn last_done(&self) -> Option<&Round> {
        self.state
            .rounds
            .iter()
            .rev()
            .find(|r| r.status == RoundStatus::Done)
    }

    pub fn last_done_mut(&mut self) -> Option<&mut Round> {
        self.state
            .rounds
            .iter_mut()
            .rev()
            .find(|r| r.status == RoundStatus::Done)
    }

    pub fn worktree_dirty(&self) -> Result<bool> {
        Ok(!git(&self.wt, &["status", "--porcelain"])?.is_empty())
    }
}
