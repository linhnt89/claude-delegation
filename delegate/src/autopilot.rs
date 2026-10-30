//! A detached, non-model driver: one fresh headless lead per wave, on its own branch.
//! Durable state is outside the repository. Process identities include boot/start ticks;
//! kernel locks serialize startup and exclude already-running interactive mutations.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::config::WorkersConfig;
use crate::quota::{self, Account, AccountName, LoginSource, RunwayStatus};
use crate::task::{CheckOutcome, Ctx, RoundStatus, Task, git};
use crate::{prompts, status};

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Start a detached run of the mandate's waves
    Start {
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..))]
        max_waves: u32,
        #[arg(long)]
        quota_floor: Option<String>,
    },
    /// Stop the lead and driver, leaving worker tasks intact
    Stop,
    /// Show the current or last run
    Status {
        #[arg(long)]
        json: bool,
    },
    #[command(name = "_drive", hide = true)]
    Drive { id: String },
}

pub fn dispatch(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    // A detached driver's cwd changes, so an explicit relative task home must not.
    let absolute = Ctx {
        root: ctx.root.clone(),
        dhome: std::path::absolute(&ctx.dhome)?,
        gitdir: ctx.gitdir.clone(),
    };
    let ctx = &absolute;
    match cmd {
        Cmd::Start {
            max_waves,
            quota_floor,
        } => start(ctx, max_waves, quota_floor.as_deref()),
        Cmd::Stop => stop(ctx),
        Cmd::Status { json } => show(ctx, json),
        Cmd::Drive { id } => drive(ctx, &id),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Process {
    pid: u32,
    start: String,
    boot: String,
}

impl Process {
    fn read(pid: u32) -> Result<Self> {
        let text = fs::read_to_string(format!("/proc/{pid}/stat"))
            .context("reading autopilot process identity; retry delegate autopilot start")?;
        let fields = text.rsplit_once(')').context("invalid process identity")?.1;
        let start = fields
            .split_whitespace()
            .nth(19)
            .context("missing process start time")?;
        Ok(Self {
            pid,
            start: start.into(),
            boot: fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                .trim()
                .into(),
        })
    }

    fn alive(&self) -> bool {
        status::is_alive(self.pid, &self.start, &self.boot)
    }
}

/// Explicit unlock also releases locks temporarily inherited across another thread's fork.
#[derive(Debug)]
pub struct Lock(File);

impl Lock {
    fn open(path: &Path, shared: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        if shared {
            file.lock_shared()?;
        } else {
            file.lock()?;
        }
        Ok(Self(file))
    }

    // A pre-autopilot operation still excludes initial startup, without creating files.
    fn directory(path: &Path, shared: bool) -> Result<Option<Self>> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if shared {
            file.lock_shared()?;
        } else {
            file.try_lock().context(
                "another lead is working; merge, discard or finish its tasks first, then retry",
            )?;
        }
        Ok(Some(Self(file)))
    }

    fn exclusive_now(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock().context(
            "another lead is working; merge, discard or finish its tasks first, then retry",
        )?;
        Ok(Self(file))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Mandate {
    goals: Vec<String>,
    gate: String,
    max_rounds: usize,
}

fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|line| line.trim() != heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with(['.', '_'])
        && !id.contains(['/', '\\'])
        && !id.chars().any(char::is_whitespace)
}

impl Mandate {
    fn parse(text: &str) -> Result<Self> {
        let lines = section(text, "## Mandate");
        let value = |key: &str| {
            lines
                .iter()
                .find_map(|line| line.trim().strip_prefix(key))
                .map(str::trim)
        };
        let goals = value("- Goal:")
            .unwrap_or_default()
            .split([',', ' ', '\t'])
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        // A goal's occurrence in the mandate itself does not establish that it is planned.
        let mut in_mandate = false;
        let outside = text
            .lines()
            .filter(|line| {
                if line.starts_with("## ") {
                    in_mandate = line.trim() == "## Mandate";
                }
                !in_mandate
            })
            .collect::<Vec<_>>()
            .join("\n");
        if goals.is_empty()
            || goals.iter().any(|id| {
                !valid_id(id)
                    || !outside
                        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                        .any(|word| word == id)
            })
        {
            bail!(
                "write ## Mandate in PLAN.md with '- Goal: t001, t002' naming one or more ids elsewhere in PLAN.md, and '- Gate: `scripts/check`'; commit PLAN.md first"
            );
        }
        let gate = value("- Gate:").and_then(|v| v.strip_prefix('`'))
            .and_then(|v| v.strip_suffix('`')).filter(|v| !v.trim().is_empty())
            .context("write '- Gate: `scripts/check`' under ## Mandate in PLAN.md, then commit PLAN.md first")?;
        let max_rounds = value("- Max rounds:")
            .map_or(Ok(3), str::parse::<usize>)
            .context("Max rounds must be a positive whole number; fix and commit PLAN.md first")?;
        if max_rounds == 0 {
            bail!("Max rounds must be at least 1; fix and commit PLAN.md first");
        }
        Ok(Self {
            goals,
            gate: gate.into(),
            max_rounds,
        })
    }
}

fn duration(text: &str) -> Result<Duration> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        _ => 0,
    };
    let seconds = digits
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(scale))
        .filter(|n| *n > 0)
        .with_context(|| {
            format!("invalid autopilot quota floor {text:?}; use a duration such as 30m, 1h or 3h")
        })?;
    Ok(Duration::from_secs(seconds))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Run {
    id: String,
    driver: Process,
    lead: Option<Process>,
    wave: u32,
    max_waves: u32,
    stop_reason: Option<String>,
    report: Option<PathBuf>,
    branch: String,
    base: String,
    project: String,
    worktree: PathBuf,
    log: PathBuf,
    initial: String,
    mandate: Mandate,
    quota_floor: u64,
    notify: Option<String>,
    reset: Option<String>,
    last_wave: Vec<String>,
    task_ids: Vec<String>,
}

impl Run {
    fn running(&self) -> bool {
        self.stop_reason.is_none() && self.driver.alive()
    }
    fn stop_requested(&self) -> bool {
        self.log.join("stop").exists()
    }
    fn log(&self, line: &str) -> Result<()> {
        writeln!(log_file(self)?, "{line}")?;
        Ok(())
    }
}

fn home(ctx: &Ctx) -> PathBuf {
    ctx.dhome.join("autopilot")
}
fn load(ctx: &Ctx) -> Result<Option<Run>> {
    let path = home(ctx).join("state.json");
    match fs::read(&path) {
        Ok(bytes) => {
            let run: Run = serde_json::from_slice(&bytes).with_context(|| {
                format!(
                    "reading {}; remove it or run delegate autopilot stop before retrying",
                    path.display()
                )
            })?;
            // Every observer of a dead driver must retire its lead, not just start.
            if !run.driver.alive() {
                kill_leftover(&run);
            }
            Ok(Some(run))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!(
                "reading {}; remove it or run delegate autopilot stop before retrying",
                path.display()
            )
        }),
    }
}
fn save(ctx: &Ctx, run: &Run) -> Result<()> {
    let pending = home(ctx).join(format!(".state-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&pending, serde_json::to_vec_pretty(run)?)?;
    fs::rename(pending, home(ctx).join("state.json"))?;
    Ok(())
}
fn log_file(run: &Run) -> Result<File> {
    Ok(OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.log.join("autopilot.log"))?)
}
fn control(ctx: &Ctx) -> Result<Lock> {
    fs::create_dir_all(home(ctx))?;
    Lock::open(&home(ctx).join("control.guard"), false)
}
fn refusal() -> anyhow::Error {
    anyhow::anyhow!(
        "autopilot owns the lead lock (or this run id is no longer authorized); run delegate autopilot stop before running, merging or discarding tasks"
    )
}

/// The interactive caller holds a shared lease until its whole operation has finished.
/// Authorized lead commands instead rely on the driver's exclusive lease and live identity.
pub fn authorize(ctx: &Ctx) -> Result<Option<Lock>> {
    let directory = Lock::directory(&ctx.dhome, true)?;
    let token = std::env::var("DELEGATE_AUTOPILOT").ok();
    // Ordinary commands (including dry runs) leave no autopilot footprint.
    if load(ctx)?.is_none() {
        return if token.is_some() {
            Err(refusal())
        } else {
            Ok(directory)
        };
    }
    let _control = control(ctx)?;
    if let Some(run) = load(ctx)?
        && run.running()
    {
        if token.as_deref() != Some(&run.id) || ctx.root != run.worktree {
            return Err(refusal());
        }
        return Ok(None);
    }
    // An orphaned lead may not turn into an ordinary interactive lead after its driver dies.
    if token.is_some() {
        return Err(refusal());
    }
    Ok(Some(Lock::open(&home(ctx).join("work.guard"), true)?))
}

fn authorized_run(ctx: &Ctx) -> Result<Option<Run>> {
    if let Some(token) = std::env::var_os("DELEGATE_AUTOPILOT") {
        let run = load(ctx)?
            .filter(|run| run.running() && token == run.id.as_str() && ctx.root == run.worktree)
            .ok_or_else(refusal)?;
        Ok(Some(run))
    } else {
        Ok(None)
    }
}

pub fn before_run(ctx: &Ctx, task: &Task) -> Result<()> {
    let Some(run) = authorized_run(ctx)? else {
        return Ok(());
    };
    if run.log.join("gate.failed").exists() {
        bail!(
            "the autopilot gate failed; do not start more tasks; end the wave and inspect delegate autopilot status (or delegate autopilot stop)"
        );
    }
    let failures = task
        .state
        .rounds
        .iter()
        .filter(|round| {
            round.status == RoundStatus::Done
                && !round.quota
                && !round.research
                && round
                    .checks
                    .as_ref()
                    .is_some_and(|checks| checks.outcome == CheckOutcome::Fail)
        })
        .count();
    if failures >= run.mandate.max_rounds {
        bail!(
            "{} has {failures} product-failing rounds (Max rounds: {}); park it with its question under ## Waiting for you in PLAN.md",
            task.id,
            run.mandate.max_rounds
        );
    }
    Ok(())
}

/// Only successful task merges reach here. A failed gate is sticky for the whole run.
pub fn after_merge(ctx: &Ctx, id: &str) -> Result<()> {
    let Some(run) = authorized_run(ctx)? else {
        return Ok(());
    };
    let mut journal = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.log.join("merges"))?;
    writeln!(journal, "{id}")?;
    run.log(&format!("merged {id}; gate: {}", run.mandate.gate))?;
    let result = Command::new("sh")
        .args(["-c", &run.mandate.gate])
        .current_dir(&run.worktree)
        .stdin(Stdio::null())
        .stdout(log_file(&run)?)
        .stderr(log_file(&run)?)
        .status();
    if !result.as_ref().is_ok_and(std::process::ExitStatus::success) {
        fs::write(run.log.join("gate.failed"), format!("{id}\n"))?;
        bail!(
            "autopilot gate failed after {id}: {}; end the wave and inspect delegate autopilot status",
            run.mandate.gate
        );
    }
    Ok(())
}

fn ancestor(ctx: &Ctx, older: &str, newer: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", older, newer])
        .current_dir(&ctx.root)
        .output()?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!(
            "could not compare autopilot with {newer}; inspect git history and retry: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn foreign_tasks(ctx: &Ctx) -> Result<()> {
    let mut foreign = Vec::new();
    for id in status::task_ids(ctx) {
        let task = Task::load(ctx, &id)?;
        if let Some(base) = &task.state.base
            && base != "autopilot"
        {
            foreign.push(format!("{id} (base {base})"));
        } else if task.state.base.is_none() && task.wt.exists() {
            foreign.push(format!("{id} (base unknown)"));
        }
    }
    if !foreign.is_empty() {
        bail!(
            "open tasks do not belong to autopilot: {}; merge, discard or finish them first",
            foreign.join(", ")
        );
    }
    Ok(())
}

fn prepare_worktree(ctx: &Ctx, base: &str) -> Result<PathBuf> {
    let path = home(ctx).join("worktree");
    let exists = git(
        &ctx.root,
        &["rev-parse", "--verify", "refs/heads/autopilot"],
    )
    .is_ok();
    let reset = !exists || ancestor(ctx, "autopilot", base)?;
    if exists && !reset && !ancestor(ctx, base, "autopilot")? {
        bail!(
            "the base branch moved past unlanded autopilot work; land or discard autopilot first"
        );
    }
    if path.exists() {
        if git(&path, &["symbolic-ref", "--short", "HEAD"])? != "autopilot" {
            bail!(
                "{} is not on branch autopilot; repair or remove that worktree and retry",
                path.display()
            );
        }
        // Reuse interrupted/unlanded work as it is. Only a reset to a newer base
        // requires cleanliness; never erase the previous lead's uncommitted handoff.
        if reset && git(&path, &["rev-parse", "HEAD"])? != git(&path, &["rev-parse", base])? {
            if !git(&path, &["status", "--porcelain"])?.is_empty() {
                bail!(
                    "{} has uncommitted changes; commit or discard them there before resetting autopilot to the base",
                    path.display()
                );
            }
            git(&path, &["reset", "-q", "--hard", base])?;
        }
    } else {
        if exists && reset {
            git(&ctx.root, &["branch", "-f", "autopilot", base])?;
        }
        let mut args = vec!["worktree", "add", "-q"];
        if !exists {
            args.extend(["-b", "autopilot"]);
        }
        let path_text = path.to_string_lossy();
        args.extend([path_text.as_ref(), if exists { "autopilot" } else { base }]);
        git(&ctx.root, &args).context("creating autopilot worktree; inspect git worktree list and repair or remove a stale autopilot worktree before retrying")?;
    }
    path.canonicalize()
        .context("resolving autopilot worktree; check permissions and retry")
}

fn start(ctx: &Ctx, max_waves: u32, floor: Option<&str>) -> Result<()> {
    if !crate::harness::on_path("claude") {
        bail!(
            "claude is not on PATH; install Claude Code or add claude to PATH, then retry delegate autopilot start"
        );
    }
    fs::create_dir_all(&ctx.dhome)?;
    let directory = Lock::directory(&ctx.dhome, false)?;
    let control = control(ctx)?;
    let mut previous = load(ctx)?;
    if previous.as_ref().is_some_and(Run::running) {
        bail!(
            "autopilot is already running; use delegate autopilot status or delegate autopilot stop first"
        );
    }
    if let Some(previous) = previous.as_mut() {
        kill_leftover(previous);
        if previous.stop_reason.is_none()
            || (previous.notify.is_some() && !previous.log.join("notify.attempted").exists())
        {
            let reason = previous.stop_reason.clone().unwrap_or_else(|| {
                if previous.stop_requested() {
                    "stopped".into()
                } else {
                    "driver_died".into()
                }
            });
            // Recovery notification must not retain the startup/lead lock.
            drop(control);
            drop(directory);
            finish(ctx, previous, &reason)?;
            return start(ctx, max_waves, floor);
        }
    }
    let work = Lock::exclusive_now(&home(ctx).join("work.guard"))?;
    if !git(&ctx.root, &["status", "--porcelain", "--", "PLAN.md"])?.is_empty() {
        bail!("commit PLAN.md first, then retry delegate autopilot start");
    }
    let base = git(&ctx.root, &["symbolic-ref", "--short", "HEAD"])
        .context("check out a base branch before running delegate autopilot start")?;
    if base == "autopilot" {
        bail!("start autopilot from your base branch's checkout, not the autopilot worktree");
    }
    foreign_tasks(ctx)?;
    let cfg = WorkersConfig::load()?;
    let quota_floor = duration(
        floor
            .or(cfg.autopilot.quota_floor.as_deref())
            .unwrap_or("1h"),
    )?
    .as_secs();
    let worktree = prepare_worktree(ctx, &base)?;
    let plan = fs::read_to_string(worktree.join("PLAN.md"))
        .context("write ## Mandate in PLAN.md with '- Goal: t001' and '- Gate: `scripts/check`', then commit PLAN.md first")?;
    let mandate = Mandate::parse(&plan)?;
    let id = uuid::Uuid::new_v4().to_string();
    let log = home(ctx).join("runs").join(&id);
    fs::create_dir_all(&log)?;
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log.join("autopilot.log"))?;
    let mut child = Command::new("setsid")
        .arg(std::env::current_exe()?)
        .args(["autopilot", "_drive", &id])
        .current_dir(&worktree)
        .env("DELEGATE_HOME", &ctx.dhome)
        .env_remove("DELEGATE_AUTOPILOT")
        .env_remove("DELEGATE_AUTOPILOT_WAVE")
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .spawn()
        .context("starting detached autopilot driver; install setsid (util-linux) and retry")?;
    let persisted = (|| {
        let run = Run {
            id,
            driver: Process::read(child.id())?,
            lead: None,
            wave: 0,
            max_waves,
            stop_reason: None,
            report: previous.and_then(|run| run.report),
            branch: "autopilot".into(),
            base,
            project: ctx
                .root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .replace(['\n', '\r'], " "),
            initial: git(&worktree, &["rev-parse", "HEAD"])?,
            worktree,
            log,
            mandate,
            quota_floor,
            notify: cfg.autopilot.notify,
            reset: None,
            last_wave: Vec::new(),
            task_ids: status::task_ids(ctx),
        };
        save(ctx, &run)?;
        Ok::<_, anyhow::Error>(run)
    })();
    if persisted.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let run = persisted?;
    drop(work);
    drop(control);
    drop(directory);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !run.log.join("ready").exists() {
        if let Some(exit) = child.try_wait()? {
            bail!(
                "autopilot driver failed to start ({exit}); inspect {} and retry",
                run.log.display()
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "autopilot driver did not start; inspect {} and retry",
                run.log.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!(
        "autopilot started (pid {}); follow it with: delegate autopilot status",
        run.driver.pid
    );
    Ok(())
}

fn signal(pid: u32, group: bool) {
    let target = if group {
        format!("-{pid}")
    } else {
        pid.to_string()
    };
    let _ = Command::new("kill")
        .args(["-KILL", "--", &target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn kill_leftover(run: &Run) {
    let Some(process) = &run.lead else {
        return;
    };
    if !fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .is_ok_and(|boot| boot.trim() == process.boot)
    {
        return;
    }
    if let Ok(current) = Process::read(process.pid) {
        // Include zombies: their descendants may still occupy the process group.
        if current.start == process.start {
            signal(process.pid, true);
        }
        return;
    }
    // A group can outlive its leader. Do not signal a recycled numeric PGID:
    // find an actual member carrying this run's private group marker first.
    let marker = format!("_DELEGATE_AUTOPILOT_GROUP={}", run.id);
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(stat) = fs::read_to_string(path.join("stat")) else {
                continue;
            };
            let group = stat
                .rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().nth(2));
            if group.and_then(|group| group.parse::<u32>().ok()) != Some(process.pid) {
                continue;
            }
            if fs::read(path.join("environ")).is_ok_and(|env| {
                env.split(|byte| *byte == 0)
                    .any(|value| value == marker.as_bytes())
            }) {
                signal(process.pid, true);
                return;
            }
        }
    }
}

/// Always kill remaining descendants, even if the lead itself has exited successfully.
struct Group(Child);
impl Drop for Group {
    fn drop(&mut self) {
        signal(self.0.id(), true);
        let _ = self.0.wait();
    }
}

fn run_process(ctx: &Ctx, run: &mut Run, mut command: Command) -> Result<bool> {
    if run.stop_requested() {
        return Ok(false);
    }
    let mut group = Group(command.current_dir(&run.worktree).env("_DELEGATE_AUTOPILOT_GROUP", &run.id).stdin(Stdio::null()).process_group(0)
        .spawn().context("starting autopilot command; inspect autopilot.log and check that claude and sh are on PATH")?);
    run.lead = Some(Process::read(group.0.id())?);
    save(ctx, run)?;
    let result = loop {
        if run.stop_requested() {
            break false;
        }
        if let Some(exit) = group.0.try_wait()? {
            break exit.success();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    drop(group);
    run.lead = None;
    save(ctx, run)?;
    Ok(result)
}

fn merged(run: &Run, since: Option<&str>) -> Result<Vec<String>> {
    let reference = since.map_or_else(|| "autopilot".into(), |since| format!("{since}..autopilot"));
    let subjects = git(
        &run.worktree,
        &["log", "--reverse", "--format=%s", &reference, "--"],
    )?;
    let journal = fs::read_to_string(run.log.join("merges")).unwrap_or_default();
    let mut ids = Vec::new();
    for subject in subjects.lines() {
        if let Some((id, _)) = subject.split_once(": ") {
            let numbered = id.strip_prefix('t').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            });
            if (numbered
                || run
                    .mandate
                    .goals
                    .iter()
                    .chain(&run.task_ids)
                    .any(|goal| goal == id)
                || journal.lines().any(|merged| merged == id))
                && !ids.iter().any(|seen| seen == id)
            {
                ids.push(id.to_string());
            }
        }
    }
    Ok(ids)
}

fn parked(run: &Run) -> Vec<(String, String)> {
    let text = fs::read_to_string(run.worktree.join("PLAN.md")).unwrap_or_default();
    section(&text, "## Waiting for you")
        .into_iter()
        .filter_map(|line| {
            let (id, question) = line.trim().strip_prefix("- ")?.split_once(':')?;
            (valid_id(id.trim()) && !question.trim().is_empty())
                .then(|| (id.trim().into(), question.trim().into()))
        })
        .collect()
}

fn quota_stop(run: &mut Run, report: &quota::AccountReport) -> Option<&'static str> {
    let runway = report.runway;
    if matches!(
        runway.status,
        RunwayStatus::ExhaustedNow | RunwayStatus::ProjectedExhaustion
    ) && u64::try_from(runway.seconds_left).unwrap_or(0) < run.quota_floor
    {
        run.reset = report.reset_at().map(quota::format_clock);
        Some("quota_floor")
    } else {
        None
    }
}

fn wave_stop(run: &Run, gate_passed: bool) -> Result<Option<&'static str>> {
    if !gate_passed || run.log.join("gate.failed").exists() {
        return Ok(Some("gate_failed"));
    }
    let all = merged(run, None)?;
    if run.mandate.goals.iter().all(|goal| all.contains(goal)) {
        return Ok(Some("goal_met"));
    }
    if run.last_wave.is_empty() {
        return Ok(Some(if parked(run).is_empty() {
            "no_progress"
        } else {
            "waiting_for_user"
        }));
    }
    Ok((run.wave >= run.max_waves).then_some("wave_limit"))
}

fn waves(ctx: &Ctx, run: &mut Run) -> Result<&'static str> {
    let cfg = WorkersConfig::load()?;
    for wave in 1..=run.max_waves {
        if run.stop_requested() {
            return Ok("stopped");
        }
        let reports = quota::read_accounts(
            &[Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            }],
            &cfg.quota,
        );
        if let Some(report) = reports.first() {
            let runway = report.runway;
            if let Some(reason) = quota_stop(run, report) {
                return Ok(if run.stop_requested() {
                    "stopped"
                } else {
                    reason
                });
            }
            if runway.status == RunwayStatus::Unknown {
                run.log("Claude runway unknown; continuing as delegate quota selection does")?;
            }
        }
        if run.stop_requested() {
            return Ok("stopped");
        }
        let before = git(&run.worktree, &["rev-parse", "HEAD"])?;
        run.wave = wave;
        save(ctx, run)?;
        run.log(&format!("wave {wave}: starting fresh Claude lead"))?;
        let mut lead = Command::new("claude");
        lead.args([
            "-p",
            &prompts::autopilot(wave),
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            "auto",
        ])
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("DELEGATE_WORKER")
        .env_remove("_DELEGATE_BACKGROUND_RUN")
        .env("DELEGATE_AUTOPILOT", &run.id)
        .env("DELEGATE_AUTOPILOT_WAVE", wave.to_string())
        .stdout(File::create(run.log.join(format!("wave-{wave}.jsonl")))?)
        .stderr(log_file(run)?);
        let success = run_process(ctx, run, lead)?;
        run.log(&format!("wave {wave}: lead exited (success: {success})"))?;
        run.last_wave = merged(run, Some(&before))?;
        if run.stop_requested() {
            return Ok("stopped");
        }
        let mut gate = Command::new("sh");
        gate.args(["-c", &run.mandate.gate])
            .stdout(log_file(run)?)
            .stderr(log_file(run)?);
        let passed = run_process(ctx, run, gate)?;
        if run.stop_requested() {
            return Ok("stopped");
        }
        if let Some(reason) = wave_stop(run, passed)? {
            return Ok(reason);
        }
    }
    Ok("wave_limit")
}

fn drive(ctx: &Ctx, id: &str) -> Result<()> {
    let control = control(ctx)?;
    let mut run = load(ctx)?
        .filter(|run| run.id == id && run.driver.pid == std::process::id())
        .context("this autopilot driver is not authorized; use delegate autopilot start")?;
    let work = Lock::exclusive_now(&home(ctx).join("work.guard"))?;
    drop(control);
    fs::write(run.log.join("ready"), "ready\n")?;
    let result = waves(ctx, &mut run);
    let reason = match result {
        Ok(reason) => reason,
        Err(error) => {
            run.log(&format!("driver error: {error:#}"))?;
            if run.stop_requested() {
                "stopped"
            } else {
                "driver_died"
            }
        }
    };
    kill_leftover(&run);
    drop(work);
    finish(ctx, &mut run, reason)
}

fn proposed(run: &Run) -> Result<Vec<String>> {
    let files = git(
        &run.worktree,
        &[
            "diff",
            "--name-only",
            "--diff-filter=A",
            &run.initial,
            "autopilot",
            "--",
            "docs/decisions",
        ],
    )?;
    let mut decisions = Vec::new();
    for path in files.lines() {
        let text = git(&run.worktree, &["show", &format!("autopilot:{path}")])?;
        if text
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("- Status: proposed"))
        {
            decisions.push(path.into());
        }
    }
    Ok(decisions)
}

fn finish(ctx: &Ctx, run: &mut Run, reason: &str) -> Result<()> {
    let control = control(ctx)?;
    let finalize = Lock::open(&run.log.join("final.guard"), false)?;
    if let Some(current) = load(ctx)? {
        if current.id != run.id {
            return Ok(());
        }
        if current.stop_reason.is_some() {
            *run = current;
            drop(finalize);
            drop(control);
            return notify(run, Duration::from_secs(60));
        }
    }
    let ids = merged(run, Some(&run.initial))?;
    let waiting = parked(run);
    let decisions = proposed(run)?;
    let report = run.log.join("report.md");
    let mut text = format!(
        "# Autopilot run {}\n\nStop reason: `{reason}`\n\n## Merged in this run\n",
        run.id
    );
    for id in &ids {
        writeln!(text, "- {id}")?;
    }
    text.push_str("\n## Waiting for you\n");
    for (id, question) in &waiting {
        writeln!(text, "- {id}: {question}")?;
    }
    text.push_str("\n## Proposed decisions to confirm\n");
    for decision in decisions {
        writeln!(text, "- {decision}")?;
    }
    if reason == "gate_failed" {
        writeln!(
            text,
            "\nGate: `{}`\nMerged in the last wave: {}",
            run.mandate.gate,
            run.last_wave.join(", ")
        )?;
    }
    if reason == "quota_floor" {
        writeln!(
            text,
            "\nClaude account resets: {}",
            run.reset.as_deref().unwrap_or("unknown")
        )?;
    }
    fs::write(&report, text)?;
    // Snapshot counts/ids before releasing the lock: the next run may move the branch.
    let waiting_ids = waiting
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut message = format!(
        "{reason}: {} merged ({}); {} waiting ({waiting_ids})",
        ids.len(),
        ids.join(", "),
        waiting.len()
    );
    if reason == "gate_failed" {
        write!(message, "; gate failed after {}", run.last_wave.join(", "))?;
    }
    if reason == "quota_floor" {
        write!(
            message,
            "; Claude resets {}",
            run.reset.as_deref().unwrap_or("unknown")
        )?;
    }
    fs::write(
        run.log.join("notify.message"),
        message.replace(['\n', '\r'], " "),
    )?;
    run.stop_reason = Some(reason.into());
    run.report = Some(report.clone());
    // Retain a dead driver's identity so cleanup can retry if its group is still exiting.
    if reason != "driver_died" {
        run.lead = None;
    }
    save(ctx, run)?;
    run.log(&format!("stopping: {reason}; report: {}", report.display()))?;
    drop(finalize);
    drop(control);
    notify(run, Duration::from_secs(60))
}

fn notify(run: &Run, timeout: Duration) -> Result<()> {
    if let Some(command) = &run.notify {
        let reason = run
            .stop_reason
            .as_deref()
            .context("autopilot has not stopped")?;
        let report = run.report.as_ref().context("autopilot report is missing")?;
        // Persist the attempt before invoking user code: recovery must not send it twice.
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(run.log.join("notify.attempted"))
        {
            Ok(_) => {
                let message = fs::read_to_string(run.log.join("notify.message"))?;
                let mut notification = Command::new("sh");
                notification
                    .args(["-c", command])
                    .current_dir(&run.worktree)
                    .env("DELEGATE_MESSAGE", message.replace(['\n', '\r'], " "))
                    .env("DELEGATE_TITLE", format!("delegate autopilot: {reason}"))
                    .env("DELEGATE_PROJECT", &run.project)
                    .env("DELEGATE_REPORT", report)
                    .stdin(Stdio::null())
                    .stdout(log_file(run)?)
                    .stderr(log_file(run)?);
                run_notify(run, notification, timeout)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error)
                    .context("recording notify attempt; check log directory permissions");
            }
        }
    }
    Ok(())
}

fn run_notify(run: &Run, mut command: Command, timeout: Duration) -> Result<()> {
    let child = command.process_group(0).spawn();
    let Ok(child) = child else {
        return run.log("notify failed");
    };
    let mut group = Group(child);
    let deadline = Instant::now() + timeout;
    loop {
        match group.0.try_wait() {
            Ok(Some(exit)) => {
                return if exit.success() {
                    Ok(())
                } else {
                    run.log("notify failed")
                };
            }
            Ok(None) => {}
            Err(_) => return run.log("notify failed"),
        }
        if Instant::now() >= deadline {
            drop(group);
            return run.log("notify failed: timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn stop(ctx: &Ctx) -> Result<()> {
    let control_guard = control(ctx)?;
    let Some(mut run) = load(ctx)? else {
        println!("autopilot is not running; start it with: delegate autopilot start");
        return Ok(());
    };
    if run.stop_reason.is_some() {
        kill_leftover(&run);
        run.lead = None;
        save(ctx, &run)?;
        println!("autopilot is not running; inspect delegate autopilot status");
        return Ok(());
    }
    fs::write(run.log.join("stop"), "stopped\n")?;
    kill_leftover(&run);
    drop(control_guard);
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut finalized = false;
    while run.driver.alive() && Instant::now() < deadline {
        finalized =
            load(ctx)?.is_some_and(|current| current.id != run.id || current.stop_reason.is_some());
        if finalized {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // A finalized driver may still be notifying. Leave its timeout/reaper alive.
    if !finalized && run.driver.alive() {
        signal(run.driver.pid, false);
    }
    let control = control(ctx)?;
    if let Some(current) = load(ctx)?
        && current.id == run.id
    {
        run = current;
    }
    kill_leftover(&run);
    drop(control);
    if run.stop_reason.is_none() {
        finish(ctx, &mut run, "stopped")?;
    }
    println!("autopilot stopped; worker tasks are unchanged (delegate list)");
    Ok(())
}

fn show(ctx: &Ctx, json: bool) -> Result<()> {
    // Loading retires orphaned leads; finalization also recovers their report/notify.
    let mut run = load(ctx)?;
    if let Some(previous) = run.as_mut()
        && !previous.driver.alive()
    {
        let reason = previous.stop_reason.clone().unwrap_or_else(|| {
            if previous.stop_requested() {
                "stopped".into()
            } else {
                "driver_died".into()
            }
        });
        finish(ctx, previous, &reason)?;
        run = load(ctx)?;
    }
    let value = if let Some(run) = &run {
        let running = run.running();
        serde_json::json!({
            "running": running, "wave": run.wave, "max_waves": run.max_waves,
            "stop_reason": if running { None } else { Some(run.stop_reason.as_deref().unwrap_or("driver_died")) },
            "report": run.report, "branch": run.branch, "base": run.base,
            "worktree": run.worktree, "log": run.log, "pid": running.then_some(run.driver.pid),
        })
    } else {
        serde_json::json!({"running": false, "wave": 0, "max_waves": 5, "stop_reason": null,
            "report": null, "branch": "autopilot", "base": null,
            "worktree": home(ctx).join("worktree"), "log": null, "pid": null})
    };
    if json {
        println!("{}", serde_json::to_string(&value)?);
    } else if let Some(run) = run {
        println!(
            "autopilot: {} (wave {}/{})\nbranch: {} (base {})\nworktree: {}\nlog: {}\nreport: {}",
            if run.running() {
                "running"
            } else {
                run.stop_reason.as_deref().unwrap_or("driver_died")
            },
            run.wave,
            run.max_waves,
            run.branch,
            run.base,
            run.worktree.display(),
            run.log.display(),
            run.report
                .as_deref()
                .map_or_else(|| "not written yet".into(), |p| p.display().to_string())
        );
    } else {
        println!(
            "autopilot has not run; write ## Mandate in PLAN.md, commit it, then delegate autopilot start"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(extra: &str) -> String {
        format!(
            "# Plan\n\n## Mandate\n- Goal: t001, t002\n- Gate: `scripts/check`\n{extra}\n## Tasks\n| t001 | one |\n| t002 | two |\n"
        )
    }

    #[test]
    fn validates_mandate_where_used() -> Result<()> {
        let mandate = Mandate::parse(&plan("- Max rounds: 2"))?;
        assert_eq!(mandate.goals, ["t001", "t002"]);
        assert_eq!(mandate.gate, "scripts/check");
        assert_eq!(mandate.max_rounds, 2);
        assert_eq!(Mandate::parse(&plan(""))?.max_rounds, 3);
        for bad in [
            plan("").replace("- Goal: t001, t002", "- Goal:"),
            plan("").replace("`scripts/check`", "scripts/check"),
            plan("- Max rounds: 0"),
            plan("- Max rounds: many"),
            plan("").replace("| t002 |", "| t003 |"),
            "# Plan\n".into(),
        ] {
            assert!(Mandate::parse(&bad).is_err(), "accepted {bad}");
        }
        Ok(())
    }

    #[test]
    fn floors_require_positive_checked_durations() -> Result<()> {
        assert_eq!(duration("1h")?, Duration::from_secs(3_600));
        assert_eq!(duration("30m")?, Duration::from_mins(30));
        assert_eq!(duration("2d")?, Duration::from_hours(48));
        for bad in ["", "0h", "-1h", "1", "1.5h", "18446744073709551615d"] {
            assert!(duration(bad).is_err(), "accepted {bad}");
        }
        Ok(())
    }

    #[test]
    fn process_identity_is_not_just_a_pid() -> Result<()> {
        let mut process = Process::read(std::process::id())?;
        assert!(process.alive());
        process.start.push('0');
        assert!(!process.alive());
        Ok(())
    }

    struct Fixture {
        path: PathBuf,
        ctx: Ctx,
        run: Run,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let path =
                std::env::temp_dir().join(format!("delegate-autopilot-{}", uuid::Uuid::new_v4()));
            let root = path.join("repo");
            fs::create_dir_all(&root)?;
            git(&root, &["init", "-q", "-b", "main"])?;
            fs::write(root.join("PLAN.md"), plan(""))?;
            git(&root, &["add", "PLAN.md"])?;
            git(
                &root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-qm",
                    "Plan",
                ],
            )?;
            git(&root, &["branch", "autopilot"])?;
            let ctx = Ctx {
                gitdir: root.join(".git"),
                root: root.clone(),
                dhome: path.join("tasks"),
            };
            let log = home(&ctx).join("runs/test");
            fs::create_dir_all(&log)?;
            let run = Run {
                id: "test-run".into(),
                driver: Process::read(std::process::id())?,
                lead: None,
                wave: 1,
                max_waves: 5,
                stop_reason: None,
                report: None,
                branch: "autopilot".into(),
                base: "main".into(),
                project: "test".into(),
                worktree: root,
                initial: git(&ctx.root, &["rev-parse", "HEAD"])?,
                log,
                mandate: Mandate::parse(&plan("- Max rounds: 2"))?,
                quota_floor: 3_600,
                notify: None,
                reset: None,
                last_wave: Vec::new(),
                task_ids: vec!["t001".into(), "t002".into()],
            };
            save(&ctx, &run)?;
            Ok(Self { path, ctx, run })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    // Isolate environment-dependent checks rather than mutating the parallel test runner's env.
    #[test]
    fn authorization_child() -> Result<()> {
        let Some(path) = std::env::var_os("_AP_TEST_FIXTURE") else {
            return Ok(());
        };
        let path = PathBuf::from(path);
        let ctx = Ctx {
            root: path.join("repo"),
            gitdir: path.join("repo/.git"),
            dhome: path.join("tasks"),
        };
        let expected = std::env::var("_AP_TEST_ALLOWED")? == "yes";
        let result = if std::env::var_os("_AP_TEST_START").is_some() {
            start(&ctx, 5, None)
        } else if std::env::var_os("_AP_TEST_ROUND").is_some() {
            before_run(&ctx, &Task::load(&ctx, "t001")?)
        } else {
            authorize(&ctx).map(drop)
        };
        assert_eq!(result.is_ok(), expected, "{result:?}");
        if std::env::var_os("_AP_TEST_START").is_some() {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("claude is not on PATH"), "{error}");
            assert!(error.contains("install"), "{error}");
        }
        Ok(())
    }

    fn child(fixture: &Fixture, token: Option<&str>, allowed: bool, rounds: bool) -> Result<()> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "autopilot::tests::authorization_child",
                "--nocapture",
            ])
            .env("_AP_TEST_FIXTURE", &fixture.path)
            .env("_AP_TEST_ALLOWED", if allowed { "yes" } else { "no" })
            .env_remove("DELEGATE_AUTOPILOT")
            .env_remove("_AP_TEST_START")
            .env_remove("_AP_TEST_ROUND");
        if let Some(token) = token {
            command.env("DELEGATE_AUTOPILOT", token);
        }
        if rounds {
            command.env("_AP_TEST_ROUND", "1");
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    #[test]
    fn live_run_authorization_and_dead_tokens() -> Result<()> {
        let mut fixture = Fixture::new()?;
        child(&fixture, None, false, false)?;
        child(&fixture, Some("1"), false, false)?;
        child(&fixture, Some("wrong-run"), false, false)?;
        child(&fixture, Some("test-run"), true, false)?;
        fixture.run.driver.start.push('0');
        save(&fixture.ctx, &fixture.run)?;
        child(&fixture, Some("test-run"), false, false)?;
        child(&fixture, None, true, false)?;
        Ok(())
    }

    #[test]
    fn ordinary_authorization_leaves_no_autopilot_footprint() -> Result<()> {
        let fixture = Fixture::new()?;
        fs::remove_dir_all(home(&fixture.ctx))?;
        child(&fixture, None, true, false)?;
        assert!(!home(&fixture.ctx).exists());
        let lease = Lock::directory(&fixture.ctx.dhome, true)?;
        assert!(Lock::directory(&fixture.ctx.dhome, false).is_err());
        drop(lease);
        assert!(Lock::directory(&fixture.ctx.dhome, false).is_ok());
        child(&fixture, Some("stale-run"), false, false)?;
        assert!(!home(&fixture.ctx).exists());
        Ok(())
    }

    #[test]
    fn unreadable_or_corrupt_state_refuses_with_recovery_advice() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = home(&fixture.ctx).join("state.json");
        fs::write(&path, b"not json")?;
        for unreadable in [false, true] {
            if unreadable {
                fs::remove_file(&path)?;
                fs::create_dir(&path)?;
            }
            let error = load(&fixture.ctx).unwrap_err().to_string();
            assert!(error.contains(&path.display().to_string()), "{error}");
            assert!(error.contains("remove it"), "{error}");
            assert!(error.contains("delegate autopilot stop"), "{error}");
            child(&fixture, None, false, false)?;
            assert!(!home(&fixture.ctx).join("control.guard").exists());
            assert!(!home(&fixture.ctx).join("work.guard").exists());
        }
        Ok(())
    }

    #[test]
    fn missing_claude_refuses_start_without_changing_anything() -> Result<()> {
        let fixture = Fixture::new()?;
        let executable = std::env::current_exe()?;
        let run_child = || -> Result<()> {
            let output = Command::new(&executable)
                .args([
                    "--exact",
                    "autopilot::tests::authorization_child",
                    "--nocapture",
                ])
                .env("_AP_TEST_FIXTURE", &fixture.path)
                .env("_AP_TEST_ALLOWED", "no")
                .env("_AP_TEST_START", "1")
                .env("PATH", "")
                .env_remove("DELEGATE_AUTOPILOT")
                .output()?;
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        };
        let path = home(&fixture.ctx).join("state.json");
        let before = fs::read(&path)?;
        let head = git(&fixture.ctx.root, &["rev-parse", "HEAD"])?;
        run_child()?;
        assert_eq!(fs::read(&path)?, before);
        fs::remove_dir_all(home(&fixture.ctx))?;
        run_child()?;
        assert!(!home(&fixture.ctx).exists());
        assert_eq!(git(&fixture.ctx.root, &["rev-parse", "HEAD"])?, head);
        assert_eq!(git(&fixture.ctx.root, &["status", "--porcelain"])?, "");
        Ok(())
    }

    #[test]
    fn round_limit_only_counts_product_failures_and_gate_is_sticky() -> Result<()> {
        use crate::task::{Checks, Round};
        let fixture = Fixture::new()?;
        let mut task = Task::load(&fixture.ctx, "t001")?;
        fs::create_dir_all(&task.dir)?;
        let failing = Round {
            number: 1,
            tier: None,
            worker: None,
            status: RoundStatus::Done,
            research: false,
            exit: Some(0),
            quota: false,
            checks: Some(Checks {
                outcome: CheckOutcome::Fail,
                commit: "test".into(),
            }),
        };
        task.state.rounds = vec![
            failing.clone(),
            Round {
                quota: true,
                ..failing.clone()
            },
            Round {
                research: true,
                ..failing.clone()
            },
            Round {
                checks: Some(Checks {
                    outcome: CheckOutcome::CheckError,
                    commit: "test".into(),
                }),
                ..failing.clone()
            },
        ];
        task.save()?;
        child(&fixture, Some("test-run"), true, true)?;
        task.state.rounds.push(failing);
        task.save()?;
        child(&fixture, Some("test-run"), false, true)?;
        task.state.rounds.clear();
        task.save()?;
        fs::write(fixture.run.log.join("gate.failed"), "t002")?;
        child(&fixture, Some("test-run"), false, true)?;
        Ok(())
    }

    #[test]
    fn open_specs_are_not_foreign_but_implemented_tasks_are() -> Result<()> {
        let fixture = Fixture::new()?;
        let mut task = Task::load(&fixture.ctx, "t001")?;
        fs::create_dir_all(&task.dir)?;
        fs::write(&task.spec, "# Task t001: test")?;
        assert!(foreign_tasks(&fixture.ctx).is_ok());
        task.state.base = Some("main".into());
        task.save()?;
        assert!(
            foreign_tasks(&fixture.ctx)
                .unwrap_err()
                .to_string()
                .contains("t001")
        );
        task.state.base = Some("autopilot".into());
        task.save()?;
        assert!(foreign_tasks(&fixture.ctx).is_ok());
        Ok(())
    }

    #[test]
    fn worktree_reuse_checks_ancestry_without_touching_main() -> Result<()> {
        let fixture = Fixture::new()?;
        let before = git(&fixture.ctx.root, &["rev-parse", "main"])?;
        let worktree = prepare_worktree(&fixture.ctx, "main")?;
        git(
            &worktree,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "t001: one",
            ],
        )?;
        let unlanded = git(&worktree, &["rev-parse", "HEAD"])?;
        prepare_worktree(&fixture.ctx, "main")?;
        assert_eq!(git(&worktree, &["rev-parse", "HEAD"])?, unlanded);
        let handoff = format!(
            "{}\n## Waiting for you\n- t002: choose rounding\n",
            plan("")
        );
        fs::write(worktree.join("PLAN.md"), &handoff)?;
        prepare_worktree(&fixture.ctx, "main")?;
        assert_eq!(fs::read_to_string(worktree.join("PLAN.md"))?, handoff);
        assert_eq!(git(&fixture.ctx.root, &["rev-parse", "main"])?, before);
        git(
            &fixture.ctx.root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "User progress",
            ],
        )?;
        assert!(
            prepare_worktree(&fixture.ctx, "main")
                .unwrap_err()
                .to_string()
                .contains("land or discard autopilot first")
        );
        // Once autopilot is an ancestor again, reset to the base's new HEAD.
        git(&worktree, &["reset", "-q", "--hard", "main"])?;
        git(
            &fixture.ctx.root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "New base",
            ],
        )?;
        prepare_worktree(&fixture.ctx, "main")?;
        assert_eq!(
            git(&worktree, &["rev-parse", "HEAD"])?,
            git(&fixture.ctx.root, &["rev-parse", "main"])?
        );
        Ok(())
    }

    #[test]
    fn successful_lead_cannot_leave_descendants_running() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60 & echo $! > descendant.pid"]);
        assert!(run_process(&fixture.ctx, &mut fixture.run, command)?);
        let pid: u32 = fs::read_to_string(fixture.ctx.root.join("descendant.pid"))?
            .trim()
            .parse()?;
        if let Ok(process) = Process::read(pid) {
            assert_dead(&process);
        }
        Ok(())
    }

    #[test]
    fn a_late_stop_cannot_overwrite_a_restarted_run() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let mut replacement = fixture.run.clone();
        replacement.id = "new-run".into();
        save(&fixture.ctx, &replacement)?;
        finish(&fixture.ctx, &mut fixture.run, "stopped")?;
        let current = load(&fixture.ctx)?.unwrap();
        assert_eq!(current.id, "new-run");
        assert!(current.running());
        assert!(!fixture.run.log.join("report.md").exists());
        Ok(())
    }

    #[test]
    fn interactive_operation_lease_excludes_startup() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = home(&fixture.ctx).join("work.guard");
        let lease = Lock::open(&path, true)?;
        assert!(Lock::exclusive_now(&path).is_err());
        drop(lease);
        assert!(Lock::exclusive_now(&path).is_ok());
        Ok(())
    }

    #[test]
    fn recovery_kills_a_group_even_after_its_leader_has_exited() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let command = Command::new("sh")
            .args(["-c", "sleep 60 & echo $! > orphan.pid"])
            .current_dir(&fixture.ctx.root)
            .env("_DELEGATE_AUTOPILOT_GROUP", &fixture.run.id)
            .process_group(0)
            .spawn()?;
        let mut group = Group(command);
        fixture.run.lead = Some(Process::read(group.0.id())?);
        group.0.wait()?;
        let pid: u32 = fs::read_to_string(fixture.ctx.root.join("orphan.pid"))?
            .trim()
            .parse()?;
        let orphan = Process::read(pid)?;
        fixture.run.id = "not-this-group".into();
        kill_leftover(&fixture.run);
        assert!(orphan.alive(), "a foreign group was killed");
        fixture.run.id = "test-run".into();
        kill_leftover(&fixture.run);
        let deadline = Instant::now() + Duration::from_secs(2);
        while orphan.alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!orphan.alive());
        Ok(())
    }

    fn orphan_lead(fixture: &mut Fixture) -> Result<Group> {
        let child = Command::new("sleep")
            .arg("60")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let group = Group(child);
        fixture.run.driver.start.push('0');
        fixture.run.lead = Some(Process::read(group.0.id())?);
        save(&fixture.ctx, &fixture.run)?;
        Ok(group)
    }

    fn assert_dead(process: &Process) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while process.alive() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!process.alive(), "process {} survived cleanup", process.pid);
    }

    #[test]
    fn dead_driver_authorization_retires_its_lead_even_with_a_stale_token() -> Result<()> {
        for token in [None, Some("test-run")] {
            let mut fixture = Fixture::new()?;
            let _group = orphan_lead(&mut fixture)?;
            child(&fixture, token, token.is_none(), false)?;
            assert_dead(fixture.run.lead.as_ref().unwrap());
        }
        Ok(())
    }

    #[test]
    fn death_recovery_reports_once_and_retires_orphan() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let _group = orphan_lead(&mut fixture)?;
        // Persist a proposed decision and a parked question in the autopilot worktree.
        git(&fixture.ctx.root, &["checkout", "-q", "autopilot"])?;
        fs::create_dir_all(fixture.ctx.root.join("docs/decisions"))?;
        fs::write(
            fixture.ctx.root.join("docs/decisions/999-proposal.md"),
            "# Proposal\n- Status: proposed\n",
        )?;
        fs::write(
            fixture.ctx.root.join("PLAN.md"),
            format!(
                "{}\n## Waiting for you\n- t002: which rounding?\n",
                plan("")
            ),
        )?;
        git(&fixture.ctx.root, &["add", "."])?;
        git(
            &fixture.ctx.root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "t001: done",
            ],
        )?;
        fixture.run.notify =
            Some("printf '%s\\n' \"$DELEGATE_MESSAGE\" >> notification.log".into());
        save(&fixture.ctx, &fixture.run)?;
        show(&fixture.ctx, true)?;
        assert_dead(fixture.run.lead.as_ref().unwrap());
        show(&fixture.ctx, true)?;
        let run = load(&fixture.ctx)?.unwrap();
        assert_eq!(run.stop_reason.as_deref(), Some("driver_died"));
        assert!(run.lead.is_some());
        let report = fs::read_to_string(run.report.unwrap())?;
        for expected in [
            "driver_died",
            "t001",
            "t002: which rounding?",
            "999-proposal.md",
        ] {
            assert!(report.contains(expected));
        }
        let messages = fs::read_to_string(fixture.ctx.root.join("notification.log"))?;
        assert_eq!(messages.lines().count(), 1);
        assert!(!messages.contains("rounding"));
        assert!(!messages.contains("proposal.md"));
        Ok(())
    }

    #[test]
    fn wave_stop_order_is_gate_then_goal_then_progress_then_limit() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.run.wave = fixture.run.max_waves;
        assert_eq!(wave_stop(&fixture.run, false)?, Some("gate_failed"));
        assert_eq!(wave_stop(&fixture.run, true)?, Some("no_progress"));
        fs::write(
            fixture.ctx.root.join("PLAN.md"),
            format!(
                "{}\n## Waiting for you\n- t002: choose rounding\n",
                plan("")
            ),
        )?;
        assert_eq!(wave_stop(&fixture.run, true)?, Some("waiting_for_user"));
        fixture.run.last_wave = vec!["t001".into()];
        assert_eq!(wave_stop(&fixture.run, true)?, Some("wave_limit"));
        fixture.run.wave = 1;
        assert_eq!(wave_stop(&fixture.run, true)?, None);
        git(&fixture.ctx.root, &["checkout", "-q", "autopilot"])?;
        for id in ["t001", "t002"] {
            git(
                &fixture.ctx.root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    &format!("{id}: done"),
                ],
            )?;
        }
        fixture.run.last_wave.clear();
        fixture.run.wave = fixture.run.max_waves;
        assert_eq!(wave_stop(&fixture.run, true)?, Some("goal_met"));
        assert_eq!(wave_stop(&fixture.run, false)?, Some("gate_failed"));
        fs::write(fixture.run.log.join("gate.failed"), "t001\n")?;
        assert_eq!(wave_stop(&fixture.run, true)?, Some("gate_failed"));
        Ok(())
    }

    #[test]
    fn quota_floor_stops_below_but_not_at_floor_and_records_reset() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let reset = 1_800_000_000;
        let mut report = quota::AccountReport::from_windows(
            Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            },
            vec![quota::Window {
                name: "test".into(),
                percent_left: 0.0,
                resets_at: Some(reset),
                seconds: 5 * 3_600,
                scope: Vec::new(),
                no_reset: false,
            }],
            None,
            None,
        );
        for (status, seconds, expected) in [
            (RunwayStatus::ExhaustedNow, 0, Some("quota_floor")),
            (RunwayStatus::ProjectedExhaustion, -1, Some("quota_floor")),
            (
                RunwayStatus::ProjectedExhaustion,
                3_599,
                Some("quota_floor"),
            ),
            (RunwayStatus::ProjectedExhaustion, 3_600, None),
            (RunwayStatus::ProjectedExhaustion, 3_601, None),
            (RunwayStatus::ThroughReset, 1, None),
            (RunwayStatus::Unknown, 0, None),
        ] {
            report.runway.status = status;
            report.runway.seconds_left = seconds;
            assert_eq!(quota_stop(&mut fixture.run, &report), expected);
        }
        assert_eq!(fixture.run.reset, Some(quota::format_clock(reset)));
        fixture.run.wave = 0;
        finish(&fixture.ctx, &mut fixture.run, "quota_floor")?;
        let run = load(&fixture.ctx)?.unwrap();
        assert_eq!(run.wave, 0);
        assert_eq!(run.stop_reason.as_deref(), Some("quota_floor"));
        let text = fs::read_to_string(run.report.unwrap())?;
        assert!(text.contains(&quota::format_clock(reset)));
        Ok(())
    }

    #[test]
    fn notification_child() -> Result<()> {
        let Some(path) = std::env::var_os("_AP_TEST_NOTIFICATION") else {
            return Ok(());
        };
        let path = PathBuf::from(path);
        let ctx = Ctx {
            root: path.join("repo"),
            gitdir: path.join("repo/.git"),
            dhome: path.join("tasks"),
        };
        let run = load(&ctx)?.unwrap();
        assert!(!run.running());
        assert_eq!(run.stop_reason.as_deref(), Some("no_progress"));
        assert!(run.report.as_ref().unwrap().is_file());
        assert_eq!(
            std::env::var_os("DELEGATE_REPORT"),
            run.report.as_ref().map(|p| p.as_os_str().to_owned())
        );
        let _control = Lock::exclusive_now(&home(&ctx).join("control.guard"))?;
        let _work = Lock::exclusive_now(&home(&ctx).join("work.guard"))?;
        fs::write(run.log.join("notify.checked"), "checked\n")?;
        Ok(())
    }

    #[test]
    fn final_state_and_unlocked_guards_precede_failing_notify() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.run.notify = Some(format!(
            "_AP_TEST_NOTIFICATION='{}' '{}' --exact autopilot::tests::notification_child --nocapture; exit 1",
            fixture.path.display(),
            std::env::current_exe()?.display(),
        ));
        finish(&fixture.ctx, &mut fixture.run, "no_progress")?;
        assert!(fixture.run.log.join("notify.checked").is_file());
        let run = load(&fixture.ctx)?.unwrap();
        assert!(!run.running());
        assert_eq!(run.stop_reason.as_deref(), Some("no_progress"));
        assert!(run.report.unwrap().is_file());
        assert!(
            fs::read_to_string(fixture.run.log.join("autopilot.log"))?.contains("notify failed")
        );
        finish(&fixture.ctx, &mut fixture.run, "stopped")?;
        assert_eq!(
            load(&fixture.ctx)?.unwrap().stop_reason.as_deref(),
            Some("no_progress")
        );
        Ok(())
    }

    #[test]
    fn hanging_notify_times_out_and_kills_descendants_without_changing_state() -> Result<()> {
        let mut fixture = Fixture::new()?;
        finish(&fixture.ctx, &mut fixture.run, "no_progress")?;
        fixture.run.notify = Some("sleep 60 & echo $! > notify.pid; wait".into());
        let before = fs::read(home(&fixture.ctx).join("state.json"))?;
        let started = Instant::now();
        notify(&fixture.run, Duration::from_millis(300))?;
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid: u32 = fs::read_to_string(fixture.ctx.root.join("notify.pid"))?
            .trim()
            .parse()?;
        if let Ok(process) = Process::read(pid) {
            assert_dead(&process);
        }
        assert!(
            fs::read_to_string(fixture.run.log.join("autopilot.log"))?
                .contains("notify failed: timed out")
        );
        assert_eq!(fs::read(home(&fixture.ctx).join("state.json"))?, before);
        // The same run cannot notify twice, even after a timeout.
        fixture.run.notify = Some("touch unexpected-notify".into());
        notify(&fixture.run, Duration::from_millis(300))?;
        assert!(!fixture.ctx.root.join("unexpected-notify").exists());
        Ok(())
    }

    #[test]
    fn autopilot_settings_are_optional() -> Result<()> {
        let defaults: WorkersConfig = toml::from_str(crate::config::WORKERS_EXAMPLE)?;
        assert!(defaults.autopilot.notify.is_none());
        let cfg: WorkersConfig = toml::from_str(&format!(
            "{}\n[autopilot]\nquota_floor = '2h'\nnotify = 'notify-send hello'\n",
            crate::config::WORKERS_EXAMPLE
        ))?;
        assert_eq!(cfg.autopilot.quota_floor.as_deref(), Some("2h"));
        assert_eq!(cfg.autopilot.notify.as_deref(), Some("notify-send hello"));
        Ok(())
    }
}
