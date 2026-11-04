//! The Herdr side-pane reporter: the lead observed in a pane, its tasks, and the
//! sidebar tokens that make those tasks visible beside the conversation.

use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::flag;

use crate::origin::{self, PaneLead};
use crate::status::{self, Budget, TaskRead, TaskView};
use crate::task::{Ctx, Origin};
use crate::watch;

const KEYS: [&str; 5] = ["dlg_t1", "dlg_t2", "dlg_t3", "dlg_t4", "dlg_more"];
const REPORT_SOURCE: &str = "delegate:watch";
const MAX_TOKEN_CHARS: usize = 80;
const DEFAULT_WIDTH: usize = 100;

#[derive(Clone, Debug)]
enum LeadState {
    Known(PaneLead),
    Unavailable,
}

#[derive(Debug)]
struct Refresh {
    lead: LeadState,
    tasks: std::result::Result<Vec<TaskRead>, String>,
    finished: Instant,
}

#[derive(Clone, Debug)]
struct Memory {
    instance: Option<String>,
    origin: Option<Origin>,
}

#[derive(Debug)]
struct GoodTasks {
    tasks: Vec<TaskRead>,
    at: Instant,
}

/// Run `delegate watch --lead-pane`. All reads happen on the reader thread; this loop
/// only reports, draws and handles quit.
pub fn run(ctx: &Ctx, pane: &str, interval_seconds: f64) -> Result<()> {
    let interval = watch::refresh_interval(interval_seconds)?;
    let terminal = io::stdout().is_terminal();
    let interactive = terminal && io::stdin().is_terminal();
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [SIGINT, SIGTERM, SIGHUP] {
        flag::register(signal, Arc::clone(&stop))
            .with_context(|| format!("installing the signal handler for {signal}"))?;
    }
    let terminal_guard = if interactive {
        RawTerminal::enter()
    } else {
        None
    };
    if interactive && terminal_guard.is_some() {
        spawn_key_reader(Arc::clone(&stop));
    }
    let socket = std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from);
    let reader_ctx = Ctx {
        root: ctx.root.clone(),
        dhome: ctx.dhome.clone(),
        gitdir: ctx.gitdir.clone(),
    };
    let receiver = spawn_reader(
        socket.clone(),
        pane.to_string(),
        reader_ctx,
        interval,
    );
    let ttl = interval.saturating_mul(3);
    let mut viewer = Viewer {
        socket,
        pane: pane.to_string(),
        interval,
        ttl,
        ttl_ms: ttl_ms(ttl),
        terminal,
        width: output_width(terminal),
        stop,
        seq: 0,
        memory: HashMap::new(),
        lead: LeadState::Unavailable,
        tasks: None,
        tasks_ok: false,
    };
    let stdout = io::stdout();
    let mut output = stdout.lock();
    viewer.report_loop(&receiver, &mut output)
}

struct Viewer {
    socket: Option<PathBuf>,
    pane: String,
    interval: Duration,
    ttl: Duration,
    ttl_ms: u64,
    terminal: bool,
    width: usize,
    stop: Arc<AtomicBool>,
    seq: u64,
    memory: HashMap<String, Memory>,
    lead: LeadState,
    tasks: Option<GoodTasks>,
    tasks_ok: bool,
}

impl Viewer {
    fn report_loop(&mut self, receiver: &Receiver<Refresh>, output: &mut impl Write) -> Result<()> {
        let mut next_report = Instant::now();
        loop {
            loop {
                match receiver.try_recv() {
                    Ok(refresh) => self.apply(refresh),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.send(&cleared());
                        return Ok(());
                    }
                }
            }
            if self.stop.load(Ordering::SeqCst) {
                self.send(&cleared());
                return Ok(());
            }
            let now = Instant::now();
            if now >= next_report {
                self.send(&self.tokens(now));
                let frame = self.frame(now);
                output
                    .write_all(&frame)
                    .context("writing watch output; check the terminal or output destination")?;
                output.flush()?;
                next_report = next_report
                    .checked_add(self.interval)
                    .unwrap_or_else(|| now + self.interval);
                if next_report <= now {
                    next_report = now + self.interval;
                }
            }
            let wait = next_report
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50));
            match receiver.recv_timeout(wait) {
                Ok(refresh) => self.apply(refresh),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.send(&cleared());
                    return Ok(());
                }
            }
        }
    }

    fn apply(&mut self, refresh: Refresh) {
        self.lead = refresh.lead;
        match refresh.tasks {
            Ok(tasks) => {
                let listed: HashSet<&str> = tasks.iter().map(TaskRead::id).collect();
                self.memory.retain(|id, _| listed.contains(id.as_str()));
                for task in &tasks {
                    if let TaskRead::View { view, .. } = task {
                        remember(&mut self.memory, view);
                    }
                }
                self.tasks = Some(GoodTasks {
                    tasks,
                    at: refresh.finished,
                });
                self.tasks_ok = true;
            }
            Err(_) => self.tasks_ok = false,
        }
    }

    fn send(&mut self, tokens: &[Option<String>; 5]) {
        let Some(socket) = &self.socket else {
            return;
        };
        self.seq = self.seq.saturating_add(1);
        let pairs: Vec<(&str, Option<String>)> = KEYS
            .iter()
            .copied()
            .zip(tokens.iter().cloned())
            .collect();
        let _ = origin::report_metadata(
            socket,
            &self.pane,
            REPORT_SOURCE,
            self.seq,
            self.ttl_ms,
            &pairs,
        );
    }

    fn tokens(&self, now: Instant) -> [Option<String>; 5] {
        let mut out = cleared();
        let LeadState::Known(PaneLead::Supported(Origin::Supported { .. })) = &self.lead else {
            return out;
        };
        let Some(good) = &self.tasks else {
            return out;
        };
        if !self.tasks_ok && now.saturating_duration_since(good.at) > self.ttl {
            return out;
        }
        let mut rows: Vec<(u8, &'static str, String)> = self
            .this_lead()
            .into_iter()
            .map(|task| {
                let state = state_of(task);
                (rank(state), state, task.id().to_string())
            })
            .collect();
        rows.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.2.cmp(&right.2)));
        let count = rows.len();
        for (index, (_, state, id)) in rows.iter().take(4).enumerate() {
            out[index] = Some(cut(
                &format!("{state} · {id}"),
                MAX_TOKEN_CHARS,
            ));
        }
        if count > 4 {
            out[4] = Some(format!("+{} more", count - 4));
        }
        out
    }

    fn frame(&self, now: Instant) -> Vec<u8> {
        let mut frame = String::new();
        push_line(&mut frame, &self.header(), self.width);
        push_line(
            &mut frame,
            &format!("health · {}", self.health(now)),
            self.width,
        );
        let lead_known = matches!(
            &self.lead,
            LeadState::Known(PaneLead::Supported(Origin::Supported { .. }))
        );
        if self.tasks.is_none() {
            push_line(&mut frame, "tasks unavailable; no successful read yet", self.width);
        } else if lead_known {
            let this_lead = self.this_lead();
            if this_lead.is_empty() {
                push_line(&mut frame, "no tasks for this lead", self.width);
            }
            for task in &this_lead {
                push_line(&mut frame, &row(task), self.width);
                self.push_activity(&mut frame, task);
            }
        } else {
            // Without a lead identity, group by the durable origins already stored on
            // the tasks. Never infer one from a pane, focus or checkout name.
            let groups = self.grouped();
            if groups.is_empty() {
                push_line(&mut frame, "no tasks", self.width);
            }
            for (label, tasks) in groups {
                push_line(&mut frame, &label, self.width);
                for task in tasks.iter().take(12) {
                    push_line(&mut frame, &format!("  {}", row(task)), self.width);
                    self.push_activity(&mut frame, task);
                }
                if tasks.len() > 12 {
                    push_line(
                        &mut frame,
                        &format!("  … {} more", tasks.len() - 12),
                        self.width,
                    );
                }
            }
        }
        let (other, codex, unknown, unknown_unavailable) = self.counts();
        let unavailable = if unknown_unavailable == 0 {
            String::new()
        } else {
            format!("   unknown lead · unavailable · {unknown_unavailable}")
        };
        push_line(
            &mut frame,
            &format!(
                "other leads · {other}   Codex (unsupported) · {codex}   unknown lead · {unknown}{unavailable}"
            ),
            self.width,
        );
        let mut bytes = frame.into_bytes();
        if self.terminal {
            let mut redrawn = Vec::with_capacity(bytes.len() + 16);
            redrawn.extend_from_slice(b"\x1b[H\x1b[2J");
            for byte in bytes {
                if byte == b'\n' {
                    redrawn.extend_from_slice(b"\r\n");
                } else {
                    redrawn.push(byte);
                }
            }
            bytes = redrawn;
        }
        bytes
    }

    fn push_activity(&self, frame: &mut String, task: &TaskRead) {
        if let TaskRead::View {
            dropped,
            activity_unknown,
            ..
        } = task
        {
            if let Some(summary) = dropped.summary() {
                push_line(frame, &format!("  activity: {summary}"), self.width);
            }
            if *activity_unknown {
                push_line(
                    frame,
                    "  activity: worktree scan capped; time unknown",
                    self.width,
                );
            }
        }
    }

    fn grouped(&self) -> Vec<(String, Vec<&TaskRead>)> {
        let Some(good) = &self.tasks else {
            return Vec::new();
        };
        let mut groups: std::collections::BTreeMap<String, Vec<&TaskRead>> =
            std::collections::BTreeMap::new();
        for task in &good.tasks {
            let label = match origin_of(task, &self.memory) {
                Some(Origin::Supported { agent, value, .. }) => {
                    format!("lead · {agent} · {}", digest(value))
                }
                Some(Origin::Unsupported { agent, .. }) => format!("{agent} (unsupported)"),
                None => "unknown lead".into(),
            };
            groups.entry(label).or_default().push(task);
        }
        groups
            .into_iter()
            .map(|(label, mut tasks)| {
                tasks.sort_by(|left, right| left.id().cmp(right.id()));
                (label, tasks)
            })
            .collect()
    }

    fn header(&self) -> String {
        match &self.lead {
            LeadState::Known(PaneLead::Supported(Origin::Supported {
                agent, value, ..
            })) => format!("This lead · {agent} · {}", digest(value)),
            LeadState::Known(PaneLead::Unsupported { .. }) => {
                "This lead · Codex: attribution unsupported".into()
            }
            LeadState::Known(PaneLead::NoConversation) => {
                "This lead · no conversation in this pane".into()
            }
            LeadState::Known(
                PaneLead::Supported(Origin::Unsupported { .. }) | PaneLead::Invalid,
            )
            | LeadState::Unavailable => "This lead · lead unavailable".into(),
        }
    }

    fn health(&self, now: Instant) -> String {
        let Some(good) = &self.tasks else {
            return "unavailable".into();
        };
        let age = now.saturating_duration_since(good.at);
        if !self.tasks_ok || age > self.interval {
            format!("stale since {}s", age.as_secs())
        } else {
            format!("live · read {}s ago", age.as_secs())
        }
    }

    fn this_lead(&self) -> Vec<&TaskRead> {
        let LeadState::Known(PaneLead::Supported(lead @ Origin::Supported { .. })) = &self.lead
        else {
            return Vec::new();
        };
        let mut tasks: Vec<&TaskRead> = self.tasks.as_ref().map_or_else(Vec::new, |good| {
            good.tasks
                .iter()
                .filter(|task| origin_of(task, &self.memory) == Some(lead))
                .collect()
        });
        tasks.sort_by(|left, right| {
            rank(state_of(left))
                .cmp(&rank(state_of(right)))
                .then_with(|| left.id().cmp(right.id()))
        });
        tasks
    }

    fn counts(&self) -> (usize, usize, usize, usize) {
        let Some(good) = &self.tasks else {
            return (0, 0, 0, 0);
        };
        let current = match &self.lead {
            LeadState::Known(PaneLead::Supported(origin @ Origin::Supported { .. })) => Some(origin),
            _ => None,
        };
        let mut other = 0;
        let mut codex = 0;
        let mut unknown = 0;
        let mut unknown_unavailable = 0;
        for task in &good.tasks {
            match origin_of(task, &self.memory) {
                Some(Origin::Supported { .. }) => {
                    if origin_of(task, &self.memory) != current {
                        other += 1;
                    }
                }
                Some(Origin::Unsupported { .. }) => codex += 1,
                None => {
                    unknown += 1;
                    if matches!(task, TaskRead::Unavailable { .. }) {
                        unknown_unavailable += 1;
                    }
                }
            }
        }
        (other, codex, unknown, unknown_unavailable)
    }
}

fn cleared() -> [Option<String>; 5] {
    std::array::from_fn(|_| None)
}

fn remember(memory: &mut HashMap<String, Memory>, view: &TaskView) {
    let next = Memory {
        instance: view.instance.clone(),
        origin: view.origin.clone(),
    };
    if memory
        .get(&view.id)
        .is_some_and(|old| old.instance == next.instance)
    {
        return;
    }
    memory.insert(view.id.clone(), next);
}

fn origin_of<'a>(task: &'a TaskRead, memory: &'a HashMap<String, Memory>) -> Option<&'a Origin> {
    match task {
        TaskRead::View { .. } => task.view().and_then(|view| view.origin.as_ref()),
        TaskRead::Unavailable { id, .. } => memory.get(id).and_then(|memory| memory.origin.as_ref()),
    }
}

fn state_of(task: &TaskRead) -> &'static str {
    let TaskRead::View { view, ready, .. } = task else {
        return "unavailable";
    };
    if view.stalled {
        return "stalled";
    }
    match view.phase.as_str() {
        "interrupted" => "interrupted",
        "done" => match view.outcome.as_deref() {
            Some("fail" | "failed_to_start") => "failed",
            Some("pass") if *ready == Some(true) => "ready",
            Some("pass" | "check error" | "no_checks") => "blocked",
            _ => "done",
        },
        "not_started" => "waiting",
        "checks" => "checking",
        _ => "running",
    }
}

fn rank(state: &str) -> u8 {
    match state {
        "failed" | "blocked" | "interrupted" | "unavailable" => 0,
        "stalled" => 1,
        "ready" => 2,
        "running" | "checking" => 3,
        _ => 4,
    }
}

fn row(task: &TaskRead) -> String {
    match task {
        TaskRead::Unavailable { id, reason } => {
            format!("{id}  unavailable · {reason}")
        }
        TaskRead::View { view, ready, .. } => {
            let status = view.outcome.as_ref().map_or_else(
                || view.phase.clone(),
                |outcome| format!("{} · {outcome}", view.phase),
            );
            let stalled = if view.stalled { "  stalled" } else { "" };
            let readiness = if ready.is_none() {
                "  readiness unknown"
            } else {
                ""
            };
            format!("{}  {}  {status}{stalled}{readiness}", view.id, view.title)
        }
    }
}

fn push_line(frame: &mut String, text: &str, width: usize) {
    frame.push_str(&fit(text, width));
    frame.push('\n');
}

fn fit(text: &str, width: usize) -> String {
    let text = sanitize(text);
    let width = width.max(1);
    if text.chars().count() <= width {
        text
    } else {
        let mut truncated: String = text.chars().take(width - 1).collect();
        truncated.push('…');
        truncated
    }
}

fn cut(text: &str, width: usize) -> String {
    fit(text, width)
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if origin::forbidden_display_char(character) {
                '\u{FFFD}'
            } else {
                character
            }
        })
        .collect()
}

fn digest(value: &str) -> String {
    crate::sha256::hex(value.as_bytes()).chars().take(6).collect()
}

fn ttl_ms(ttl: Duration) -> u64 {
    u64::try_from(ttl.as_millis())
        .unwrap_or(86_400_000)
        .clamp(1, 86_400_000)
}

fn spawn_reader(
    socket: Option<PathBuf>,
    pane: String,
    ctx: Ctx,
    interval: Duration,
) -> Receiver<Refresh> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        loop {
            let lead = socket.as_deref().map_or(LeadState::Unavailable, |socket| {
                match origin::pane_lead(socket, &pane) {
                    Ok(lead) => LeadState::Known(lead),
                    Err(_) => LeadState::Unavailable,
                }
            });
            let tasks = status::read_tasks(
                &ctx,
                Budget {
                    steps: 5,
                    details: false,
                    ..Budget::default()
                },
            )
            .map_err(|error| format!("{error:#}"));
            let finished = Instant::now();
            if sender
                .send(Refresh {
                    lead,
                    tasks,
                    finished,
                })
                .is_err()
            {
                return;
            }
            thread::sleep(interval);
        }
    });
    receiver
}

fn is_quit_key(byte: u8) -> bool {
    byte == b'q' || byte == 0x03
}

fn spawn_key_reader(stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut byte = [0_u8; 1];
        loop {
            match stdin.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) if is_quit_key(byte[0]) => {
                    stop.store(true, Ordering::SeqCst);
                    return;
                }
                Ok(_) => {}
            }
        }
    });
}

struct RawTerminal {
    saved: String,
}

impl RawTerminal {
    fn enter() -> Option<Self> {
        let saved = stty(&["-g"]).ok()?.trim().to_string();
        if saved.is_empty() {
            return None;
        }
        if stty(&["raw", "-echo"]).is_err() {
            let _ = stty(&[saved.as_str()]);
            return None;
        }
        Some(Self { saved })
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        let _ = stty(&[self.saved.as_str()]);
    }
}

fn stty(args: &[&str]) -> Result<String> {
    let output = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .context("running stty; check that a terminal is attached")?;
    if !output.status.success() {
        bail!(
            "stty {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn output_width(terminal: bool) -> usize {
    if !terminal {
        return DEFAULT_WIDTH;
    }
    stty(&["size"])
        .ok()
        .and_then(|size| {
            size.split_whitespace()
                .nth(1)
                .and_then(|columns| columns.parse::<usize>().ok())
        })
        .unwrap_or(DEFAULT_WIDTH)
        .max(20)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;

    use serde_json::{Value, json};

    use crate::status::{ChangedFile, EventDrops};
    use crate::task::RefKind;

    struct Fixture {
        dir: PathBuf,
        path: PathBuf,
        identity: Arc<Mutex<Option<Value>>>,
        requests: Arc<Mutex<Vec<Value>>>,
        stop: Arc<AtomicBool>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture() -> Fixture {
        let dir = std::env::temp_dir().join(format!("delegate-herdr-view-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let identity = Arc::new(Mutex::new(None));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let server_identity = Arc::clone(&identity);
        let server_requests = Arc::clone(&requests);
        let server_stop = Arc::clone(&stop);
        thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut connection, _)) => {
                        connection.set_nonblocking(false).unwrap();
                        let mut line = String::new();
                        {
                            let mut reader = BufReader::new(&connection);
                            if reader.read_line(&mut line).is_err() {
                                continue;
                            }
                        }
                        let Ok(request) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        server_requests.lock().unwrap().push(request.clone());
                        let id = request["id"].clone();
                        let method = request["method"].as_str().unwrap_or_default();
                        let response = if method == "pane.get" {
                            let pane = request["params"]["pane_id"].clone();
                            let mut info = json!({"pane_id": pane});
                            if let Some(identity) = server_identity.lock().unwrap().clone() {
                                info["agent_session"] = identity;
                            }
                            json!({"id": id, "result": {"type": "pane_info", "pane": info}})
                        } else {
                            json!({"id": id, "result": {"type": "ok"}})
                        };
                        let mut response = response.to_string();
                        response.push('\n');
                        let _ = connection.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Fixture {
            dir,
            path,
            identity,
            requests,
            stop,
        }
    }

    fn supported(agent: &str, value: &str) -> Origin {
        Origin::Supported {
            source: format!("herdr:{agent}"),
            agent: agent.into(),
            kind: RefKind::Id,
            value: value.into(),
        }
    }

    fn identity(agent: &str, value: &str) -> Value {
        json!({
            "source": format!("herdr:{agent}"),
            "agent": agent,
            "kind": "id",
            "value": value,
        })
    }

    fn view(id: &str, phase: &str, outcome: Option<&str>, ready: bool) -> TaskRead {
        TaskRead::View {
            view: TaskView {
                id: id.into(),
                title: format!("Title of {id}"),
                goal: String::new(),
                session: None,
                session_name: "by hand".into(),
                origin: Some(supported("pi", "/sessions/lead-a.jsonl")),
                instance: Some(format!("{id}-instance")),
                phase: phase.into(),
                outcome: outcome.map(str::to_owned),
                worker: Some("pi:test".into()),
                tier: "standard".into(),
                pid: None,
                started_at: None,
                last_activity_at: None,
                stalled: false,
                changed_files: vec![ChangedFile {
                    path: "shop/pricing.py".into(),
                    added: 1,
                    removed: 0,
                }],
                steps: vec![],
                steps_available: false,
                ready_to_merge: ready,
                merge_blockers: vec![],
                next: None,
                log: "run.log".into(),
                log_tail: String::new(),
                elapsed_secs: 1,
                activity_age_secs: 0,
            },
            ready: Some(ready),
            dropped: EventDrops::default(),
            activity_unknown: false,
        }
    }

    fn with_origin(task: TaskRead, origin: Origin) -> TaskRead {
        match task {
            TaskRead::View {
                mut view,
                ready,
                dropped,
                activity_unknown,
            } => {
                view.origin = Some(origin);
                TaskRead::View {
                    view,
                    ready,
                    dropped,
                    activity_unknown,
                }
            }
            TaskRead::Unavailable { id, reason } => TaskRead::Unavailable { id, reason },
        }
    }

    fn viewer(
        socket: Option<PathBuf>,
        lead: LeadState,
        tasks: Option<GoodTasks>,
        tasks_ok: bool,
        ttl: Duration,
    ) -> Viewer {
        Viewer {
            socket,
            pane: "w1:p1".into(),
            interval: Duration::from_secs(2),
            ttl,
            ttl_ms: ttl_ms(ttl),
            terminal: false,
            width: 100,
            stop: Arc::new(AtomicBool::new(false)),
            seq: 0,
            memory: HashMap::new(),
            lead,
            tasks,
            tasks_ok,
        }
    }

    #[test]
    fn tokens_follow_attention_order_use_state_first_and_mark_more() {
        let now = Instant::now();
        let lead = supported("pi", "/sessions/lead-a.jsonl");
        let mut tasks = vec![
            view("t1", "implement", None, false),
            view("t2", "done", Some("pass"), true),
            view("t3", "done", Some("fail"), false),
            view("t4", "implement", None, false),
            view("t5", "done", Some("pass"), false),
            view("t6", "not_started", None, false),
        ];
        for task in &mut tasks {
            if task.id() == "t4"
                && let TaskRead::View { view, .. } = task
            {
                view.stalled = true;
            }
        }
        let mut viewer = viewer(
            None,
            LeadState::Known(PaneLead::Supported(lead)),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        let tokens = viewer.tokens(now);
        assert_eq!(tokens[0].as_deref(), Some("failed · t3"));
        assert_eq!(tokens[1].as_deref(), Some("blocked · t5"));
        assert_eq!(tokens[2].as_deref(), Some("stalled · t4"));
        assert_eq!(tokens[3].as_deref(), Some("ready · t2"));
        assert_eq!(tokens[4].as_deref(), Some("+2 more"));

        let unknown = viewer.tokens(now);
        assert_eq!(unknown, tokens);
        viewer.lead = LeadState::Known(PaneLead::NoConversation);
        assert_eq!(viewer.tokens(now), cleared());
        viewer.lead = LeadState::Known(PaneLead::Unsupported {
            source: "herdr:codex".into(),
            agent: "codex".into(),
        });
        assert_eq!(viewer.tokens(now), cleared());
    }

    #[test]
    fn an_unreadable_task_is_attributed_from_memory_and_never_from_a_guess() {
        let now = Instant::now();
        let lead = supported("pi", "/sessions/lead-a.jsonl");
        let other = supported("claude", "session-b");
        let tasks = vec![
            view("t1", "implement", None, false),
            TaskRead::Unavailable {
                id: "t2".into(),
                reason: "bad STATE.json".into(),
            },
            with_origin(view("t3", "implement", None, false), other),
            TaskRead::Unavailable {
                id: "t4".into(),
                reason: "bad STATE.json".into(),
            },
        ];
        let mut viewer = viewer(
            None,
            LeadState::Known(PaneLead::Supported(lead)),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        viewer.memory.insert(
            "t2".into(),
            Memory {
                instance: Some("t2-instance".into()),
                origin: Some(supported("pi", "/sessions/lead-a.jsonl")),
            },
        );
        let tokens = viewer.tokens(now);
        assert_eq!(
            tokens[0].as_deref(),
            Some("unavailable · t2"),
            "remembered task ranks with failed/blocked"
        );
        assert_eq!(tokens[1].as_deref(), Some("running · t1"));
        assert_eq!(tokens[2], None);
        let (other_count, codex, unknown, unknown_unavailable) = viewer.counts();
        assert_eq!((other_count, codex, unknown, unknown_unavailable), (1, 0, 1, 1));
    }

    #[test]
    fn memory_is_learned_replaced_and_dropped_by_the_live_reader() {
        let now = Instant::now();
        let lead_a = supported("pi", "/sessions/lead-a.jsonl");
        let lead_b = supported("pi", "/sessions/lead-b.jsonl");
        let lead = LeadState::Known(PaneLead::Supported(lead_a.clone()));
        let mut viewer = viewer(None, lead.clone(), None, false, Duration::from_secs(6));
        let mut task = view("t1", "implement", None, false);
        if let TaskRead::View { view, .. } = &mut task {
            view.origin = Some(lead_a.clone());
            view.instance = Some("instance-a".into());
        }
        viewer.apply(Refresh {
            lead: lead.clone(),
            tasks: Ok(vec![task]),
            finished: now,
        });
        assert_eq!(viewer.tokens(now)[0].as_deref(), Some("running · t1"));

        viewer.apply(Refresh {
            lead: lead.clone(),
            tasks: Ok(vec![TaskRead::Unavailable {
                id: "t1".into(),
                reason: "bad STATE.json".into(),
            }]),
            finished: now,
        });
        assert_eq!(viewer.tokens(now)[0].as_deref(), Some("unavailable · t1"));

        let mut reused = view("t1", "implement", None, false);
        if let TaskRead::View { view, .. } = &mut reused {
            view.origin = Some(lead_b.clone());
            view.instance = Some("instance-b".into());
        }
        viewer.apply(Refresh {
            lead: lead.clone(),
            tasks: Ok(vec![reused]),
            finished: now,
        });
        assert_eq!(
            viewer.tokens(now),
            cleared(),
            "a reused id with a new instance is a different task"
        );
        assert_eq!(viewer.counts().0, 1);

        viewer.apply(Refresh {
            lead,
            tasks: Ok(Vec::new()),
            finished: now,
        });
        assert!(viewer.memory.is_empty(), "memory is dropped with the listing");
    }

    #[test]
    fn a_lead_change_or_an_old_read_clears_the_tokens() {
        let now = Instant::now();
        let lead_a = supported("pi", "/sessions/lead-a.jsonl");
        let lead_b = supported("pi", "/sessions/lead-b.jsonl");
        let tasks = vec![view("t1", "implement", None, false)];
        let mut viewer = viewer(
            None,
            LeadState::Known(PaneLead::Supported(lead_b)),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        assert_eq!(viewer.tokens(now), cleared(), "another lead's rows leaked");
        viewer.lead = LeadState::Known(PaneLead::Supported(lead_a));
        assert_eq!(viewer.tokens(now)[0].as_deref(), Some("running · t1"));
        viewer.tasks_ok = false;
        assert_eq!(
            viewer.tokens(now)[0].as_deref(),
            Some("running · t1"),
            "a short read failure repeats the last good rows"
        );
        viewer.tasks = Some(GoodTasks {
            tasks: vec![view("t1", "implement", None, false)],
            at: now.checked_sub(Duration::from_secs(7)).unwrap(),
        });
        assert_eq!(
            viewer.tokens(now),
            cleared(),
            "an outage longer than the TTL clears the rows"
        );
    }

    #[test]
    fn empty_live_and_stale_reads_are_different() {
        let now = Instant::now();
        let lead = LeadState::Known(PaneLead::Supported(supported("pi", "/a.jsonl")));
        let mut viewer = viewer(
            None,
            lead.clone(),
            Some(GoodTasks {
                tasks: Vec::new(),
                at: now,
            }),
            true,
            Duration::from_secs(6),
        );
        assert_eq!(viewer.tokens(now), cleared());
        assert!(viewer.health(now).starts_with("live"));
        viewer.tasks = None;
        assert_eq!(viewer.tokens(now), cleared());
        assert_eq!(viewer.health(now), "unavailable");
        viewer.tasks = Some(GoodTasks {
            tasks: vec![view("t1", "implement", None, false)],
            at: now.checked_sub(Duration::from_secs(7)).unwrap(),
        });
        viewer.tasks_ok = false;
        assert_eq!(viewer.health(now), "stale since 7s");
    }

    #[test]
    fn reports_carry_source_seq_ttl_all_keys_and_no_titles() {
        let fixture = fixture();
        let now = Instant::now();
        let lead = supported("pi", "/sessions/lead-a.jsonl");
        let tasks = vec![
            view("t1", "implement", None, false),
            TaskRead::Unavailable {
                id: "t2".into(),
                reason: "bad STATE.json".into(),
            },
        ];
        let mut viewer = viewer(
            Some(fixture.path.clone()),
            LeadState::Known(PaneLead::Supported(lead.clone())),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_millis(1500),
        );
        viewer.memory.insert(
            "t2".into(),
            Memory {
                instance: None,
                origin: Some(lead),
            },
        );
        viewer.send(&viewer.tokens(now));
        viewer.send(&cleared());
        let requests = fixture.requests.lock().unwrap();
        let reports: Vec<&Value> = requests
            .iter()
            .filter(|request| request["method"] == "pane.report_metadata")
            .collect();
        assert_eq!(reports.len(), 2);
        for (index, report) in reports.iter().enumerate() {
            let params = &report["params"];
            assert_eq!(params["source"], "delegate:watch");
            assert_eq!(params["ttl_ms"], 1500);
            assert_eq!(params["seq"], i64::try_from(index + 1).unwrap());
            assert_eq!(
                params["tokens"].as_object().unwrap().len(),
                5,
                "all five keys are set every cycle"
            );
        }
        assert_eq!(reports[0]["params"]["tokens"]["dlg_t1"], "unavailable · t2");
        assert_eq!(reports[0]["params"]["tokens"]["dlg_t2"], "running · t1");
        assert!(reports[0]["params"]["tokens"]["dlg_t3"].is_null());
        assert!(reports[0]["params"]["tokens"]["dlg_more"].is_null());
        for value in reports[1]["params"]["tokens"].as_object().unwrap().values() {
            assert!(value.is_null(), "the final report must clear every key");
        }
    }

    #[test]
    fn every_exit_trigger_shares_one_flag_and_has_a_final_clear() {
        assert!(is_quit_key(b'q'));
        assert!(is_quit_key(0x03));
        assert!(!is_quit_key(b'Q'));
        assert!(!is_quit_key(b'x'));

        let fixture = fixture();
        let (sender, receiver) = mpsc::channel();
        let mut viewer = viewer(
            Some(fixture.path.clone()),
            LeadState::Unavailable,
            None,
            false,
            Duration::from_secs(6),
        );
        viewer.stop.store(true, Ordering::SeqCst);
        let mut output = Vec::new();
        viewer.report_loop(&receiver, &mut output).unwrap();
        drop(sender);
        let requests = fixture.requests.lock().unwrap();
        let reports: Vec<&Value> = requests
            .iter()
            .filter(|request| request["method"] == "pane.report_metadata")
            .collect();
        assert_eq!(reports.len(), 1);
        for value in reports[0]["params"]["tokens"].as_object().unwrap().values() {
            assert!(value.is_null());
        }
    }

    #[test]
    fn quit_is_seen_within_two_hundred_ms_even_with_a_stuck_reader() {
        let (_sender, receiver) = mpsc::channel::<Refresh>();
        let mut viewer = viewer(
            None,
            LeadState::Unavailable,
            None,
            false,
            Duration::from_secs(6),
        );
        let stop = Arc::clone(&viewer.stop);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            stop.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let mut output = Vec::new();
        viewer.report_loop(&receiver, &mut output).unwrap();
        assert!(started.elapsed() < Duration::from_millis(200), "{:?}", started.elapsed());
    }

    #[test]
    fn task_text_is_sanitized_and_truncated_before_it_reaches_a_token() {
        let now = Instant::now();
        let lead = supported("pi", "/sessions/lead-a.jsonl");
        let mut task = view(
            "t1\u{1b}\u{202e}",
            "done",
            Some("fail"),
            false,
        );
        if let TaskRead::View { view, .. } = &mut task {
            view.title = "failed \u{202e}exploit\u{2028}title".into();
        }
        let viewer = viewer(
            None,
            LeadState::Known(PaneLead::Supported(lead)),
            Some(GoodTasks {
                tasks: vec![task],
                at: now,
            }),
            true,
            Duration::from_secs(6),
        );
        let tokens = viewer.tokens(now);
        let value = tokens[0].as_deref().unwrap();
        assert!(value.starts_with("failed · t1"));
        assert!(!value.contains("exploit"));
        assert!(!value.contains('\u{202e}'));
        assert!(value.chars().count() <= MAX_TOKEN_CHARS);
        assert_eq!(sanitize("a\u{1b}b\u{202e}c\u{2028}d"), "a\u{FFFD}b\u{FFFD}c\u{FFFD}d");
        assert_eq!(cut(&"x".repeat(100), 10).chars().count(), 10);
        assert!(cut(&"x".repeat(100), 10).ends_with('…'));
    }

    fn signal_test_ctx(root: &std::path::Path) -> Ctx {
        Ctx {
            root: root.join("repo"),
            dhome: root.join("tasks"),
            gitdir: root.join("repo/.git"),
        }
    }

    fn signal_task(ctx: &Ctx, origin: &Origin) {
        let mut task = crate::task::Task::load(ctx, "t301").unwrap();
        std::fs::create_dir_all(&task.dir).unwrap();
        std::fs::write(&task.spec, "# Task t301: Waiting\n").unwrap();
        task.state.origin = Some(origin.clone());
        task.state.instance = Some("t301-instance".into());
        task.save().unwrap();
    }

    #[test]
    fn signal_child() {
        let Ok(signal) = std::env::var("DELEGATE_TEST_SIGNAL") else {
            return;
        };
        let root = PathBuf::from(
            std::env::var("DELEGATE_TEST_ROOT").expect("test root is set by the parent"),
        );
        let ctx = signal_test_ctx(&root);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(700));
            let _ = Command::new("kill")
                .arg(format!("-{signal}"))
                .arg(std::process::id().to_string())
                .status();
        });
        run(&ctx, "w1:p1", 0.05).unwrap();
    }

    #[test]
    fn every_signal_exit_path_sends_a_final_clear() {
        let lead = supported("pi", "/sessions/lead-a.jsonl");
        for signal in ["INT", "TERM", "HUP"] {
            let root = std::env::temp_dir().join(format!(
                "delegate-signal-{signal}-{}",
                uuid::Uuid::new_v4()
            ));
            let ctx = signal_test_ctx(&root);
            std::fs::create_dir_all(&ctx.dhome).unwrap();
            signal_task(&ctx, &lead);
            let fixture = fixture();
            *fixture.identity.lock().unwrap() = Some(identity("pi", "/sessions/lead-a.jsonl"));
            let exe = std::env::current_exe().unwrap();
            let status = Command::new("timeout")
                .arg("10")
                .arg(exe)
                .args(["--exact", "herdr_view::tests::signal_child"])
                .env("DELEGATE_TEST_SIGNAL", signal)
                .env("DELEGATE_TEST_ROOT", &root)
                .env("HERDR_SOCKET_PATH", &fixture.path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "{signal} child failed");
            let requests = fixture.requests.lock().unwrap();
            let reports: Vec<&Value> = requests
                .iter()
                .filter(|request| request["method"] == "pane.report_metadata")
                .collect();
            assert!(!reports.is_empty(), "{signal}: no reports");
            assert!(
                reports
                    .iter()
                    .any(|report| report["params"]["tokens"]["dlg_t1"].as_str().is_some()),
                "{signal}: the pane never showed a row before the signal"
            );
            for value in reports
                .last()
                .unwrap()["params"]["tokens"]
                .as_object()
                .unwrap()
                .values()
            {
                assert!(value.is_null(), "{signal}: the last report did not clear");
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn pane_identity_is_read_once_per_refresh_and_follows_the_conversation() {
        let fixture = fixture();
        *fixture.identity.lock().unwrap() = Some(identity("pi", "/sessions/a.jsonl"));
        assert_eq!(
            origin::pane_lead(&fixture.path, "w1:p1").unwrap(),
            PaneLead::Supported(supported("pi", "/sessions/a.jsonl"))
        );
        *fixture.identity.lock().unwrap() = None;
        assert_eq!(
            origin::pane_lead(&fixture.path, "w1:p1").unwrap(),
            PaneLead::NoConversation
        );
        *fixture.identity.lock().unwrap() = Some(identity("codex", "thread"));
        assert_eq!(
            origin::pane_lead(&fixture.path, "w1:p1").unwrap(),
            PaneLead::Unsupported {
                source: "herdr:codex".into(),
                agent: "codex".into(),
            }
        );
        *fixture.identity.lock().unwrap() =
            Some(json!({"source": "herdr:pi", "agent": "pi", "kind": "id", "value": "a\u{202e}"}));
        assert_eq!(
            origin::pane_lead(&fixture.path, "w1:p1").unwrap(),
            PaneLead::Invalid
        );
        *fixture.identity.lock().unwrap() = Some(identity("claude", "session-b"));
        assert_eq!(
            origin::pane_lead(&fixture.path, "w1:p1").unwrap(),
            PaneLead::Supported(supported("claude", "session-b"))
        );
    }
}
