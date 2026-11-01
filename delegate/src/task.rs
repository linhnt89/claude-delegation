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
    /// Locate existing state without registering this otherwise unconfigured machine.
    pub(crate) fn inspect() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let root = PathBuf::from(git(&cwd, &["rev-parse", "--show-toplevel"])?);
        let gitdir = PathBuf::from(git(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        let main = gitdir.parent().unwrap_or(&root);
        let legacy = main.with_file_name(format!(
            "{}.delegate",
            main.file_name().unwrap_or_default().to_string_lossy()
        ));
        // Existing homes still need their crash-safe migration on first use.
        if legacy.exists() && std::env::var_os("DELEGATE_HOME").is_none() {
            return Self::discover();
        }
        let dhome = std::env::var_os("DELEGATE_HOME").map_or_else(
            || crate::checkout::peek(&gitdir, main, &crate::vault::data_dir()),
            |path| Ok(PathBuf::from(path)),
        )?;
        Ok(Self {
            root,
            dhome,
            gitdir,
        })
    }

    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let mut root = PathBuf::from(
            git(&cwd, &["rev-parse", "--show-toplevel"]).context("not inside a git repository")?,
        );
        // Migration can move the very worktree this command was started in.
        // Its administrative backlink is repaired along with the other worktrees.
        let admin = if root.join(".git").is_file() {
            Some(PathBuf::from(git(
                &root,
                &["rev-parse", "--absolute-git-dir"],
            )?))
        } else {
            None
        };
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
        if let Some(admin) = admin
            && let Ok(backlink) = std::fs::read_to_string(admin.join("gitdir"))
            && let Some(moved) = Path::new(backlink.trim()).parent()
            && moved != root
        {
            let relative = cwd.strip_prefix(&root)?;
            std::env::set_current_dir(moved.join(relative))
                .context("following moved task worktree; retry delegate from the main checkout")?;
            root = moved.into();
        }
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
    CheckError,
    NoCommands,
}

impl CheckOutcome {
    pub fn label(self) -> &'static str {
        match self {
            CheckOutcome::Pass => "pass",
            CheckOutcome::Fail => "FAIL",
            CheckOutcome::CheckError => "CHECK ERROR",
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
    /// The worker hit its account's usage limit mid-round; its work was committed and
    /// handed to the next candidate. Absent (false) on rounds saved by older versions.
    #[serde(default)]
    pub quota: bool,
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

    pub fn status_label(&self, no_changes: bool, committed: bool) -> String {
        let failed_quota = self.status == RoundStatus::Done && self.quota && !committed;
        let label = match self.status {
            RoundStatus::Running => "unfinished (interrupted, or still running)".into(),
            RoundStatus::Questions => "questions".into(),
            RoundStatus::Failed => "failed to start".into(),
            RoundStatus::Done if self.research => "research done".into(),
            RoundStatus::Done if failed_quota => "failed to start (quota)".into(),
            RoundStatus::Done if self.quota => "done, quota".into(),
            RoundStatus::Done if no_changes && self.checks.is_none() => "done, no changes".into(),
            RoundStatus::Done => format!(
                "done, checks {}",
                self.checks.as_ref().map_or("not run", |c| match c.outcome {
                    CheckOutcome::CheckError => "check error",
                    outcome => outcome.label(),
                })
            ),
        };
        if failed_quota {
            return label;
        }
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

    /// Workers on the newest matching commit with trailers on this task's base branch.
    /// Task state is deleted after merge, so attribution must come from git history.
    /// Ignore unattributed follow-ups; unreadable attribution imposes no family rule.
    pub fn check_writers(&self, ctx: &Ctx, id: &str) -> Result<Vec<crate::config::Worker>> {
        let base = self.base(ctx)?;
        let history = git(
            &ctx.root,
            &["log", "--first-parent", "--format=%H%x00%s", &base, "--"],
        )?;
        let prefix = format!("{id}: ");
        for commit in history.lines().filter_map(|line| {
            let (commit, subject) = line.split_once('\0')?;
            subject.starts_with(&prefix).then_some(commit)
        }) {
            let trailers = git(
                &ctx.root,
                &[
                    "show",
                    "-s",
                    "--format=%(trailers:key=Worker,valueonly)",
                    commit,
                    "--",
                ],
            )?;
            if trailers.is_empty() {
                continue;
            }
            return Ok(trailers
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<_>>>()
                .unwrap_or_default());
        }
        Ok(Vec::new())
    }

    /// Distinct workers whose rounds actually made commits, in round order.
    pub fn committed_workers(&self, ctx: &Ctx) -> Result<Vec<String>> {
        let subjects = self.round_subjects(ctx)?;
        let mut workers = Vec::new();
        for round in &self.state.rounds {
            let subject = format!("task {} round {}", self.id, round.number);
            if !round.research
                && subjects.iter().any(|line| line == &subject)
                && let Some(worker) = &round.worker
                && !workers.contains(worker)
            {
                workers.push(worker.clone());
            }
        }
        Ok(workers)
    }

    /// Whether this round recorded a commit of its own on the task branch.
    pub fn round_committed(&self, ctx: &Ctx, round: &Round) -> Result<bool> {
        let subject = format!("task {} round {}", self.id, round.number);
        Ok(self.round_subjects(ctx)?.iter().any(|line| line == &subject))
    }

    /// Commit subjects on the task branch since its base.
    fn round_subjects(&self, ctx: &Ctx) -> Result<Vec<String>> {
        let range = format!("{}..{}", self.base(ctx)?, self.branch);
        Ok(git(&ctx.root, &["log", "--format=%s", &range, "--"])?
            .lines()
            .map(str::to_string)
            .collect())
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
            quota: false,
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
                r.status_label(false, true),
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
            assert_eq!(r.status_label(false, true), expected);
            if status != RoundStatus::Done {
                assert_eq!(r.status_label(true, true), expected);
            }
        }
    }

    #[test]
    fn done_labels_preserve_checks_and_warn_about_worker_exit() {
        for (outcome, expected) in [
            (CheckOutcome::Pass, "pass"),
            (CheckOutcome::Fail, "FAIL"),
            (CheckOutcome::CheckError, "check error"),
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
                    r.status_label(no_changes, true),
                    format!("done, checks {expected}")
                );
            }
            r.exit = Some(3);
            assert_eq!(
                r.status_label(false, true),
                format!("done, checks {expected}, worker exited 3")
            );
        }
    }

    #[test]
    fn no_changes_label_and_exit_warning() {
        let mut r = round();
        assert_eq!(r.status_label(true, true), "done, no changes");
        r.exit = Some(3);
        assert_eq!(r.status_label(true, true), "done, no changes, worker exited 3");
        r.exit = Some(-1);
        assert_eq!(r.status_label(true, true), "done, no changes, worker exited -1");
    }

    #[test]
    fn old_round_without_exit_still_loads() {
        let r: Round = serde_json::from_str(
            r#"{"number":1,"tier":null,"worker":"pi:test","status":"done","checks":null}"#,
        )
        .unwrap();
        assert_eq!(r.exit, None);
        assert!(!r.research);
        assert!(!r.quota);
        assert_eq!(r.status_label(false, true), "done, checks not run");
    }

    #[test]
    fn check_outcomes_round_trip_and_old_rounds_still_load() {
        for (stored, outcome) in [
            ("pass", CheckOutcome::Pass),
            ("fail", CheckOutcome::Fail),
            ("check_error", CheckOutcome::CheckError),
            ("no_commands", CheckOutcome::NoCommands),
        ] {
            let json = format!(
                r#"{{"number":1,"tier":"standard","worker":"pi:test","status":"done","checks":{{"outcome":"{stored}","commit":"abc"}}}}"#
            );
            let r: Round = serde_json::from_str(&json).unwrap();
            assert_eq!(r.checks.as_ref().unwrap().outcome, outcome);
            assert_eq!(r.exit, None);
            let saved = serde_json::to_string(&r).unwrap();
            let restored: Round = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored.checks.unwrap().outcome, outcome);
        }
    }

    #[test]
    fn quota_round_is_visible_and_old_rounds_default_to_false() {
        let r = Round {
            quota: true,
            ..round()
        };
        assert_eq!(r.status_label(false, true), "done, quota");
        assert_eq!(r.status_label(true, true), "done, quota");
        assert_eq!(r.status_label(false, false), "failed to start (quota)");
        assert_eq!(r.status_label(true, false), "failed to start (quota)");
        let old: Round = serde_json::from_str(
            r#"{"number":1,"tier":"fast","worker":"pi:test","status":"done","checks":null}"#,
        )
        .unwrap();
        assert!(!old.quota);
    }

    #[test]
    fn quota_round_with_exit_hides_the_exit_when_no_commit() {
        let mut r = Round {
            quota: true,
            exit: Some(1),
            ..round()
        };
        assert_eq!(r.status_label(false, false), "failed to start (quota)");
        assert_eq!(r.status_label(false, true), "done, quota, worker exited 1");
        r.exit = None;
        assert_eq!(r.status_label(false, true), "done, quota");
    }

    #[test]
    fn research_round_shows_research_done() {
        let mut r = Round {
            status: RoundStatus::Done,
            research: true,
            ..round()
        };
        assert_eq!(r.status_label(true, true), "research done");
        assert_eq!(r.status_label(false, true), "research done");
        r.exit = Some(3);
        assert_eq!(r.status_label(true, true), "research done, worker exited 3");
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
            restored.status_label(false, true),
            "unfinished (interrupted, or still running), worker exited 3"
        );
    }
}
