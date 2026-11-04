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
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{ProjectRules, ReviewRule, Worker};
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

/// Bounds for the side pane's read-only refresh. The defaults are the t124 values;
/// the next task only changes `steps` (and may turn details off for the plain frame).
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Most event steps kept per task.
    pub steps: usize,
    /// Read changed files and the log tail (the expanded-task fields).
    pub details: bool,
    /// Most directory entries the worktree activity scan may inspect.
    pub max_entries: usize,
    /// Deadline for every git process.
    pub git_timeout: Duration,
    /// Most bytes read from the end of `events.jsonl`.
    pub event_bytes: u64,
    /// Most records considered from that window.
    pub event_records: usize,
    /// Largest single event record accepted.
    pub event_record_bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            steps: 5,
            details: true,
            max_entries: 5_000,
            git_timeout: Duration::from_secs(2),
            event_bytes: 256 * 1024,
            event_records: 200,
            event_record_bytes: 16 * 1024,
        }
    }
}

/// What the bounded event reader omitted, so the UI can say so instead of showing a
/// clean-looking empty activity list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventDrops {
    /// Bytes of `events.jsonl` before the 256 KiB window.
    pub before_window: u64,
    /// Complete records older than the per-refresh record limit.
    pub over_limit: usize,
    /// Records larger than the record-size limit.
    pub oversized: usize,
    /// Records that were not JSON, or had no usable step id.
    pub malformed: usize,
    /// Partial records at either end of the window.
    pub incomplete: usize,
    /// The file could not be opened or read.
    pub unreadable: bool,
}

impl EventDrops {
    pub fn is_empty(&self) -> bool {
        self.before_window == 0
            && self.over_limit == 0
            && self.oversized == 0
            && self.malformed == 0
            && self.incomplete == 0
            && !self.unreadable
    }

    /// A short, sanitized description for a frame or expanded row.
    pub fn summary(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if self.unreadable {
            parts.push("activity unreadable".to_string());
        }
        if self.before_window > 0 {
            parts.push(format!(
                "dropped {} KiB before the read window",
                self.before_window.div_ceil(1024)
            ));
        }
        if self.over_limit > 0 {
            parts.push(format!("dropped {} older records", self.over_limit));
        }
        if self.oversized > 0 {
            parts.push(format!("dropped {} oversized records", self.oversized));
        }
        if self.malformed > 0 {
            parts.push(format!("dropped {} malformed records", self.malformed));
        }
        if self.incomplete > 0 {
            parts.push(format!("dropped {} incomplete records", self.incomplete));
        }
        Some(parts.join("; "))
    }
}

/// One task as the side pane read it. A task that fails to load is isolated instead of
/// turning the whole list into an empty or failed state. The view dominates the enum on
/// purpose: this is the row the side pane consumes, not a hot collection of tiny values.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum TaskRead {
    View {
        view: TaskView,
        /// `None` means a bounded git call failed, so merge readiness is unknown.
        ready: Option<bool>,
        dropped: EventDrops,
        /// The worktree scan hit its entry cap before it could find the latest mtime.
        activity_unknown: bool,
    },
    Unavailable { id: String, reason: String },
}

impl TaskRead {
    pub fn id(&self) -> &str {
        match self {
            Self::View { view, .. } => &view.id,
            Self::Unavailable { id, .. } => id,
        }
    }

    pub fn view(&self) -> Option<&TaskView> {
        match self {
            Self::View { view, .. } => Some(view),
            Self::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Default)]
struct Readiness {
    ready: Option<bool>,
    blockers: Vec<String>,
    next: Option<String>,
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
    pub origin: Option<crate::task::Origin>,
    pub instance: Option<String>,
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

/// Read every task in this checkout with explicit budgets. A missing task store is an
/// error, never an empty list; one unreadable task is isolated as `Unavailable`.
pub fn read_tasks(ctx: &Ctx, budget: Budget) -> Result<Vec<TaskRead>> {
    let ids = list_task_ids(ctx)?;
    Ok(ids
        .into_iter()
        .map(|id| match Task::load(ctx, &id) {
            Ok(task) => read_task(ctx, &task, budget),
            Err(error) => TaskRead::Unavailable {
                id,
                reason: format!("{error:#}"),
            },
        })
        .collect())
}

/// List task folders without the lossy `task_ids` behaviour. A missing, unreadable
/// or unlistable store is an outage, not an empty task list.
fn list_task_ids(ctx: &Ctx) -> Result<Vec<String>> {
    let entries = fs::read_dir(&ctx.dhome).with_context(|| {
        format!(
            "listing task state in {}; restore it or check permissions",
            ctx.dhome.display()
        )
    })?;
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!("listing task state in {}; check permissions", ctx.dhome.display())
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(['_', '.']) {
            continue;
        }
        let file_type = entry.file_type().with_context(|| {
            format!("reading task state entry {}; check permissions", entry.path().display())
        })?;
        if !file_type.is_dir() {
            continue;
        }
        // Keep the existing definition of a task (a SPEC.md), but an unreadable SPEC
        // still names a task we must isolate, not silently omit.
        match fs::metadata(entry.path().join("SPEC.md")) {
            Ok(metadata) if metadata.is_file() => ids.push(name),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => ids.push(name),
        }
    }
    ids.sort();
    Ok(ids)
}

fn read_task(ctx: &Ctx, t: &Task, budget: Budget) -> TaskRead {
    let live = read_live(t).unwrap_or_default();
    let phase = phase(t, live.as_ref());
    let alive = live.as_ref().is_some_and(LiveState::alive);
    let active = live.as_ref().filter(|state| state.phase != "done");
    let round = t.last_round();
    let outcome = if phase == "done" {
        round.map(|round| match round.status {
            RoundStatus::Failed => "failed_to_start",
            RoundStatus::Questions => "questions",
            RoundStatus::Done if round.quota => "quota",
            RoundStatus::Done if round.research => "research",
            RoundStatus::Done => match round.checks.as_ref().map(|checks| checks.outcome) {
                Some(CheckOutcome::Pass) => "pass",
                Some(CheckOutcome::Fail) => "fail",
                Some(CheckOutcome::CheckError) => "check error",
                Some(CheckOutcome::NoCommands) => "no_checks",
                None => "no_changes",
            },
            RoundStatus::Running => unreachable!("running round is interrupted"),
        })
        .map(str::to_string)
    } else {
        None
    };
    let started_at = live.as_ref().map(|state| state.started_at.clone());
    let start = started_at.as_deref().and_then(parse_timestamp).unwrap_or(0);
    let now = now_secs();
    let scan = latest_mtime_bounded(&t.wt, budget.max_entries);
    let mut activity_unknown = scan.truncated;
    let activity = if scan.truncated {
        None
    } else {
        Some(scan.latest.unwrap_or(0).max(start)).filter(|seconds| *seconds != 0)
    };
    let stall_secs = std::env::var("DELEGATE_STALL_SECS")
        .ok()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(300);
    let Readiness {
        ready: readiness_ready,
        blockers: readiness_blockers,
        next: readiness_next,
    } = bounded_readiness(ctx, t, &budget);
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
            (_, Some("pass")) if readiness_ready == Some(true) => {
                Some(format!("delegate merge {}", t.id))
            }
            (_, Some("pass" | "check error")) => readiness_next,
            (_, Some("quota")) => Some(format!(
                "delegate run {} (retry after the quota resets)",
                t.id
            )),
            (_, Some("research" | "no_changes")) => Some(format!("delegate discard {}", t.id)),
            _ => Some(format!("delegate run {}", t.id)),
        }
    };
    let spec_text = fs::read_to_string(&t.spec).unwrap_or_default();
    let mut view = TaskView {
        id: t.id.clone(),
        title: spec::display_title(&spec_text, &t.id),
        goal: spec::goal(&spec_text)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string(),
        session: t.state.owner.as_ref().map(|owner| owner.session.clone()),
        session_name: t.state.owner.as_ref().map_or_else(
            || "by hand".into(),
            |owner| {
                Path::new(&owner.dir)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
        ),
        origin: t.state.origin.clone(),
        instance: t.state.instance.clone(),
        phase,
        outcome,
        worker: active
            .and_then(|state| state.worker.clone())
            .or_else(|| round.and_then(|round| round.worker.clone())),
        tier: active.map_or_else(
            || round.map_or_else(|| "custom".into(), crate::task::Round::tier_label),
            |state| state.tier.clone(),
        ),
        pid: live.as_ref().filter(|state| state.alive()).map(|state| state.pid),
        started_at,
        last_activity_at: activity.map(timestamp),
        stalled: alive
            && live.as_ref().is_some_and(|state| {
                matches!(state.phase.as_str(), "questions" | "implement")
            })
            && activity.is_some_and(|seconds| now.saturating_sub(seconds) > stall_secs),
        changed_files: if budget.details && t.wt.join(".git").exists() {
            changed_files_bounded(ctx, t, &budget).0
        } else {
            vec![]
        },
        steps: vec![],
        steps_available: false,
        ready_to_merge: readiness_ready.unwrap_or(false),
        merge_blockers: readiness_blockers,
        next,
        log: t.dir.join("run.log").display().to_string(),
        log_tail: if budget.details {
            tail(&t.dir.join("run.log"))
        } else {
            String::new()
        },
        elapsed_secs: if start == 0 {
            0
        } else {
            let end = live
                .as_ref()
                .filter(|state| state.phase == "done")
                .and_then(|state| parse_timestamp(&state.heartbeat_at))
                .unwrap_or(now);
            end.saturating_sub(start)
        },
        activity_age_secs: activity.map_or(0, |seconds| now.saturating_sub(seconds)),
    };
    let mut events = crate::harness::events::collect_tail(
        &t.dir.join("events.jsonl"),
        budget.event_bytes,
        budget.event_records,
        budget.event_record_bytes,
    );
    view.steps_available = !events.steps.is_empty();
    let step_activity = events
        .steps
        .iter()
        .filter_map(|step| step["at"].as_str().and_then(parse_timestamp))
        .max();
    if let Some(step_activity) = step_activity
        && activity.is_none_or(|activity| step_activity > activity)
    {
        activity_unknown = false;
        view.last_activity_at = Some(timestamp(step_activity));
        view.activity_age_secs = now.saturating_sub(step_activity);
        view.stalled = view.pid.is_some()
            && matches!(view.phase.as_str(), "questions" | "implement")
            && view.activity_age_secs > stall_secs;
    }
    if events.steps.len() > budget.steps {
        events.steps.drain(..events.steps.len() - budget.steps);
    }
    view.steps = events.steps;
    if let Some(pid) = view.pid {
        view.next = view
            .stalled
            .then(|| format!("delegate status {}  ·  kill {pid}", view.id));
    }
    TaskRead::View {
        view,
        ready: readiness_ready,
        dropped: events.dropped,
        activity_unknown,
    }
}

fn bounded_readiness(ctx: &Ctx, t: &Task, budget: &Budget) -> Readiness {
    let mut readiness = Readiness::default();
    let block = |readiness: &mut Readiness, message: String, command: String| {
        if readiness.blockers.is_empty() {
            readiness.next = Some(command);
        }
        readiness.blockers.push(message);
    };
    if let Err(error) = ensure_idle(t) {
        block(
            &mut readiness,
            format!("{error:#}"),
            format!("delegate status {}", t.id),
        );
        readiness.ready = Some(false);
        return readiness;
    }
    if t.last_round().is_some_and(|round| round.status == RoundStatus::Running) {
        block(
            &mut readiness,
            format!("{}: the last round did not finish", t.id),
            format!("delegate run {}", t.id),
        );
        readiness.ready = Some(false);
        return readiness;
    }
    let Some(implemented) = t.last_done() else {
        block(
            &mut readiness,
            format!("{} has no implemented round", t.id),
            format!("delegate run {}", t.id),
        );
        readiness.ready = Some(false);
        return readiness;
    };
    let base = if let Some(base) = &t.state.base {
        base.clone()
    } else {
        let Ok(base) = bounded_git(
            &ctx.root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            budget.git_timeout,
        ) else {
            readiness.ready = None;
            return readiness;
        };
        base
    };
    let Ok(current) = bounded_git(
        &ctx.root,
        &["rev-parse", "--abbrev-ref", "HEAD"],
        budget.git_timeout,
    ) else {
        readiness.ready = None;
        return readiness;
    };
    if current != base {
        block(
            &mut readiness,
            format!("check out {base} first"),
            format!("git checkout {base}"),
        );
    }
    let Ok(dirty) = bounded_git(
        &ctx.root,
        &["status", "--porcelain", "--untracked-files=no"],
        budget.git_timeout,
    ) else {
        readiness.ready = None;
        return readiness;
    };
    if !dirty.is_empty() {
        block(
            &mut readiness,
            format!("commit or stash changes in {} first", ctx.root.display()),
            "git stash".into(),
        );
    }
    let range = format!("{base}...{}", t.branch);
    let Ok(paths) = bounded_git(
        &ctx.root,
        &["diff", "--name-only", &range],
        budget.git_timeout,
    ) else {
        readiness.ready = None;
        return readiness;
    };
    let paths = paths
        .lines()
        .map(str::to_string)
        .collect::<Vec<String>>();
    if paths.is_empty() {
        block(
            &mut readiness,
            format!("{} has no changes to merge (use: delegate discard {})", t.branch, t.id),
            format!("delegate discard {}", t.id),
        );
    }
    let Ok(head) = bounded_git(
        &ctx.root,
        &["rev-parse", &t.branch],
        budget.git_timeout,
    ) else {
        readiness.ready = None;
        return readiness;
    };
    match &implemented.checks {
        Some(checks) if checks.commit != head => block(
            &mut readiness,
            "acceptance checks haven't run on the final commit (run: delegate check)".into(),
            format!("delegate check {}", t.id),
        ),
        None => block(
            &mut readiness,
            "acceptance checks haven't run (run: delegate check)".into(),
            format!("delegate check {}", t.id),
        ),
        Some(checks) => match checks.outcome {
            CheckOutcome::Pass => {}
            CheckOutcome::NoCommands => block(
                &mut readiness,
                "the spec has no runnable acceptance commands".into(),
                format!("add acceptance commands to {}, then delegate check {}", t.spec.display(), t.id),
            ),
            CheckOutcome::Fail => block(
                &mut readiness,
                "acceptance checks failed".into(),
                format!("delegate run {}", t.id),
            ),
            CheckOutcome::CheckError => block(
                &mut readiness,
                "an acceptance check is broken".into(),
                format!("fix the check, then delegate check {}", t.id),
            ),
        },
    }
    let review = t.state.review.as_ref().filter(|review| review.commit == head);
    match project_rules(&ctx.root) {
        Ok(rules) => match rules.matching(&paths) {
            Ok(matching) => {
                for rule in matching {
                    if let Some(min) = rule.min_tier
                        && implemented.tier.is_none_or(|used| used < min)
                    {
                        block(
                            &mut readiness,
                            format!("rule {} needs tier >= {min}", rule.label()),
                            format!("delegate run {} --tier {min}", t.id),
                        );
                    }
                    if rule.review == Some(ReviewRule::Always) && review.is_none() {
                        block(
                            &mut readiness,
                            format!("rule {} needs a review of the final commit", rule.label()),
                            format!("delegate review {}", t.id),
                        );
                    }
                }
            }
            Err(error) => block(
                &mut readiness,
                format!("cannot evaluate project rules: {error:#}"),
                format!("fix .delegate.toml, then delegate status {}", t.id),
            ),
        },
        Err(error) => block(
            &mut readiness,
            format!("cannot read project rules: {error:#}"),
            format!("fix .delegate.toml, then delegate status {}", t.id),
        ),
    }
    if let Some(review) = review
        && review.high != Some(0)
    {
        block(
            &mut readiness,
            format!(
                "review found {} high-severity issue(s)",
                review.high.map_or_else(|| "unknown".into(), |count| count.to_string())
            ),
            format!("delegate run {}", t.id),
        );
    }
    if fs::read(&t.spec).is_err() {
        block(
            &mut readiness,
            format!("{} cannot be read", t.spec.display()),
            format!("restore {}", t.spec.display()),
        );
    }
    readiness.ready = Some(readiness.blockers.is_empty());
    readiness
}

fn project_rules(root: &Path) -> Result<ProjectRules> {
    let path = root.join(".delegate.toml");
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("parsing {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProjectRules::default()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

#[allow(dead_code)] // The message keeps debugging useful even when readiness only says unknown.
#[derive(Debug)]
enum GitError {
    Timeout,
    Failed(String),
}

fn bounded_git(cwd: &Path, args: &[&str], timeout: Duration) -> std::result::Result<String, GitError> {
    let deadline = Instant::now().checked_add(timeout).unwrap_or_else(Instant::now);
    let mut child = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| GitError::Failed(format!("running git: {error}")))?;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                drop(child.stdout.take());
                drop(child.stderr.take());
                return Err(GitError::Failed(format!("waiting for git: {error}")));
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            // Do not wait for EOF here: a killed shell's descendants may still hold
            // the pipes open. Readiness is already unknown, so no output is needed.
            drop(child.stdout.take());
            drop(child.stderr.take());
            return Err(GitError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| GitError::Failed(format!("reading git output: {error}")))?;
    if !output.status.success() {
        return Err(GitError::Failed(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
}

fn changed_files_bounded(
    ctx: &Ctx,
    t: &Task,
    budget: &Budget,
) -> (Vec<ChangedFile>, bool) {
    let base = if let Some(base) = &t.state.base {
        base.clone()
    } else {
        let Ok(base) = bounded_git(
            &ctx.root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            budget.git_timeout,
        ) else {
            return (Vec::new(), true);
        };
        base
    };
    let Ok(merge_base) = bounded_git(
        &t.wt,
        &["merge-base", &base, "HEAD"],
        budget.git_timeout,
    ) else {
        return (Vec::new(), true);
    };
    let Ok(diff) = bounded_git(
        &t.wt,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--numstat",
            "-z",
            &merge_base,
            "--",
            ".",
            ":(exclude).task/**",
        ],
        budget.git_timeout,
    ) else {
        return (Vec::new(), true);
    };
    let mut files = Vec::new();
    for entry in diff.split('\0').filter(|entry| !entry.is_empty()) {
        let mut fields = entry.splitn(3, '\t');
        let added = fields.next().and_then(|count| count.parse().ok()).unwrap_or(0);
        let removed = fields.next().and_then(|count| count.parse().ok()).unwrap_or(0);
        if let Some(path) = fields.next() {
            files.push(ChangedFile {
                path: path.into(),
                added,
                removed,
            });
        }
    }
    let Ok(untracked) = bounded_git(
        &t.wt,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        budget.git_timeout,
    ) else {
        return (files, true);
    };
    let mut truncated = false;
    for path in untracked
        .split('\0')
        .filter(|path| !path.is_empty() && !path.starts_with(".task/"))
    {
        if files.len() >= budget.max_entries {
            truncated = true;
            break;
        }
        let file = t.wt.join(path);
        let added = match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.is_symlink() => 1,
            Ok(metadata) if metadata.is_file() => {
                if metadata.len() > 5 * 1024 * 1024 {
                    0
                } else {
                    fs::read(&file).map_or(0, |bytes| {
                        if bytes.contains(&0) {
                            0
                        } else {
                            bytes.split_inclusive(|byte| *byte == b'\n').count() as u64
                        }
                    })
                }
            }
            _ => 0,
        };
        files.push(ChangedFile {
            path: path.into(),
            added,
            removed: 0,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    (files, truncated)
}

#[derive(Debug, Default)]
struct Scan {
    latest: Option<u64>,
    truncated: bool,
}

fn latest_mtime_bounded(root: &Path, max_entries: usize) -> Scan {
    let mut latest = None;
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0_usize;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            seen += 1;
            if seen > max_entries {
                return Scan {
                    latest,
                    truncated: true,
                };
            }
            if entry.file_name() == ".git" || entry.file_name() == ".task" {
                continue;
            }
            if let Ok(metadata) = fs::symlink_metadata(entry.path()) {
                if metadata.is_dir() {
                    pending.push(entry.path());
                } else if let Ok(time) = metadata.modified() {
                    let seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                    latest = Some(latest.map_or(seconds, |old| old.max(seconds)));
                }
            }
        }
    }
    Scan {
        latest,
        truncated: false,
    }
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
        origin: t.state.origin.clone(),
        instance: t.state.instance.clone(),
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
            origin: None,
            instance: None,
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
    fn status_json_always_has_origin_and_instance_at_every_depth() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-origin-view-{}", uuid::Uuid::new_v4()));
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        let mut task = Task::load(&ctx, "t001")?;
        fs::create_dir_all(&task.dir)?;
        fs::write(&task.spec, "# Task t001: A task\n")?;
        for depth in [Depth::Phase, Depth::Full] {
            let json = serde_json::to_value(&collect(&ctx, Some("t001"), depth)?[0])?;
            assert!(json["origin"].is_null());
            assert!(json["instance"].is_null());
        }

        task.state.instance = Some("6f0d9c4e-0e7a-4b1a-9a08-2f5e4a2c1b3d".into());
        task.state.origin = Some(crate::task::Origin::Supported {
            source: "herdr:pi".into(),
            agent: "pi".into(),
            kind: crate::task::RefKind::Path,
            value: "/sessions/lead.jsonl".into(),
        });
        task.save()?;
        let json = serde_json::to_value(&collect(&ctx, Some("t001"), Depth::Phase)?[0])?;
        assert_eq!(
            json["origin"],
            serde_json::json!({
                "support": "supported",
                "source": "herdr:pi",
                "agent": "pi",
                "kind": "path",
                "value": "/sessions/lead.jsonl",
            })
        );
        assert_eq!(json["instance"], "6f0d9c4e-0e7a-4b1a-9a08-2f5e4a2c1b3d");
        fs::remove_dir_all(root)?;
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

    fn test_ctx(root: &Path) -> Ctx {
        Ctx {
            root: root.join("repo"),
            dhome: root.join("tasks"),
            gitdir: root.join("repo/.git"),
        }
    }

    fn write_task(ctx: &Ctx, id: &str) -> Result<Task> {
        let task = Task::load(ctx, id)?;
        fs::create_dir_all(&task.dir)?;
        fs::write(&task.spec, format!("# Task {id}: Read me\n"))?;
        Ok(task)
    }

    #[test]
    fn an_unlistable_store_is_an_error_not_an_empty_list() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-unlistable-{}", uuid::Uuid::new_v4()));
        let ctx = test_ctx(&root);
        fs::create_dir_all(&ctx.root)?;
        fs::write(&ctx.dhome, "not a task folder")?;
        assert!(read_tasks(&ctx, Budget::default()).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn one_corrupt_task_is_isolated_and_the_rest_still_read() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-corrupt-{}", uuid::Uuid::new_v4()));
        let ctx = test_ctx(&root);
        fs::create_dir_all(&ctx.dhome)?;
        write_task(&ctx, "t001")?;
        write_task(&ctx, "t002")?;
        fs::write(ctx.dhome.join("t002/STATE.json"), "{not json")?;
        let reads = read_tasks(&ctx, Budget::default())?;
        assert_eq!(reads.len(), 2);
        assert!(matches!(&reads[0], TaskRead::View { view, .. } if view.id == "t001"));
        assert!(matches!(&reads[1], TaskRead::Unavailable { id, .. } if id == "t002"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn a_hundred_thousand_worktree_files_stop_at_the_entry_budget() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-many-files-{}", uuid::Uuid::new_v4()));
        let ctx = test_ctx(&root);
        fs::create_dir_all(&ctx.dhome)?;
        let task = write_task(&ctx, "t001")?;
        fs::create_dir_all(&task.wt)?;
        for index in 0..100_000 {
            File::create(task.wt.join(format!("f{index}")))?;
        }
        let started = Instant::now();
        let reads = read_tasks(
            &ctx,
            Budget {
                details: false,
                ..Budget::default()
            },
        )?;
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
        let TaskRead::View {
            activity_unknown, ..
        } = &reads[0]
        else {
            panic!("the task should still be readable");
        };
        assert!(*activity_unknown);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn done_task(ctx: &Ctx) -> Result<()> {
        let mut task = write_task(ctx, "t001")?;
        task.state.base = Some("main".into());
        task.state.rounds.push(crate::task::Round {
            number: 1,
            tier: Some(crate::config::Tier::Standard),
            worker: Some("pi:test".into()),
            status: RoundStatus::Done,
            research: false,
            exit: Some(0),
            quota: false,
            checks: Some(crate::task::Checks {
                outcome: CheckOutcome::Pass,
                commit: "deadbeef".into(),
            }),
        });
        task.save()?;
        Ok(())
    }

    #[test]
    fn stalled_git_child() {
        let Ok(root) = std::env::var("DELEGATE_TEST_STALL_GIT_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let ctx = test_ctx(&root);
        let started = Instant::now();
        let reads = read_tasks(
            &ctx,
            Budget {
                details: false,
                git_timeout: Duration::from_millis(100),
                ..Budget::default()
            },
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        let TaskRead::View { view, ready, .. } = &reads[0] else {
            panic!("the task should still be readable");
        };
        assert_eq!(view.id, "t001");
        assert_eq!(*ready, None);
    }

    #[test]
    fn stalled_git_on_path_leaves_readiness_unknown() -> Result<()> {
        let root = std::env::temp_dir().join(format!("delegate-stalled-git-{}", uuid::Uuid::new_v4()));
        let ctx = test_ctx(&root);
        fs::create_dir_all(&ctx.dhome)?;
        done_task(&ctx)?;
        let bin = root.join("bin");
        fs::create_dir_all(&bin)?;
        let script = bin.join("git");
        fs::write(&script, "#!/bin/sh\nsleep 30\n")?;
        let mut permissions = fs::metadata(&script)?.permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        fs::set_permissions(&script, permissions)?;
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let path = std::env::join_paths(paths)?;
        let exe = std::env::current_exe()?;
        let status = Command::new(exe)
            .args([
                "--exact",
                "status::tests::stalled_git_child",
                "--nocapture",
            ])
            .env("PATH", path)
            .env("DELEGATE_TEST_STALL_GIT_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        assert!(status.success());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
