//! Shared live-state and read-only task views for status, watch and statusline.
//!
//! `collect(Depth::Full)` includes diffs and the log tail; `Depth::Phase` omits
//! those expensive details. Neither depth writes files or refreshes live state.
//! Renderers use colour only when explicitly enabled for terminal output. `is_alive`
//! verifies a Linux process identity, not just the existence of its pid.
//! Readers never lock files: `operation.guard` is writer-only exclusion, while
//! `operation.lock` is an atomic owner record (marked done before unlocking).

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{IsTerminal, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Worker;
use crate::task::{CheckOutcome, Ctx, RoundStatus, Task, git};
use crate::{commands, spec};

/// How much work a view collector may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Full,
    // Constructed by t011/t012; keep the parallel-task interface available now.
    #[allow(dead_code)]
    Phase,
}

/// The public JSON task object. Optional facts are null until known.
/// `log_tail`, `elapsed_secs` and `activity_age_secs` are renderer-only data.
#[derive(Debug, Serialize)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub session: Option<String>,
    pub session_name: String,
    pub phase: String,
    pub outcome: Option<String>,
    pub worker: Option<String>,
    pub tier: String,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub stalled: bool,
    pub changed_files: Vec<ChangedFile>,
    pub steps: Vec<serde_json::Value>,
    pub steps_available: bool,
    pub ready_to_merge: bool,
    pub merge_blockers: Vec<String>,
    pub next: Option<String>,
    pub log: String,
    #[serde(skip)]
    pub log_tail: String,
    #[serde(skip)]
    pub elapsed_secs: u64,
    #[serde(skip)]
    pub activity_age_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub added: u64,
    pub removed: u64,
}

/// Persisted operation identity; start ticks are Linux /proc/stat field 22.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveState {
    pub phase: String,
    pub worker: Option<String>,
    pub tier: String,
    pub candidate: String,
    pub pid: u32,
    pub pid_start: String,
    pub boot_id: String,
    pub started_at: String,
    pub heartbeat_at: String,
}

fn process_stat(pid: u32) -> Option<(String, String)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces and parentheses; fields after its final ')' start at 3.
    let fields: Vec<_> = text.rsplit_once(')')?.1.split_whitespace().collect();
    Some((fields.first()?.to_string(), fields.get(19)?.to_string()))
}

/// Alive only on this boot, with the same start ticks, and not a zombie/dead process.
pub fn is_alive(pid: u32, pid_start: &str, boot_id: &str) -> bool {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .is_ok_and(|boot| boot.trim() == boot_id.trim())
        && process_stat(pid).is_some_and(|(state, start)| {
            state != "Z" && state != "X" && state != "x" && start == pid_start
        })
}

impl LiveState {
    pub fn alive(&self) -> bool {
        self.phase != "done" && is_alive(self.pid, &self.pid_start, &self.boot_id)
    }

    fn current(phase: &str) -> Result<Self> {
        let pid = std::process::id();
        let (_, pid_start) = process_stat(pid).context("reading runner identity from /proc")?;
        let now = timestamp(now_secs());
        Ok(Self {
            phase: phase.into(),
            worker: None,
            tier: "custom".into(),
            candidate: "0/0".into(),
            pid,
            pid_start,
            boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                .trim()
                .into(),
            started_at: now.clone(),
            heartbeat_at: now,
        })
    }
}

// Preserve the shared reader API for parallel adapter/view tasks.
#[allow(clippy::unnecessary_wraps)]
pub fn read_live(t: &Task) -> Result<Option<LiveState>> {
    // Live state is best-effort metadata, not the durable round history. A stale,
    // unreadable or older-format file must not prevent viewing or cleaning a task.
    Ok(read_owner(&t.dir.join("status.json")))
}

fn read_owner(path: &Path) -> Option<LiveState> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn atomic_live(path: &Path, state: &LiveState) -> Result<()> {
    let pending = path.with_file_name(format!(".status-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        fs::write(&pending, serde_json::to_vec(state)?)?;
        fs::rename(&pending, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}

fn busy(id: &str, state: &LiveState) -> anyhow::Error {
    anyhow::anyhow!(
        "{id} is running ({}, pid {}); wait for it, or stop it with: kill {}",
        state.phase,
        state.pid,
        state.pid
    )
}

/// Read-only refusal check, also used by merge readiness.
pub fn ensure_idle(t: &Task) -> Result<()> {
    if let Some(live) = read_live(t)?
        && live.alive()
    {
        return Err(busy(&t.id, &live));
    }
    // Merge/discard and preflight have an owner record without replacing the last
    // run's live state. Readers never lock: even a brief shared lock can make a
    // healthy starting writer's nonblocking exclusive acquisition fail.
    if let Some(owner) = read_owner(&t.dir.join("operation.lock"))
        && owner.alive()
    {
        return Err(busy(&t.id, &owner));
    }
    Ok(())
}

/// Explicitly unlock before closing: a concurrent fork can temporarily inherit a
/// CLOEXEC descriptor, and closing only our copy would leave that lock held.
#[derive(Debug)]
struct OperationLock {
    file: File,
    owner: Option<(std::path::PathBuf, LiveState)>,
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        if let Some((path, owner)) = self.owner.as_mut() {
            owner.phase = "done".into();
            // Merge/discard may already have removed the entire task directory.
            if path.parent().is_some_and(Path::is_dir)
                && let Err(error) = atomic_live(path, owner)
            {
                eprintln!("warning: finishing task operation: {error:#}");
            }
        }
        let _ = self.file.unlock();
    }
}

/// Only an exclusive writer may remove leftovers from a killed atomic write.
fn clean_pending(t: &Task) -> Result<()> {
    for entry in fs::read_dir(&t.dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (name.starts_with(".status-") || name.starts_with(".STATE-"))
            && name.ends_with(".tmp")
            && entry.file_type()?.is_file()
        {
            fs::remove_file(entry.path()).with_context(|| {
                format!(
                    "removing stale {}; check permissions and retry",
                    entry.path().display()
                )
            })?;
        }
    }
    Ok(())
}

/// Exclusive per-task operation. Kernel locks survive neither crashes nor reboot.
/// Once started, the guard heartbeats and writes done on all ordinary return paths.
#[derive(Debug)]
pub struct Operation {
    _lock: OperationLock,
    path: std::path::PathBuf,
    live: Arc<Mutex<LiveState>>,
    stop: Option<mpsc::Sender<()>>,
    heartbeat: Option<JoinHandle<()>>,
    started: bool,
}

impl Operation {
    pub fn acquire(t: &Task, phase: &str) -> Result<Self> {
        ensure_idle(t)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            // Keep writer exclusion separate from the readable owner record.
            // Legacy shared readers of operation.lock cannot contend with this.
            .open(t.dir.join("operation.guard"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => {
                ensure_idle(t)?;
                bail!("{} has another operation starting; wait and retry", t.id);
            }
            Err(fs::TryLockError::Error(error)) => {
                return Err(error).context("locking task operation; check permissions and retry");
            }
        }
        let mut lock = OperationLock {
            file: lock,
            owner: None,
        };
        // A legacy runner may have no lock file. Check its identity again after locking.
        if let Some(live) = read_live(t)?
            && live.alive()
        {
            return Err(busy(&t.id, &live));
        }
        clean_pending(t)?;
        let live = LiveState::current(phase)?;
        let owner_path = t.dir.join("operation.lock");
        atomic_live(&owner_path, &live)?;
        lock.owner = Some((owner_path, live.clone()));
        Ok(Self {
            _lock: lock,
            path: t.dir.join("status.json"),
            live: Arc::new(Mutex::new(live)),
            stop: None,
            heartbeat: None,
            started: false,
        })
    }

    pub fn start(
        &mut self,
        phase: &str,
        worker: Option<&str>,
        tier: &str,
        candidate: &str,
    ) -> Result<()> {
        self.update(phase, worker, tier, candidate)?;
        if self.started {
            return Ok(());
        }
        self.started = true;
        let (send, receive) = mpsc::channel();
        self.stop = Some(send);
        let path = self.path.clone();
        let live = Arc::clone(&self.live);
        self.heartbeat = Some(std::thread::spawn(move || {
            while receive.recv_timeout(Duration::from_secs(1))
                == Err(mpsc::RecvTimeoutError::Timeout)
            {
                if let Ok(mut state) = live.lock() {
                    state.heartbeat_at = timestamp(now_secs());
                    if let Err(error) = atomic_live(&path, &state) {
                        eprintln!("warning: updating live status: {error:#}");
                    }
                }
            }
        }));
        Ok(())
    }

    pub fn update(
        &self,
        phase: &str,
        worker: Option<&str>,
        tier: &str,
        candidate: &str,
    ) -> Result<()> {
        let mut state = self
            .live
            .lock()
            .map_err(|_| anyhow::anyhow!("live state lock poisoned"))?;
        state.phase = phase.into();
        state.worker = worker.map(str::to_owned);
        state.tier = tier.into();
        state.candidate = candidate.into();
        state.heartbeat_at = timestamp(now_secs());
        atomic_live(&self.path, &state)
    }

    pub fn phase(&self, phase: &str) -> Result<()> {
        let mut state = self
            .live
            .lock()
            .map_err(|_| anyhow::anyhow!("live state lock poisoned"))?;
        state.phase = phase.into();
        state.heartbeat_at = timestamp(now_secs());
        atomic_live(&self.path, &state)
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.heartbeat.take() {
            let _ = thread.join();
        }
        if self.started
            && let Err(error) = self.phase("done")
        {
            eprintln!("warning: finishing live status: {error:#}");
        }
    }
}

pub fn task_ids(ctx: &Ctx) -> Vec<String> {
    let mut ids: Vec<_> = fs::read_dir(&ctx.dhome)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().join("SPEC.md").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| !id.starts_with(['_', '.']))
        .collect();
    ids.sort();
    ids
}

/// Workers on this checkout's live tasks, excluding the task being started.
/// Best-effort metadata only: stale or unreadable tasks never reserve quota.
pub fn running_workers(ctx: &Ctx, exclude: &str) -> Vec<(String, Worker)> {
    task_ids(ctx)
        .into_iter()
        .filter(|id| id != exclude)
        .filter_map(|id| {
            let task = Task::load(ctx, &id).ok()?;
            let live = read_live(&task).ok()??;
            if !live.alive()
                || !matches!(
                    live.phase.as_str(),
                    "questions" | "implement" | "checks" | "review"
                )
            {
                return None;
            }
            let worker = live
                .worker
                .as_deref()
                .or_else(|| task.last_round()?.worker.as_deref())?
                .parse()
                .ok()?;
            Some((id, worker))
        })
        .collect()
}

/// Collect task snapshots without modifying the task, repository, index or worktree.
pub fn collect(ctx: &Ctx, id: Option<&str>, depth: Depth) -> Result<Vec<TaskView>> {
    let ids = task_ids(ctx);
    let selected = if let Some(id) = id {
        if !ids.iter().any(|known| known == id) {
            bail!(
                "unknown task '{id}'; known tasks: {}",
                if ids.is_empty() {
                    "none".into()
                } else {
                    ids.join(", ")
                }
            );
        }
        vec![id.to_owned()]
    } else {
        ids
    };
    selected
        .iter()
        .map(|selected_id| {
            let task = Task::load(ctx, selected_id)?;
            let mut view = collect_task(ctx, &task, depth)?;
            let mut steps = crate::harness::events::collect(&task.dir.join("events.jsonl"));
            view.steps_available = !steps.is_empty();
            let step_activity = steps
                .iter()
                .filter_map(|step| step["at"].as_str().and_then(parse_timestamp))
                .max()
                .unwrap_or(0);
            let activity = view
                .last_activity_at
                .as_deref()
                .and_then(parse_timestamp)
                .unwrap_or(0);
            if step_activity > activity {
                view.last_activity_at = Some(timestamp(step_activity));
                view.activity_age_secs = now_secs().saturating_sub(step_activity);
                let threshold = std::env::var("DELEGATE_STALL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300);
                view.stalled = view.pid.is_some()
                    && matches!(view.phase.as_str(), "questions" | "implement")
                    && view.activity_age_secs > threshold;
            }
            if id.is_none() && steps.len() > 5 {
                steps.drain(..steps.len() - 5);
            }
            view.steps = steps;
            // Stream steps can clear a filesystem-based stall; derive the live
            // hint only after both sources of activity have been considered.
            if let Some(pid) = view.pid {
                view.next = view
                    .stalled
                    .then(|| format!("delegate status {}  ·  kill {pid}", view.id));
            }
            Ok(view)
        })
        .collect()
}

/// Resolve live phases before round outcomes, including interrupted check/review.
pub fn phase(t: &Task, live: Option<&LiveState>) -> String {
    if let Some(live) = live
        && live.phase != "done"
    {
        return if live.alive() {
            live.phase.clone()
        } else {
            "interrupted".into()
        };
    }
    match t.last_round().map(|r| r.status) {
        None => "not_started",
        Some(RoundStatus::Running) => "interrupted",
        Some(_) => "done",
    }
    .into()
}

fn collect_task(ctx: &Ctx, t: &Task, depth: Depth) -> Result<TaskView> {
    let live = read_live(t)?;
    let phase = phase(t, live.as_ref());
    let alive = live.as_ref().is_some_and(LiveState::alive);
    let active = live.as_ref().filter(|s| s.phase != "done");
    let round = t.last_round();
    let outcome = if phase == "done" {
        round.map(|r| {
            match r.status {
                RoundStatus::Failed => "failed_to_start",
                RoundStatus::Questions => "questions",
                RoundStatus::Done if r.quota => "quota",
                RoundStatus::Done if r.research => "research",
                RoundStatus::Done => match r.checks.as_ref().map(|c| c.outcome) {
                    Some(CheckOutcome::Pass) => "pass",
                    Some(CheckOutcome::Fail) => "fail",
                    Some(CheckOutcome::CheckError) => "check error",
                    Some(CheckOutcome::NoCommands) => "no_checks",
                    None => "no_changes",
                },
                RoundStatus::Running => unreachable!("running round is interrupted"),
            }
            .into()
        })
    } else {
        None
    };
    let started_at = live.as_ref().map(|s| s.started_at.clone());
    let start = started_at.as_deref().and_then(parse_timestamp).unwrap_or(0);
    let now = now_secs();
    let activity = latest_mtime(&t.wt).max(start);
    let stall_secs = std::env::var("DELEGATE_STALL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let readiness = commands::merge_readiness(ctx, t, false, None);
    let ready_to_merge = readiness.blockers.is_empty();
    let next = if alive {
        None
    } else {
        match (phase.as_str(), outcome.as_deref()) {
            ("interrupted", _) => Some(format!(
                "delegate run {} or delegate discard {}",
                t.id, t.id
            )),
            (_, Some("questions")) => Some(format!(
                "answer the questions in the spec, then delegate run {}",
                t.id
            )),
            (_, Some("pass")) if ready_to_merge => Some(format!("delegate merge {}", t.id)),
            (_, Some("pass" | "check error")) => readiness.next,
            (_, Some("quota")) => Some(format!(
                "delegate run {} (retry after the quota resets)",
                t.id
            )),
            (_, Some("research" | "no_changes")) => Some(format!("delegate discard {}", t.id)),
            _ => Some(format!("delegate run {}", t.id)),
        }
    };
    let log = t.dir.join("run.log");
    let log_tail = if depth == Depth::Full {
        tail(&log)
    } else {
        String::new()
    };
    let spec_text = fs::read_to_string(&t.spec).unwrap_or_default();
    let owner = t.state.owner.as_ref();
    Ok(TaskView {
        id: t.id.clone(),
        title: spec::display_title(&spec_text, &t.id),
        goal: spec::goal(&spec_text)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string(),
        session: owner.map(|owner| owner.session.clone()),
        session_name: owner.map_or_else(
            || "by hand".into(),
            |owner| {
                Path::new(&owner.dir)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        ),
        phase,
        outcome,
        worker: active
            .and_then(|s| s.worker.clone())
            .or_else(|| round.and_then(|r| r.worker.clone())),
        tier: active.map_or_else(
            || round.map_or_else(|| "custom".into(), crate::task::Round::tier_label),
            |s| s.tier.clone(),
        ),
        pid: live.as_ref().filter(|s| s.alive()).map(|s| s.pid),
        started_at,
        last_activity_at: (activity != 0).then(|| timestamp(activity)),
        stalled: alive
            && live
                .as_ref()
                .is_some_and(|s| matches!(s.phase.as_str(), "questions" | "implement"))
            && now.saturating_sub(activity) > stall_secs,
        changed_files: if depth == Depth::Full && t.wt.join(".git").exists() {
            // Checkout/cleanup and worker git commands can race this snapshot.
            changed_files(ctx, t).unwrap_or_default()
        } else {
            vec![]
        },
        steps: vec![],
        steps_available: false,
        ready_to_merge,
        merge_blockers: readiness.blockers,
        next,
        log: log.display().to_string(),
        log_tail,
        elapsed_secs: if start == 0 {
            0
        } else {
            let end = live
                .as_ref()
                .filter(|s| s.phase == "done")
                .and_then(|s| parse_timestamp(&s.heartbeat_at))
                .unwrap_or(now);
            end.saturating_sub(start)
        },
        activity_age_secs: if activity == 0 {
            0
        } else {
            now.saturating_sub(activity)
        },
    })
}

fn changed_files(ctx: &Ctx, t: &Task) -> Result<Vec<ChangedFile>> {
    let base = git(&t.wt, &["merge-base", &t.base(ctx)?, "HEAD"])?;
    let diff = git(
        &t.wt,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--numstat",
            "-z",
            &base,
            "--",
            ".",
            ":(exclude).task/**",
        ],
    )?;
    let mut files = Vec::new();
    for entry in diff.split('\0').filter(|s| !s.is_empty()) {
        let mut fields = entry.splitn(3, '\t');
        let added = fields.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let removed = fields.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        if let Some(path) = fields.next() {
            files.push(ChangedFile {
                path: path.into(),
                added,
                removed,
            });
        }
    }
    for path in git(&t.wt, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty() && !p.starts_with(".task/"))
    {
        let file = t.wt.join(path);
        let added = match fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_symlink() => 1,
            Ok(meta) if meta.is_file() => {
                let bytes = fs::read(&file).unwrap_or_default();
                if bytes.contains(&0) {
                    0
                } else {
                    bytes.split_inclusive(|b| *b == b'\n').count() as u64
                }
            }
            // Never open a FIFO/device or follow an untracked symlink.
            _ => 0,
        };
        files.push(ChangedFile {
            path: path.into(),
            added,
            removed: 0,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn latest_mtime(root: &Path) -> u64 {
    let mut latest = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            if entry.file_name() == ".git" || entry.file_name() == ".task" {
                continue;
            }
            if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                if meta.is_dir() {
                    pending.push(entry.path());
                } else if let Ok(time) = meta.modified() {
                    latest = latest.max(
                        time.duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    );
                }
            }
        }
    }
    latest
}

#[allow(clippy::naive_bytecount)] // Small chunks; the task explicitly forbids new dependencies.
fn tail(path: &Path) -> String {
    let read = || -> Result<String> {
        let mut file = File::open(path)?;
        let mut cursor = file.metadata()?.len();
        let mut chunks = Vec::new();
        let mut newlines = 0;
        // Read backwards, only as far as five complete lines. Snapshot the length
        // so a continuously chattering worker cannot keep this reader busy forever.
        while cursor > 0 && newlines <= 5 {
            let size = usize::try_from(cursor.min(8192))?;
            cursor -= size as u64;
            file.seek(SeekFrom::Start(cursor))?;
            let mut chunk = vec![0; size];
            file.read_exact(&mut chunk)?;
            newlines += chunk.iter().filter(|b| **b == b'\n').count();
            chunks.push(chunk);
        }
        let bytes: Vec<_> = chunks.into_iter().rev().flatten().collect();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<_> = text.lines().rev().take(5).collect();
        Ok(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
    };
    read().unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Gregorian civil-date conversion (400-year eras); UTC, no locale or external programs.
fn timestamp(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86400).unwrap_or(i64::MAX / 2) + 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn parse_timestamp(s: &str) -> Option<u64> {
    let parts: Vec<i64> = s
        .trim_end_matches('Z')
        .split(['-', 'T', ':'])
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    let [mut year, month, day, hour, minute, second] = parts.as_slice().try_into().ok()?;
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(days * 86400 + hour * 3600 + minute * 60 + second).ok()
}

fn duration(seconds: u64) -> String {
    if seconds >= 60 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// Colour is a property of the output destination, never of collected state.
pub fn use_color(terminal: bool) -> bool {
    terminal && std::env::var_os("NO_COLOR").is_none()
}

// Priority, label, ANSI SGR. Keep completed non-check outcomes visible as before.
fn table_state(t: &TaskView) -> (u8, String, &'static str) {
    if t.stalled {
        (0, format!("⚠ STALLED · {}", t.phase), "1;33")
    } else if t.phase == "interrupted" {
        (1, "✗ interrupted".into(), "31")
    } else if t.phase == "done" {
        match t.outcome.as_deref() {
            Some(outcome @ ("fail" | "failed_to_start")) => {
                (1, format!("✗ done · {outcome}"), "31")
            }
            Some("pass") => {
                let label = if t.ready_to_merge {
                    "ready to merge"
                } else if t.merge_blockers.iter().any(|b| b.contains("review")) {
                    "needs review"
                } else {
                    "merge blocked"
                };
                (2, format!("✓ done · pass · {label}"), "32")
            }
            Some(outcome) => (2, format!("done, {outcome}"), ""),
            None => (2, "done".into(), ""),
        }
    } else if t.phase == "not_started" {
        (4, "not started".into(), "")
    } else {
        (3, format!("● {}", t.phase), "36")
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
}

fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.into()
    } else {
        let mut text: String = text.chars().take(width.saturating_sub(1)).collect();
        text.push('…');
        text
    }
}

fn styled(text: &str, sgr: &str, color: bool) -> String {
    if color && !sgr.is_empty() {
        format!("\x1b[{sgr}m{text}\x1b[0m")
    } else {
        text.into()
    }
}

fn file_summary(files: &[ChangedFile], width: usize) -> String {
    if files.is_empty() {
        return "0".into();
    }
    let mut prefix = files.len().to_string();
    let suffix = if files.len() > 1 {
        format!(", +{} more", files.len() - 1)
    } else {
        String::new()
    };
    let path_width = width.saturating_sub(prefix.len() + 2 + suffix.len());
    let path = one_line(&files[0].path);
    let path_len = path.chars().count();
    // Always show the first path, keeping its identifying tail when space is tight.
    let path = if path_len > path_width {
        let tail: String = path
            .chars()
            .skip(path_len.saturating_sub(path_width.saturating_sub(1)))
            .collect();
        format!("…{tail}")
    } else {
        path
    };
    let mut summary = format!("{prefix}  {path}{suffix}");
    for (index, file) in files.iter().enumerate() {
        let _ = write!(
            prefix,
            "{}{}",
            if index == 0 { "  " } else { ", " },
            one_line(&file.path)
        );
        if prefix.chars().count() > width {
            break;
        }
        let remaining = files.len() - index - 1;
        let candidate = if remaining == 0 {
            prefix.clone()
        } else {
            format!("{prefix}, +{remaining} more")
        };
        // A final short path can fit even when the preceding '+N more' did not.
        if candidate.chars().count() <= width {
            summary = candidate;
        }
    }
    summary
}

pub fn render_rows(tasks: &[TaskView], color: bool) -> String {
    if tasks.is_empty() {
        return "no tasks; start one with: delegate new <id>\n".into();
    }
    let mut tasks: Vec<_> = tasks.iter().collect();
    tasks.sort_by(|a, b| {
        table_state(a)
            .0
            .cmp(&table_state(b).0)
            .then_with(|| a.id.cmp(&b.id))
    });
    let rows: Vec<_> = tasks
        .iter()
        .map(|t| {
            [
                one_line(&t.id),
                table_state(t).1,
                one_line(t.worker.as_deref().unwrap_or("-")),
                duration(t.elapsed_secs),
                duration(t.activity_age_secs),
            ]
        })
        .collect();
    let headers = ["ID", "STATE", "WORKER", "TIME", "QUIET"];
    let mut widths = headers.map(str::len);
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }
    // Two-space indent and separators; reserve room for counts and file names.
    let other_widths = widths[0] + widths[1] + widths[3] + widths[4];
    widths[2] = widths[2].min(100_usize.saturating_sub(12 + other_widths + 20).max(6));
    let files_width = 100_usize.saturating_sub(12 + widths.iter().sum::<usize>());
    let mut text = String::from("  ");
    for (header, width) in headers.iter().zip(widths) {
        let _ = write!(text, "{header:<width$}  ");
    }
    text.push_str("FILES\n");
    for (t, row) in tasks.iter().zip(rows) {
        text.push_str("  ");
        for (index, (value, width)) in row.iter().zip(widths).enumerate() {
            let value = cut(value, width);
            let padded = format!("{value:<width$}");
            text.push_str(&styled(
                &padded,
                if index == 1 { table_state(t).2 } else { "" },
                color,
            ));
            text.push_str("  ");
        }
        let _ = writeln!(text, "{}", file_summary(&t.changed_files, files_width));
        let hint = if t.stalled {
            Some(format!(
                "→ {}",
                t.next
                    .clone()
                    .unwrap_or_else(|| format!("delegate status {}", t.id))
            ))
        } else if table_state(t).0 == 1 {
            t.next.as_ref().map(|next| format!("→ {next}"))
        } else if table_state(t).0 == 3 {
            t.steps.last().map(|step| {
                let step = format!(
                    "└ {}  {}  {}",
                    step["kind"].as_str().unwrap_or(""),
                    step["target"].as_str().unwrap_or(""),
                    step["result"].as_str().unwrap_or("")
                );
                // Include the eight-character indent in the 100-column budget.
                cut(&one_line(&step), 92)
            })
        } else {
            None
        };
        if let Some(hint) = hint {
            let _ = writeln!(text, "        {}", styled(&one_line(&hint), "2", color));
        }
    }
    text
}

pub fn render_detail(t: &TaskView, color: bool) -> String {
    let mut text = render_rows(std::slice::from_ref(t), color);
    let _ = writeln!(
        text,
        "tier: {}\noutcome: {}\nstarted: {}\nlast activity: {}",
        t.tier,
        t.outcome.as_deref().unwrap_or("-"),
        t.started_at.as_deref().unwrap_or("-"),
        t.last_activity_at.as_deref().unwrap_or("-")
    );
    for file in &t.changed_files {
        let _ = writeln!(text, "  {}  +{} -{}", file.path, file.added, file.removed);
    }
    for blocker in &t.merge_blockers {
        let _ = writeln!(text, "merge blocker: {blocker}");
    }
    if let Some(next) = &t.next {
        let _ = writeln!(text, "next: {next}");
    }
    if t.steps_available {
        for step in &t.steps {
            let at = step["at"]
                .as_str()
                .and_then(|at| at.get(11..19))
                .unwrap_or("--:--:--");
            let _ = writeln!(
                text,
                "{at}  {}  {}  {}",
                step["kind"].as_str().unwrap_or(""),
                step["target"]
                    .as_str()
                    .unwrap_or("")
                    .replace(['\n', '\r'], " "),
                step["result"]
                    .as_str()
                    .unwrap_or("")
                    .replace(['\n', '\r'], " ")
            );
        }
    } else {
        let _ = writeln!(text, "step details aren't available for this worker");
    }
    let _ = writeln!(text, "log: {}\n{}", t.log, t.log_tail);
    text
}

pub fn run(ctx: &Ctx, id: Option<&str>, json: bool) -> Result<()> {
    let views = collect(ctx, id, Depth::Full)?;
    let color = use_color(std::io::stdout().is_terminal());
    if json {
        println!("{}", serde_json::json!({"tasks": views}));
    } else if id.is_some() {
        if let Some(view) = views.first() {
            print!("{}", render_detail(view, color));
        }
    } else {
        print!("{}", render_rows(&views, color));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn view(id: &str, phase: &str, outcome: Option<&str>) -> TaskView {
        TaskView {
            id: id.into(),
            title: id.into(),
            goal: String::new(),
            session: None,
            session_name: "by hand".into(),
            phase: phase.into(),
            outcome: outcome.map(str::to_owned),
            worker: Some("pi:test@high".into()),
            tier: "standard".into(),
            pid: None,
            started_at: None,
            last_activity_at: None,
            stalled: false,
            changed_files: vec![],
            steps: vec![],
            steps_available: false,
            ready_to_merge: true,
            merge_blockers: vec![],
            next: Some(format!("delegate run {id}")),
            log: "run.log".into(),
            log_tail: String::new(),
            elapsed_secs: 65,
            activity_age_secs: 425,
        }
    }

    #[test]
    fn running_workers_counts_live_phases_and_uses_the_current_worker() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-running-{}", uuid::Uuid::new_v4()));
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        for (id, phase, worker, stale) in [
            ("t001", "questions", Some("codex:gpt-5"), false),
            ("t002", "implement", Some("pi:hyper/sonnet"), false),
            ("t003", "checks", None, false),
            ("t004", "review", Some("claude:opus"), false),
            ("t005", "implement", Some("codex:gpt-5"), false),
            ("t006", "implement", Some("codex:gpt-5"), true),
            ("t007", "done", Some("codex:gpt-5"), false),
            ("t008", "merge", Some("codex:gpt-5"), false),
            ("t009", "implement", Some("invalid-worker"), false),
            ("t010", "implement", None, false),
        ] {
            let mut task = Task::load(&ctx, id)?;
            fs::create_dir_all(&task.dir)?;
            fs::write(&task.spec, "# Task")?;
            if matches!(id, "t003" | "t004") {
                task.state.rounds.push(crate::task::Round {
                    number: 1,
                    tier: None,
                    worker: Some("codex:gpt-5".into()),
                    status: RoundStatus::Running,
                    research: false,
                    exit: None,
                    quota: false,
                    checks: None,
                });
                task.save()?;
            }
            let mut live = LiveState::current(phase)?;
            live.worker = worker.map(str::to_owned);
            if stale {
                live.boot_id = "other-boot".into();
            }
            atomic_live(&task.dir.join("status.json"), &live)?;
        }
        let running = running_workers(&ctx, "t005");
        let observed: Vec<_> = running
            .iter()
            .map(|(id, w)| (id.as_str(), w.to_string()))
            .collect();
        assert_eq!(
            observed,
            vec![
                ("t001", "codex:gpt-5".into()),
                ("t002", "pi:hyper/sonnet".into()),
                ("t003", "codex:gpt-5".into()),
                ("t004", "claude:opus".into()),
            ]
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn purpose_and_session_are_read_at_every_depth_with_safe_fallbacks() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-purpose-{}", uuid::Uuid::new_v4()));
        // No git repository is needed to read these fields, including the folder name.
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        let mut task = Task::load(&ctx, "t005")?;
        fs::create_dir_all(&task.dir)?;
        fs::write(
            &task.spec,
            "# Task t005: Add divide\n\n## Goal\n<placeholder>\n\n  Divide the bill.  \nSecond line.\n",
        )?;
        task.state.owner = Some(crate::task::Owner {
            session: "session-a".into(),
            dir: "/work/wt/pane-polish".into(),
        });
        task.save()?;
        for depth in [Depth::Phase, Depth::Full] {
            let views = collect(&ctx, Some(&task.id), depth)?;
            let json = serde_json::to_value(&views[0])?;
            assert_eq!(json["title"], "Add divide");
            assert_eq!(json["goal"], "Divide the bill.");
            assert_eq!(json["session"], "session-a");
            assert_eq!(json["session_name"], "pane-polish");
        }
        task.state.owner = None;
        task.save()?;
        fs::write(
            &task.spec,
            commands::SPEC_TEMPLATE.replace("<id>", &task.id),
        )?;
        for depth in [Depth::Phase, Depth::Full] {
            let views = collect(&ctx, Some(&task.id), depth)?;
            let json = serde_json::to_value(&views[0])?;
            assert_eq!(json["title"], "t005");
            assert_eq!(json["goal"], "");
            assert!(json["session"].is_null());
            assert_eq!(json["session_name"], "by hand");
        }
        fs::remove_file(&task.spec)?;
        for depth in [Depth::Phase, Depth::Full] {
            let view = collect_task(&ctx, &task, depth)?;
            assert_eq!(view.title, "t005");
            assert_eq!(view.goal, "");
        }
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn trouble_first_then_finished_running_and_not_started_in_id_order() {
        let mut stalled = view("t009", "implement", None);
        stalled.stalled = true;
        let tasks = [
            view("t001", "not_started", None),
            view("t008", "implement", None),
            view("t007", "done", Some("pass")),
            view("t006", "interrupted", None),
            view("t005", "done", Some("fail")),
            view("t004", "checks", None),
            stalled,
            view("t002", "done", Some("failed_to_start")),
            view("t003", "done", Some("pass")),
        ];
        let text = render_rows(&tasks, false);
        let ids: Vec<_> = text
            .lines()
            .filter_map(|line| {
                line.split_whitespace()
                    .next()
                    .filter(|word| word.starts_with('t'))
            })
            .collect();
        assert_eq!(
            ids,
            [
                "t009", "t002", "t005", "t006", "t003", "t007", "t004", "t008", "t001"
            ]
        );
        assert!(text.contains("⚠ STALLED · implement"));
        assert!(text.contains("✗ done · fail "));
        assert!(text.contains("✗ done · failed_to_start"));
        assert!(text.contains("✗ interrupted"));
        assert!(text.contains("✓ done · pass · ready to merge"));
        assert!(text.contains("● checks"));
        assert!(text.contains("not started"));
    }

    #[test]
    fn unicode_states_and_workers_keep_all_columns_aligned() {
        let mut stalled = view("t001", "implement", None);
        stalled.stalled = true;
        stalled.worker = Some("pi:modèle@high".into());
        let tasks = [
            stalled,
            view("t002", "done", Some("pass")),
            view("t003", "implement", None),
        ];
        let text = render_rows(&tasks, false);
        let header: Vec<_> = text.lines().next().unwrap().chars().collect();
        for column in ["ID", "STATE", "WORKER", "TIME", "QUIET", "FILES"] {
            let label: Vec<_> = column.chars().collect();
            let start = header
                .windows(label.len())
                .position(|part| part == label)
                .unwrap();
            for line in text
                .lines()
                .filter(|line| line.trim_start().starts_with('t'))
            {
                let chars: Vec<_> = line.chars().collect();
                assert_ne!(chars[start], ' ', "{column}: {line}");
                assert_eq!(chars[start - 1], ' ', "{column}: {line}");
            }
        }
    }

    #[test]
    fn long_worker_and_file_lists_fit_one_hundred_characters() {
        let mut task = view("t001", "implement", None);
        task.worker = Some(format!("pi:{}@high", "é".repeat(150)));
        task.changed_files = (0..12)
            .map(|index| ChangedFile {
                path: format!("shop/monthly_report_by_region_{index}.py"),
                added: 0,
                removed: 0,
            })
            .collect();
        let text = render_rows(&[task], false);
        let row = text.lines().nth(1).unwrap();
        assert!(row.chars().count() <= 100, "{row}");
        assert!(row.contains('…'));
        assert!(row.contains("12  …"));
        assert!(row.contains(".py, +11 more"));
        let files = vec![
            ChangedFile {
                path: "a.rs".into(),
                added: 0,
                removed: 0,
            },
            ChangedFile {
                path: "b_long_name.rs".into(),
                added: 0,
                removed: 0,
            },
        ];
        assert_eq!(file_summary(&files, 50), "2  a.rs, b_long_name.rs");
        assert_eq!(file_summary(&files, 16), "2  a.rs, +1 more");
        let short = vec![
            ChangedFile {
                path: "a".into(),
                added: 0,
                removed: 0,
            },
            ChangedFile {
                path: "b".into(),
                added: 0,
                removed: 0,
            },
        ];
        assert_eq!(file_summary(&short, 7), "2  a, b");
        assert_eq!(file_summary(&[], 20), "0");
    }

    #[test]
    fn long_first_path_keeps_its_tail_and_remaining_file_count() {
        let mut task = view("t003", "done", Some("pass"));
        task.changed_files = (1..=6)
            .map(|index| ChangedFile {
                path: format!("shop/reports/monthly_summary_by_region_{index}.py"),
                added: 0,
                removed: 0,
            })
            .collect();
        let text = render_rows(&[task], false);
        let row = text.lines().nth(1).unwrap();
        assert!(row.chars().count() <= 100, "{row}");
        assert!(row.contains("6  …"), "{row}");
        assert!(row.ends_with("_1.py, +5 more"), "{row}");

        let files = vec![ChangedFile {
            path: "shop/régions/été.py".into(),
            added: 0,
            removed: 0,
        }];
        let summary = file_summary(&files, 14);
        assert_eq!(summary, "1  …ons/été.py");
        assert!(summary.chars().count() <= 14);
        assert!(!summary.contains("more"));
    }

    #[test]
    fn long_running_step_fits_one_hundred_characters_including_indent() {
        let mut task = view("t001", "implement", None);
        task.steps = vec![serde_json::json!({
            "kind": "message",
            "target": "shop/été.py\nnext\rline\ttarget",
            "result": "é".repeat(300),
        })];
        let text = render_rows(&[task], false);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        let step = lines[2];
        assert!(step.starts_with("        └ message  shop/été.py next line target  "));
        assert_eq!(step.chars().count(), 100);
        assert!(step.ends_with('…'));
        assert!(!step.contains('\x1b'));
        assert!(!step.contains('\t'));
    }

    #[test]
    fn hints_steps_detail_and_terminal_only_colour() {
        let mut stalled = view("t001", "implement", None);
        stalled.stalled = true;
        stalled.pid = Some(12345);
        stalled.next = Some("delegate status t001  ·  kill 12345".into());
        let failed = view("t002", "done", Some("fail"));
        let mut running = view("t003", "implement", None);
        running.steps = vec![
            serde_json::json!({"kind":"edit\nfile", "target":"shop/\ncode", "result":"ok\rnow"}),
        ];
        let mut review = view("t004", "done", Some("pass"));
        review.ready_to_merge = false;
        review.merge_blockers = vec!["review required".into()];
        let mut blocked = view("t005", "done", Some("pass"));
        blocked.ready_to_merge = false;
        let tasks = [stalled, failed, running, review, blocked];
        let plain = render_rows(&tasks, false);
        assert!(plain.contains("→ delegate status t001  ·  kill 12345"));
        assert!(!plain.contains("delegate discard t001"));
        assert!(plain.contains("→ delegate run t002"));
        assert!(plain.contains("└ edit file  shop/ code  ok now"));
        assert!(plain.contains("✓ done · pass · needs review"));
        assert!(plain.contains("✓ done · pass · merge blocked"));
        assert!(!plain.contains('\x1b'));
        assert!(!render_detail(&tasks[0], false).contains('\x1b'));
        assert!(!use_color(false));
        assert_eq!(use_color(true), std::env::var_os("NO_COLOR").is_none());
        let colored = render_rows(&tasks, true);
        for sgr in ["1;33", "31", "32", "36", "2"] {
            assert!(colored.contains(&format!("\x1b[{sgr}m")));
        }
        let detail = render_detail(&tasks[0], false);
        assert!(detail.starts_with(&render_rows(&tasks[..1], false)));
        assert!(detail.contains("tier: standard\noutcome: -\nstarted: -\nlast activity: -"));
    }

    #[test]
    fn stalled_table_without_next_only_offers_status() {
        let mut task = view("t001", "implement", None);
        task.stalled = true;
        task.next = None;
        let text = render_rows(&[task], false);
        assert!(text.contains("→ delegate status t001\n"));
        assert!(!text.contains("delegate discard"));
    }

    #[test]
    fn log_and_task_metadata_are_not_activity() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-activity-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("tasks/t001/worktree/.task"))?;
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        let t = Task::load(&ctx, "t001")?;
        fs::write(&t.spec, "# Task t001\n")?;
        let threshold: u64 = std::env::var("DELEGATE_STALL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let start = now_secs().saturating_sub(threshold.saturating_add(60));
        let source = File::create(t.wt.join("source"))?;
        source.set_modified(UNIX_EPOCH + Duration::from_secs(start))?;
        fs::write(t.wt.join(".task/REPORT.md"), "fresh report, not activity")?;
        fs::write(t.wt.join(".git"), "fresh git metadata, not activity")?;
        fs::write(t.dir.join("run.log"), "a lot of fresh chatter")?;
        let mut live = LiveState::current("implement")?;
        live.started_at = timestamp(start);
        atomic_live(&t.dir.join("status.json"), &live)?;
        let views = collect(&ctx, None, Depth::Phase)?;
        assert!(views[0].stalled);
        assert_eq!(views[0].pid, Some(live.pid));
        let expected = format!("delegate status t001  ·  kill {}", live.pid);
        assert_eq!(views[0].next.as_deref(), Some(expected.as_str()));
        assert_eq!(serde_json::to_value(&views[0])?["next"], expected.as_str());
        assert!(render_rows(&views, false).contains(&format!("→ {expected}")));
        assert_eq!(
            views[0].last_activity_at.as_deref(),
            Some(live.started_at.as_str())
        );
        source.set_modified(SystemTime::now())?;
        let views = collect(&ctx, None, Depth::Phase)?;
        assert!(!views[0].stalled);
        assert!(views[0].next.is_none());
        source.set_modified(UNIX_EPOCH + Duration::from_secs(start))?;
        live.phase = "checks".into();
        atomic_live(&t.dir.join("status.json"), &live)?;
        let views = collect(&ctx, None, Depth::Phase)?;
        assert!(!views[0].stalled);
        assert!(views[0].next.is_none());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn log_tail_keeps_five_lines_even_when_lines_are_long() -> Result<()> {
        let path = std::env::temp_dir().join(format!("delegate-tail-{}", uuid::Uuid::new_v4()));
        let last = format!("one\ntwo\n{}\nfour\nfive", "x".repeat(20_000));
        fs::write(&path, format!("not included\n{last}\n"))?;
        assert_eq!(tail(&path), last);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn timestamps_roundtrip() {
        for seconds in [0, 1, 951_782_400, 1_793_000_000, 4_102_444_800] {
            assert_eq!(parse_timestamp(&timestamp(seconds)), Some(seconds));
        }
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn reused_pid_and_other_boot_are_not_alive() -> Result<()> {
        let live = LiveState::current("implement")?;
        assert!(live.alive());
        assert!(!is_alive(live.pid, "0", &live.boot_id));
        assert!(!is_alive(live.pid, &live.pid_start, "other boot"));
        Ok(())
    }

    #[test]
    fn zombie_is_not_alive() -> Result<()> {
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(Stdio::null())
            .spawn()?;
        let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
        let mut zombie = false;
        for _ in 0..100 {
            if let Some((state, start)) = process_stat(child.id())
                && state == "Z"
            {
                assert!(!is_alive(child.id(), &start, &boot));
                zombie = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait()?;
        assert!(zombie, "child did not become a zombie");
        Ok(())
    }
}
