//! Repository context, git, and per-task state outside the repo.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Tier;

/// Paths every command needs.
#[derive(Debug)]
pub struct Ctx {
    /// Toplevel of the current checkout (possibly a linked worktree).
    pub root: PathBuf,
    /// Task state: `$DELEGATE_HOME` or the checkout-id folder under the delegate data dir.
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
        let gitdir = PathBuf::from(git(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        let dhome = if let Some(path) = std::env::var_os("DELEGATE_HOME") {
            crate::checkout::cleanup()?;
            PathBuf::from(path)
        } else {
            crate::checkout::resolve(
                &gitdir,
                gitdir.parent().unwrap_or(&root),
                &crate::vault::data_dir(),
            )?
        };
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
        // Read-only status/diff callers must never refresh the index on disk.
        .env("GIT_OPTIONAL_LOCKS", "0")
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
    /// A candidate started, but the run (including checks) has not finished.
    Running,
    /// The worker stopped with blocking questions; nothing was built.
    Questions,
    /// The work phase finished and any acceptance checks completed.
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
    /// A read-only research round: its result is the report, never a commit.
    #[serde(default)]
    pub research: bool,
    /// Exit code of the work phase, absent for rounds saved by older versions.
    #[serde(default)]
    pub exit: Option<i32>,
    pub checks: Option<Checks>,
}

impl Round {
    pub fn fail_to_start(&mut self) {
        self.status = RoundStatus::Failed;
        self.worker = None;
    }

    pub fn tier_label(&self) -> String {
        self.tier.map_or_else(|| "custom".into(), |t| t.to_string())
    }

    pub fn status_label(&self, no_changes: bool) -> String {
        let label = match self.status {
            RoundStatus::Running => "unfinished (interrupted, or still running)".into(),
            RoundStatus::Questions => "questions".into(),
            RoundStatus::Failed => "failed to start".into(),
            RoundStatus::Done if self.research => "research done".into(),
            RoundStatus::Done if no_changes && self.checks.is_none() => "done, no changes".into(),
            RoundStatus::Done => format!(
                "done, checks {}",
                self.checks
                    .as_ref()
                    .map_or("not run", |c| c.outcome.label())
            ),
        };
        match self.exit.filter(|&exit| exit != 0) {
            Some(exit) => format!("{label}, worker exited {exit}"),
            None => label,
        }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub session: String,
    pub dir: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub owner: Option<Owner>,
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
        // An interrupted save must leave the previous state readable.
        let pending = self
            .dir
            .join(format!(".STATE-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&pending, serde_json::to_string_pretty(&self.state)? + "\n")?;
        std::fs::rename(pending, self.dir.join("STATE.json"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_shares_home_across_worktrees() -> Result<()> {
        const EXPECTED_HOME: &str = "_DELEGATE_TEST_DISCOVER_HOME";
        struct TempRepo(PathBuf);
        impl Drop for TempRepo {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        if let Some(expected) = std::env::var_os(EXPECTED_HOME) {
            let ctx = Ctx::discover()?;
            assert_eq!(ctx.dhome, PathBuf::from(expected));
            assert_eq!(ctx.root, std::env::current_dir()?);
            return Ok(());
        }

        // Discover uses process-global cwd and environment; isolate each call in a child.
        let temp = TempRepo(
            std::env::temp_dir().join(format!("delegate-discover-{}", uuid::Uuid::new_v4())),
        );
        let main = temp.0.join("project");
        std::fs::create_dir_all(&main)?;
        git(&main, &["init", "-q"])?;
        git(
            &main,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "initial",
                "--allow-empty",
            ],
        )?;
        let linked = main.join(".claude/worktrees/lead");
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &linked.to_string_lossy(),
            ],
        )?;
        let data = temp.0.join("delegate");
        let shared_home = crate::checkout::resolve(&main.join(".git"), &main, &data)?;
        let override_home = temp.0.join("override");
        for (checkout, expected, overridden) in [
            (&main, &shared_home, false),
            (&linked, &shared_home, false),
            (&linked, &override_home, true),
        ] {
            let mut child = Command::new(std::env::current_exe()?);
            child
                .args([
                    "--exact",
                    "task::tests::discover_shares_home_across_worktrees",
                ])
                .current_dir(checkout)
                .env_remove("DELEGATE_HOME")
                .env("XDG_DATA_HOME", &temp.0)
                .env(EXPECTED_HOME, expected);
            if overridden {
                child.env("DELEGATE_HOME", expected);
            }
            let output = child.output()?;
            assert!(
                output.status.success(),
                "discover from {} failed:\n{}\n{}",
                checkout.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        Ok(())
    }

    #[test]
    fn owner_round_trips_and_old_state_loads() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-owner-{}", uuid::Uuid::new_v4()));
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        let mut task = Task::load(&ctx, "t001")?;
        assert_eq!(task.state.owner, None);
        std::fs::create_dir_all(&task.dir)?;
        std::fs::write(
            task.dir.join("STATE.json"),
            r#"{"base":null,"rounds":[],"review":null}"#,
        )?;
        task = Task::load(&ctx, "t001")?;
        assert_eq!(task.state.owner, None);
        let owner = Owner {
            session: "session-a".into(),
            dir: "/projects/pane-polish".into(),
        };
        task.state.owner = Some(owner.clone());
        task.save()?;
        assert_eq!(Task::load(&ctx, "t001")?.state.owner, Some(owner));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(task.dir.join("STATE.json"))?)?;
        assert_eq!(json["owner"]["session"], "session-a");
        assert_eq!(json["owner"]["dir"], "/projects/pane-polish");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn round() -> Round {
        Round {
            number: 1,
            tier: None,
            worker: Some("pi:test".into()),
            status: RoundStatus::Done,
            research: false,
            exit: Some(0),
            checks: None,
        }
    }

    #[test]
    fn failed_to_start_clears_attempted_worker() {
        let mut r = Round {
            status: RoundStatus::Running,
            exit: None,
            ..round()
        };
        assert_eq!(r.worker.as_deref(), Some("pi:test"));

        r.fail_to_start();

        assert_eq!(r.status, RoundStatus::Failed);
        assert_eq!(r.worker, None);
        assert_eq!(
            format!(
                "{}  [{}]",
                r.status_label(false),
                r.worker.as_deref().unwrap_or("-")
            ),
            "failed to start  [-]"
        );
    }

    #[test]
    fn round_status_labels() {
        for (status, expected) in [
            (
                RoundStatus::Running,
                "unfinished (interrupted, or still running)",
            ),
            (RoundStatus::Questions, "questions"),
            (RoundStatus::Failed, "failed to start"),
            (RoundStatus::Done, "done, checks not run"),
        ] {
            let r = Round { status, ..round() };
            assert_eq!(r.status_label(false), expected);
            if status != RoundStatus::Done {
                assert_eq!(r.status_label(true), expected);
            }
        }
    }

    #[test]
    fn done_labels_preserve_checks_and_warn_about_worker_exit() {
        for (outcome, expected) in [
            (CheckOutcome::Pass, "pass"),
            (CheckOutcome::Fail, "FAIL"),
            (CheckOutcome::NoCommands, "none in spec"),
        ] {
            let mut r = Round {
                checks: Some(Checks {
                    outcome,
                    commit: "abc".into(),
                }),
                ..round()
            };
            for no_changes in [false, true] {
                assert_eq!(
                    r.status_label(no_changes),
                    format!("done, checks {expected}")
                );
            }
            r.exit = Some(3);
            assert_eq!(
                r.status_label(false),
                format!("done, checks {expected}, worker exited 3")
            );
        }
    }

    #[test]
    fn no_changes_label_and_exit_warning() {
        let mut r = round();
        assert_eq!(r.status_label(true), "done, no changes");
        r.exit = Some(3);
        assert_eq!(r.status_label(true), "done, no changes, worker exited 3");
        r.exit = Some(-1);
        assert_eq!(r.status_label(true), "done, no changes, worker exited -1");
    }

    #[test]
    fn old_round_without_exit_still_loads() {
        let r: Round = serde_json::from_str(
            r#"{"number":1,"tier":null,"worker":"pi:test","status":"done","checks":null}"#,
        )
        .unwrap();
        assert_eq!(r.exit, None);
        assert!(!r.research);
        assert_eq!(r.status_label(false), "done, checks not run");
    }

    #[test]
    fn research_round_shows_research_done() {
        let mut r = Round {
            status: RoundStatus::Done,
            research: true,
            ..round()
        };
        assert_eq!(r.status_label(true), "research done");
        assert_eq!(r.status_label(false), "research done");
        r.exit = Some(3);
        assert_eq!(r.status_label(true), "research done, worker exited 3");
    }

    #[test]
    fn running_round_with_exit_round_trips() {
        let r = Round {
            status: RoundStatus::Running,
            exit: Some(3),
            ..round()
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["status"], "running");
        assert_eq!(json["exit"], 3);
        let restored: Round = serde_json::from_value(json).unwrap();
        assert_eq!(restored.status, RoundStatus::Running);
        assert_eq!(restored.exit, Some(3));
        assert_eq!(
            restored.status_label(false),
            "unfinished (interrupted, or still running), worker exited 3"
        );
    }
}
