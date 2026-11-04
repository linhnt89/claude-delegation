//! The Herdr side-pane reporter: the lead observed in a pane, its tasks, and the
//! sidebar tokens that make those tasks visible beside the conversation.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
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
const DETAIL_STEPS: usize = 50;
const BESIDE_COMMAND: &str = "exec \"$DELEGATE_BIN\" watch --lead-pane \"$DELEGATE_LEAD_PANE\"";
const DEFAULT_AGENT_ROWS: &str = "[\"state_icon\", \"machine\", \"workspace\", \"tab\"],\n    [\"agent\"],";

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectionKey {
    id: String,
    instance: Option<String>,
}

impl SelectionKey {
    fn of(task: &TaskRead, memory: &HashMap<String, Memory>) -> Self {
        match task {
            TaskRead::View { view, .. } => Self {
                id: view.id.clone(),
                instance: view.instance.clone(),
            },
            TaskRead::Unavailable { id, .. } => Self {
                id: id.clone(),
                instance: memory.get(id).and_then(|memory| memory.instance.clone()),
            },
        }
    }

    fn of_view(view: &TaskView) -> Self {
        Self {
            id: view.id.clone(),
            instance: view.instance.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    Task(SelectionKey),
    Group(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    Enter,
    Tab,
    Quit,
}

#[derive(Debug, Default)]
struct KeyParser {
    escape: u8,
}

impl KeyParser {
    fn push(&mut self, byte: u8) -> Option<Key> {
        match (self.escape, byte) {
            (0, 0x1b) => {
                self.escape = 1;
                None
            }
            (1, b'[') => {
                self.escape = 2;
                None
            }
            (2, b'A') => {
                self.escape = 0;
                Some(Key::Up)
            }
            (2, b'B') => {
                self.escape = 0;
                Some(Key::Down)
            }
            (1 | 2, _) => {
                self.escape = 0;
                None
            }
            (_, b'\r' | b'\n') => Some(Key::Enter),
            (_, b'\t') => Some(Key::Tab),
            (_, b'q' | 0x03) => Some(Key::Quit),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum LeadState {
    Known(PaneLead),
    Unavailable,
}

#[derive(Debug)]
struct DetailRead {
    key: SelectionKey,
    tasks: std::result::Result<Vec<TaskRead>, String>,
}

#[derive(Debug)]
struct Refresh {
    lead: LeadState,
    tasks: std::result::Result<Vec<TaskRead>, String>,
    detail: Option<DetailRead>,
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
    let keys = Arc::new(Mutex::new(Vec::new()));
    if interactive && terminal_guard.is_some() {
        spawn_key_reader(Arc::clone(&stop), Arc::clone(&keys));
    }
    let socket = std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from);
    let reader_ctx = Ctx {
        root: ctx.root.clone(),
        dhome: ctx.dhome.clone(),
        gitdir: ctx.gitdir.clone(),
    };
    let (receiver, detail_tx) = spawn_reader(socket.clone(), pane.to_string(), reader_ctx, interval);
    let ttl = interval.saturating_mul(3);
    let mut viewer = Viewer {
        socket,
        pane: pane.to_string(),
        interval,
        ttl,
        ttl_ms: ttl_ms(ttl),
        terminal,
        interactive,
        width: output_width(terminal),
        stop,
        keys,
        seq: 0,
        memory: HashMap::new(),
        lead: LeadState::Unavailable,
        tasks: None,
        tasks_ok: false,
        selected: None,
        expanded: None,
        vanished: None,
        detail: None,
        detail_requested: None,
        detail_tx: Some(detail_tx),
    };
    let stdout = io::stdout();
    let mut output = stdout.lock();
    viewer.report_loop(&receiver, &mut output)
}

/// A lead pane that `--beside` may split, and the socket to ask.
#[derive(Debug)]
struct BesideTarget {
    socket: PathBuf,
    pane: String,
}

fn beside_target(env: &origin::Env) -> Result<BesideTarget> {
    if env.sandboxed {
        bail!(
            "run `delegate watch --beside` from a Herdr lead pane, not inside a worker sandbox"
        );
    }
    let Some(socket) = env.socket.as_deref() else {
        bail!(
            "run `delegate watch --beside` from a Herdr lead pane (HERDR_PANE_ID and HERDR_SOCKET_PATH must be set)"
        );
    };
    let Some(pane) = env.pane_id.as_deref().and_then(OsStr::to_str) else {
        bail!(
            "run `delegate watch --beside` from a Herdr lead pane (HERDR_PANE_ID and HERDR_SOCKET_PATH must be set)"
        );
    };
    if pane.is_empty() || pane.chars().any(origin::forbidden_display_char) {
        bail!("run `delegate watch --beside` from a Herdr lead pane with a valid pane id");
    }
    Ok(BesideTarget {
        socket: socket.to_path_buf(),
        pane: pane.to_string(),
    })
}

/// Refuse early when `--beside` was not launched from a Herdr lead pane.
pub(crate) fn check_beside_environment() -> Result<()> {
    beside_target(&origin::Env::from_process()).map(|_| ())
}

/// Split the lead pane and type the fixed side-pane command into the new pane.
pub(crate) fn run_beside(ctx: &Ctx) -> Result<()> {
    let target = beside_target(&origin::Env::from_process())?;
    let executable = std::env::current_exe()
        .context("finding the delegate executable; run the installed binary")?;
    if !executable.is_absolute() {
        bail!("finding the delegate executable; the path is not absolute");
    }
    let executable = executable
        .to_str()
        .context("finding the delegate executable; the path is not UTF-8")?;
    let pane = open_beside(&target, &ctx.root, Path::new(executable))?;
    println!("{pane}");
    Ok(())
}

fn open_beside(target: &BesideTarget, cwd: &Path, executable: &Path) -> Result<String> {
    let executable = executable
        .to_str()
        .context("the delegate executable path is not UTF-8")?;
    let env = [
        ("DELEGATE_LEAD_PANE", target.pane.clone()),
        ("DELEGATE_BIN", executable.to_string()),
    ];
    let pane = origin::pane_split(&target.socket, &target.pane, cwd, &env)
        .map_err(|error| anyhow!("Herdr refused to split the lead pane: {error}"))?;
    if pane == target.pane {
        bail!("Herdr split the lead pane but returned the same pane; no command was typed");
    }
    origin::pane_send_input(&target.socket, &pane, BESIDE_COMMAND, &["Enter"]).map_err(
        |error| anyhow!("Herdr split the pane as {pane} but could not start the side pane: {error}"),
    )?;
    Ok(pane)
}

/// The `[ui.sidebar.agents]` layout that draws the reporter's four task rows.
pub(crate) fn sidebar_config() -> String {
    let rules = [
        ("failed", "#f38ba8"),
        ("blocked", "#f38ba8"),
        ("interrupted", "#f38ba8"),
        ("unavailable", "#f38ba8"),
        ("stalled", "#f9e2af"),
        ("ready", "#a6e3a1"),
        ("running", "#89b4fa"),
        ("checking", "#89b4fa"),
    ];
    let mut rule_text = String::new();
    for (index, (state, colour)) in rules.iter().enumerate() {
        if index > 0 {
            rule_text.push_str(", ");
        }
        let _ = write!(
            rule_text,
            "{{ starts_with = \"{state}\", fg = \"{colour}\" }}"
        );
    }
    let mut task_rows = String::new();
    for key in &KEYS[..4] {
        let _ = writeln!(
            task_rows,
            "    [{{ token = \"${key}\", rules = [{rule_text}] }}],"
        );
    }
    task_rows.push_str("    [\"$dlg_more\"],\n");
    format!(
        "[ui.sidebar.agents]\n\
         [ui.sidebar.agents.rows_by_agent]\n\
         pi = [\n\
         \x20   {DEFAULT_AGENT_ROWS}\n\
         {task_rows}\
         ]\n\
         claude = [\n\
         \x20   {DEFAULT_AGENT_ROWS}\n\
         {task_rows}\
         ]\n"
    )
}

pub(crate) fn print_sidebar_config() {
    println!(
        "Paste this into Herdr's config.toml; delegate never writes it.\n\n{}",
        sidebar_config()
    );
}

struct Viewer {
    socket: Option<PathBuf>,
    pane: String,
    interval: Duration,
    ttl: Duration,
    ttl_ms: u64,
    terminal: bool,
    interactive: bool,
    width: usize,
    stop: Arc<AtomicBool>,
    keys: Arc<Mutex<Vec<Key>>>,
    seq: u64,
    memory: HashMap<String, Memory>,
    lead: LeadState,
    tasks: Option<GoodTasks>,
    tasks_ok: bool,
    selected: Option<Selection>,
    expanded: Option<SelectionKey>,
    vanished: Option<SelectionKey>,
    detail: Option<DetailRead>,
    detail_requested: Option<SelectionKey>,
    detail_tx: Option<Sender<Option<SelectionKey>>>,
}

impl Viewer {
    fn report_loop(&mut self, receiver: &Receiver<Refresh>, output: &mut impl Write) -> Result<()> {
        let mut next_report = Instant::now();
        let mut redraw = true;
        loop {
            loop {
                match receiver.try_recv() {
                    Ok(refresh) => {
                        self.apply(refresh);
                        redraw = redraw || self.interactive;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.send(&cleared());
                        return Ok(());
                    }
                }
            }
            if self.handle_keys() {
                redraw = true;
            }
            if self.stop.load(Ordering::SeqCst) {
                self.send(&cleared());
                return Ok(());
            }
            let now = Instant::now();
            let due = now >= next_report;
            if due {
                self.send(&self.tokens(now));
                next_report = next_report
                    .checked_add(self.interval)
                    .unwrap_or_else(|| now + self.interval);
                if next_report <= now {
                    next_report = now + self.interval;
                }
            }
            if due || redraw {
                let frame = self.frame(now);
                output
                    .write_all(&frame)
                    .context("writing watch output; check the terminal or output destination")?;
                output.flush()?;
                redraw = false;
            }
            let wait = next_report
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50));
            match receiver.recv_timeout(wait) {
                Ok(refresh) => {
                    self.apply(refresh);
                    redraw = redraw || self.interactive;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.send(&cleared());
                    return Ok(());
                }
            }
        }
    }

    fn handle_keys(&mut self) -> bool {
        let mut keys = Vec::new();
        if let Ok(mut pending) = self.keys.lock() {
            std::mem::swap(&mut keys, &mut pending);
        }
        let mut redraw = false;
        for key in keys {
            match key {
                Key::Up => {
                    self.move_selection(false);
                    redraw = true;
                }
                Key::Down => {
                    self.move_selection(true);
                    redraw = true;
                }
                Key::Enter => {
                    self.toggle_expanded();
                    redraw = true;
                }
                Key::Tab => {
                    self.next_group();
                    redraw = true;
                }
                Key::Quit => self.stop.store(true, Ordering::SeqCst),
            }
        }
        redraw
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
        self.detail = refresh.detail;
        self.reconcile_selection();
        self.sync_detail_request();
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

    fn known_lead(&self) -> bool {
        matches!(
            &self.lead,
            LeadState::Known(PaneLead::Supported(Origin::Supported { .. }))
        )
    }

    fn key_of(&self, task: &TaskRead) -> SelectionKey {
        SelectionKey::of(task, &self.memory)
    }

    fn nav_entries(&self) -> Vec<Selection> {
        let mut entries = Vec::new();
        if self.known_lead() {
            for task in self.this_lead() {
                entries.push(Selection::Task(self.key_of(task)));
            }
            for label in ["other leads", "Codex (unsupported)", "unknown lead"] {
                entries.push(Selection::Group(label.into()));
            }
        } else {
            for (label, tasks) in self.grouped() {
                entries.push(Selection::Group(label));
                for task in tasks {
                    entries.push(Selection::Task(self.key_of(task)));
                }
            }
        }
        entries
    }

    fn selected_index(&self, entries: &[Selection]) -> Option<usize> {
        entries
            .iter()
            .position(|entry| Some(entry) == self.selected.as_ref())
    }

    fn reconcile_selection(&mut self) {
        let entries = self.nav_entries();
        if entries.is_empty() {
            if let Some(Selection::Task(key)) = &self.selected {
                self.vanished = Some(key.clone());
            }
            self.expanded = None;
            return;
        }
        match self.selected.clone() {
            Some(Selection::Task(key)) => {
                if entries
                    .iter()
                    .any(|entry| matches!(entry, Selection::Task(known) if known == &key))
                {
                    self.vanished = None;
                } else {
                    self.vanished = Some(key);
                    self.expanded = None;
                }
            }
            Some(Selection::Group(label)) => {
                if !entries
                    .iter()
                    .any(|entry| matches!(entry, Selection::Group(known) if known == &label))
                {
                    self.selected = Some(entries[0].clone());
                    self.vanished = None;
                    self.expanded = None;
                }
            }
            None => {
                self.selected = Some(entries[0].clone());
                self.vanished = None;
            }
        }
        if let Some(expanded) = &self.expanded
            && (self.vanished.is_some()
                || !matches!(&self.selected, Some(Selection::Task(key)) if key == expanded)
                || !self.task_is_view(expanded))
        {
            self.expanded = None;
        }
    }

    fn move_selection(&mut self, down: bool) {
        let entries = self.nav_entries();
        if entries.is_empty() {
            self.selected = None;
            self.vanished = None;
            self.expanded = None;
            self.detail = None;
            return;
        }
        let Some(index) = self.selected_index(&entries) else {
            self.selected = Some(entries[0].clone());
            self.vanished = None;
            self.expanded = None;
            self.detail = None;
            return;
        };
        let next = if down {
            (index + 1).min(entries.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        self.selected = Some(entries[next].clone());
        self.vanished = None;
        self.expanded = None;
        self.detail = None;
    }

    fn task(&self, key: &SelectionKey) -> Option<&TaskRead> {
        let good = self.tasks.as_ref()?;
        good.tasks
            .iter()
            .find(|task| self.key_of(task) == *key)
    }

    fn task_is_view(&self, key: &SelectionKey) -> bool {
        matches!(self.task(key), Some(TaskRead::View { .. }))
    }

    fn toggle_expanded(&mut self) {
        let Some(Selection::Task(key)) = self.selected.clone() else {
            return;
        };
        if self.vanished.is_some() || !self.task_is_view(&key) {
            self.expanded = None;
            self.detail = None;
            return;
        }
        self.expanded = if self.expanded.as_ref() == Some(&key) {
            None
        } else {
            Some(key)
        };
        self.detail = None;
    }

    fn group_spans(&self) -> Vec<(usize, usize, Selection)> {
        let mut spans = Vec::new();
        if self.known_lead() {
            let tasks = self.this_lead().len();
            let entries = self.nav_entries();
            if tasks > 0 {
                spans.push((0, tasks - 1, entries[0].clone()));
            }
            for (offset, label) in ["other leads", "Codex (unsupported)", "unknown lead"]
                .into_iter()
                .enumerate()
            {
                let index = tasks + offset;
                spans.push((index, index, Selection::Group(label.into())));
            }
        } else {
            let mut index = 0;
            for (label, tasks) in self.grouped() {
                let last = index + tasks.len();
                spans.push((index, last, Selection::Group(label)));
                index = last + 1;
            }
        }
        spans
    }

    fn next_group(&mut self) {
        let entries = self.nav_entries();
        let spans = self.group_spans();
        if spans.is_empty() {
            return;
        }
        let current = self
            .selected_index(&entries)
            .and_then(|index| spans.iter().position(|(first, last, _)| index >= *first && index <= *last));
        let next = current.map_or(0, |index| (index + 1) % spans.len());
        self.selected = Some(spans[next].2.clone());
        self.vanished = None;
        self.expanded = None;
        self.detail = None;
    }

    fn sync_detail_request(&mut self) {
        let desired = if self.vanished.is_none()
            && let Some(Selection::Task(key)) = &self.selected
            && let Some(expanded) = &self.expanded
            && key == expanded
            && self.task_is_view(key)
        {
            Some(key.clone())
        } else {
            None
        };
        if desired == self.detail_requested {
            return;
        }
        self.detail_requested.clone_from(&desired);
        if let Some(sender) = &self.detail_tx {
            let _ = sender.send(desired);
        }
    }

    fn detail_task(&self) -> Option<&TaskRead> {
        let key = self.expanded.as_ref()?;
        if self.vanished.as_ref() == Some(key) {
            return None;
        }
        let detail = self.detail.as_ref()?;
        if &detail.key != key {
            return None;
        }
        let tasks = detail.tasks.as_ref().ok()?;
        tasks.iter().find(|task| self.key_of(task) == *key)
    }

    fn plain_text(&self, now: Instant) -> String {
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
        frame
    }

    fn frame(&self, now: Instant) -> Vec<u8> {
        let frame = if self.interactive {
            self.interactive_text(now)
        } else {
            self.plain_text(now)
        };
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

    fn interactive_text(&self, now: Instant) -> String {
        let mut frame = String::new();
        push_line(&mut frame, &self.header(), self.width);
        push_line(
            &mut frame,
            &format!("health · {}", self.health(now)),
            self.width,
        );
        if let Some(key) = &self.vanished {
            push_line(
                &mut frame,
                &format!("! task no longer available · {}", key.id),
                self.width,
            );
        }
        let lead_known = self.known_lead();
        if self.tasks.is_none() {
            push_line(
                &mut frame,
                "tasks unavailable; no successful read yet",
                self.width,
            );
        } else if lead_known {
            let this_lead = self.this_lead();
            if this_lead.is_empty() {
                push_line(&mut frame, "no tasks for this lead", self.width);
            }
            for task in &this_lead {
                self.push_interactive_task(&mut frame, task);
            }
            let (other, codex, unknown, unknown_unavailable) = self.counts();
            self.push_group(&mut frame, "other leads", other);
            self.push_group(&mut frame, "Codex (unsupported)", codex);
            self.push_group(&mut frame, "unknown lead", unknown);
            if unknown_unavailable > 0 {
                self.push_group(&mut frame, "unknown lead · unavailable", unknown_unavailable);
            }
        } else {
            let groups = self.grouped();
            if groups.is_empty() {
                push_line(&mut frame, "no tasks", self.width);
            }
            for (label, tasks) in groups {
                let line = self.marked(&Selection::Group(label.clone()), &label);
                push_line(&mut frame, &line, self.width);
                for task in tasks.iter().take(12) {
                    self.push_interactive_task(&mut frame, task);
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
        frame
    }

    fn push_interactive_task(&self, frame: &mut String, task: &TaskRead) {
        let key = SelectionKey::of(task, &self.memory);
        let line = self.marked(&Selection::Task(key.clone()), &row(task));
        push_line(frame, &line, self.width);
        self.push_activity(frame, task);
        if self.vanished.is_none()
            && self.expanded.as_ref() == Some(&key)
            && let Some(detail) = self.detail_task()
        {
            self.push_detail(frame, detail);
        } else if self.expanded.as_ref() == Some(&key) {
            push_line(frame, "  activity: loading…", self.width);
        }
    }

    fn push_group(&self, frame: &mut String, label: &str, count: usize) {
        let line = self.marked(&Selection::Group(label.to_string()), &format!("{label} · {count}"));
        push_line(frame, &line, self.width);
    }

    fn marked(&self, selection: &Selection, text: &str) -> String {
        if self.selected.as_ref() == Some(selection) {
            format!("> {text}")
        } else {
            format!("  {text}")
        }
    }

    fn push_detail(&self, frame: &mut String, task: &TaskRead) {
        match task {
            TaskRead::Unavailable { reason, .. } => {
                push_line(frame, &format!("  unavailable · {reason}"), self.width);
            }
            TaskRead::View { view, ready, .. } => {
                push_line(
                    frame,
                    &format!("  worker · {}", view.worker.as_deref().unwrap_or("-")),
                    self.width,
                );
                push_line(frame, &format!("  tier · {}", view.tier), self.width);
                let readiness = match ready {
                    Some(true) => "ready",
                    Some(false) => "not ready",
                    None => "unknown",
                };
                push_line(frame, &format!("  readiness · {readiness}"), self.width);
                for file in &view.changed_files {
                    push_line(
                        frame,
                        &format!("  file · {} +{} -{}", file.path, file.added, file.removed),
                        self.width,
                    );
                }
                if let Some(next) = &view.next {
                    push_line(frame, &format!("  next · {next}"), self.width);
                }
                if view.steps.is_empty() {
                    push_line(frame, "  No activity yet", self.width);
                }
                for step in &view.steps {
                    push_line(
                        frame,
                        &format!(
                            "  step · {}  {}  {}  {}",
                            step["at"].as_str().and_then(|at| at.get(11..19)).unwrap_or("--:--:--"),
                            step["kind"].as_str().unwrap_or(""),
                            step["target"].as_str().unwrap_or(""),
                            step["result"].as_str().unwrap_or("")
                        ),
                        self.width,
                    );
                }
            }
        }
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
) -> (Receiver<Refresh>, Sender<Option<SelectionKey>>) {
    let (sender, receiver) = mpsc::channel();
    let (detail_sender, detail_receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut detail_key: Option<SelectionKey> = None;
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
            let detail = detail_key.clone().and_then(|key| {
                let selected = tasks.as_ref().is_ok_and(|tasks| {
                    tasks.iter().any(|task| {
                        matches!(task, TaskRead::View { view, .. } if SelectionKey::of_view(view) == key)
                    })
                });
                selected.then(|| DetailRead {
                    key: key.clone(),
                    tasks: status::read_tasks(
                        &ctx,
                        Budget {
                            steps: DETAIL_STEPS,
                            ..Budget::default()
                        },
                    )
                    .map_err(|error| format!("{error:#}")),
                })
            });
            let finished = Instant::now();
            if sender
                .send(Refresh {
                    lead,
                    tasks,
                    detail,
                    finished,
                })
                .is_err()
            {
                return;
            }
            match detail_receiver.recv_timeout(interval) {
                Ok(key) => detail_key = key,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            while let Ok(key) = detail_receiver.try_recv() {
                detail_key = key;
            }
        }
    });
    (receiver, detail_sender)
}

fn spawn_key_reader(stop: Arc<AtomicBool>, keys: Arc<Mutex<Vec<Key>>>) {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut parser = KeyParser::default();
        let mut byte = [0_u8; 1];
        loop {
            match stdin.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let Some(key) = parser.push(byte[0]) else {
                        continue;
                    };
                    if key == Key::Quit {
                        stop.store(true, Ordering::SeqCst);
                        return;
                    }
                    if let Ok(mut pending) = keys.lock()
                        && pending.len() < 16
                    {
                        pending.push(key);
                    }
                }
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
                        } else if method == "pane.split" {
                            json!({
                                "id": id,
                                "result": {
                                    "type": "pane_info",
                                    "pane": {"pane_id": "w1:p101", "workspace_id": "w1"}
                                }
                            })
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
            interactive: false,
            width: 100,
            stop: Arc::new(AtomicBool::new(false)),
            keys: Arc::new(Mutex::new(Vec::new())),
            seq: 0,
            memory: HashMap::new(),
            lead,
            tasks,
            tasks_ok,
            selected: None,
            expanded: None,
            vanished: None,
            detail: None,
            detail_requested: None,
            detail_tx: None,
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

    fn key(id: &str) -> SelectionKey {
        SelectionKey {
            id: id.into(),
            instance: Some(format!("{id}-instance")),
        }
    }

    fn known_lead() -> LeadState {
        LeadState::Known(PaneLead::Supported(supported(
            "pi",
            "/sessions/lead-a.jsonl",
        )))
    }

    #[test]
    fn keys_parse_arrows_enter_tab_and_quit() {
        let mut parser = KeyParser::default();
        for byte in [0x1b, b'['] {
            assert_eq!(parser.push(byte), None);
        }
        assert_eq!(parser.push(b'A'), Some(Key::Up));
        for byte in [0x1b, b'['] {
            assert_eq!(parser.push(byte), None);
        }
        assert_eq!(parser.push(b'B'), Some(Key::Down));
        assert_eq!(parser.push(b'\r'), Some(Key::Enter));
        assert_eq!(parser.push(b'\t'), Some(Key::Tab));
        assert_eq!(parser.push(b'q'), Some(Key::Quit));
        assert_eq!(parser.push(0x03), Some(Key::Quit));
    }

    #[test]
    fn selection_starts_on_the_first_row_then_moves_to_collapsed_groups() {
        let now = Instant::now();
        let lead = known_lead();
        let tasks = vec![
            view("t1", "implement", None, false),
            view("t2", "done", Some("pass"), true),
            view("t3", "not_started", None, false),
        ];
        let mut viewer = viewer(
            None,
            lead.clone(),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        viewer.reconcile_selection();
        assert_eq!(viewer.selected, Some(Selection::Task(key("t2"))));
        viewer.move_selection(true);
        assert_eq!(viewer.selected, Some(Selection::Task(key("t1"))));
        viewer.move_selection(true);
        assert_eq!(viewer.selected, Some(Selection::Task(key("t3"))));
        viewer.move_selection(true);
        assert_eq!(viewer.selected, Some(Selection::Group("other leads".into())));
        viewer.next_group();
        assert_eq!(
            viewer.selected,
            Some(Selection::Group("Codex (unsupported)".into()))
        );
        viewer.next_group();
        assert_eq!(viewer.selected, Some(Selection::Group("unknown lead".into())));
        viewer.next_group();
        assert_eq!(viewer.selected, Some(Selection::Task(key("t2"))));
    }

    #[test]
    fn enter_expands_and_collapses_the_selected_task() {
        let now = Instant::now();
        let lead = known_lead();
        let tasks = vec![view("t1", "implement", None, false)];
        let mut viewer = viewer(
            None,
            lead,
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        viewer.reconcile_selection();
        let (sender, receiver) = mpsc::channel();
        viewer.detail_tx = Some(sender);
        viewer.toggle_expanded();
        assert_eq!(viewer.expanded, Some(key("t1")));
        viewer.sync_detail_request();
        assert_eq!(receiver.try_recv().unwrap(), Some(key("t1")));
        viewer.toggle_expanded();
        assert_eq!(viewer.expanded, None);
        viewer.sync_detail_request();
        assert_eq!(receiver.try_recv().unwrap(), None);
    }

    #[test]
    fn a_vanished_selection_is_not_replaced() {
        let now = Instant::now();
        let lead = known_lead();
        let tasks = vec![
            view("t1", "implement", None, false),
            view("t2", "implement", None, false),
        ];
        let mut viewer = viewer(
            None,
            lead.clone(),
            Some(GoodTasks { tasks, at: now }),
            true,
            Duration::from_secs(6),
        );
        viewer.reconcile_selection();
        assert_eq!(viewer.selected, Some(Selection::Task(key("t1"))));
        viewer.apply(Refresh {
            lead,
            tasks: Ok(vec![view("t2", "implement", None, false)]),
            detail: None,
            finished: now,
        });
        assert_eq!(viewer.vanished, Some(key("t1")));
        assert_eq!(viewer.selected, Some(Selection::Task(key("t1"))));
        viewer.interactive = true;
        let frame = String::from_utf8(viewer.frame(now)).unwrap();
        assert!(frame.contains("task no longer available"));
        assert!(!frame.contains('\u{1b}'), "the frame carries terminal controls");
    }

    #[test]
    fn beside_splits_the_lead_and_types_only_the_fixed_command() {
        let fixture = fixture();
        let target = BesideTarget {
            socket: fixture.path.clone(),
            pane: "w1:p1".into(),
        };
        let pane = open_beside(
            &target,
            Path::new("/tmp/checkout"),
            Path::new("/opt/delegate/bin/delegate"),
        )
        .unwrap();
        assert_eq!(pane, "w1:p101");
        let requests = fixture.requests.lock().unwrap();
        let split = requests
            .iter()
            .find(|request| request["method"] == "pane.split")
            .unwrap();
        let params = &split["params"];
        assert_eq!(params["target_pane_id"], "w1:p1");
        assert_eq!(params["direction"], "right");
        assert_eq!(params["focus"], false);
        assert_eq!(params["cwd"], "/tmp/checkout");
        assert_eq!(params["env"]["DELEGATE_LEAD_PANE"], "w1:p1");
        assert_eq!(params["env"]["DELEGATE_BIN"], "/opt/delegate/bin/delegate");
        let inputs: Vec<&Value> = requests
            .iter()
            .filter(|request| request["method"] == "pane.send_input")
            .collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0]["params"]["pane_id"], "w1:p101");
        assert_eq!(inputs[0]["params"]["text"], BESIDE_COMMAND);
        assert_eq!(inputs[0]["params"]["keys"], json!(["Enter"]));
        assert!(
            requests.iter().all(|request| request["method"] != "pane.send_input"
                || request["params"]["pane_id"] != "w1:p1"),
            "input reached the lead pane"
        );
    }

    #[test]
    fn beside_refuses_without_a_herdr_lead_pane() {
        let socket = PathBuf::from("/tmp/herdr.sock");
        let cases = [
            origin::Env {
                sandboxed: false,
                socket: None,
                pane_id: Some("w1:p1".into()),
                claude_session: None,
            },
            origin::Env {
                sandboxed: false,
                socket: Some(socket.clone()),
                pane_id: None,
                claude_session: None,
            },
            origin::Env {
                sandboxed: true,
                socket: Some(socket.clone()),
                pane_id: Some("w1:p1".into()),
                claude_session: None,
            },
        ];
        for env in cases {
            let error = beside_target(&env).unwrap_err().to_string();
            assert!(error.to_lowercase().contains("herdr"), "{error}");
        }
    }

    #[test]
    fn sidebar_config_parses_and_colours_state_first() {
        let value: toml::Value = toml::from_str(&sidebar_config()).unwrap();
        let agents = value["ui"]["sidebar"]["agents"]["rows_by_agent"]
            .as_table()
            .unwrap();
        for agent in ["pi", "claude"] {
            let rows = agents[agent].as_array().unwrap();
            assert_eq!(rows[0].as_array().unwrap()[0].as_str(), Some("state_icon"));
            assert_eq!(rows[1].as_array().unwrap()[0].as_str(), Some("agent"));
            let tokens: Vec<&toml::Value> = rows
                .iter()
                .flat_map(|row| row.as_array().unwrap())
                .collect();
            for want in ["$dlg_t1", "$dlg_t2", "$dlg_t3", "$dlg_t4", "$dlg_more"] {
                assert!(
                    tokens.iter().any(|token| {
                        token.as_str() == Some(want)
                            || token.get("token").and_then(toml::Value::as_str) == Some(want)
                    }),
                    "{agent} lacks {want}"
                );
            }
            for token in tokens {
                let Some(table) = token.as_table() else {
                    continue;
                };
                let Some(rules) = table.get("rules").and_then(toml::Value::as_array) else {
                    continue;
                };
                for rule in rules {
                    let rule = rule.as_table().unwrap();
                    assert_eq!(
                        rule.keys().filter(|key| *key != "fg").collect::<Vec<_>>(),
                        vec!["starts_with"]
                    );
                }
            }
        }
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
            detail: None,
            finished: now,
        });
        assert_eq!(viewer.tokens(now)[0].as_deref(), Some("running · t1"));

        viewer.apply(Refresh {
            lead: lead.clone(),
            tasks: Ok(vec![TaskRead::Unavailable {
                id: "t1".into(),
                reason: "bad STATE.json".into(),
            }]),
            detail: None,
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
            detail: None,
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
            detail: None,
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
        let mut parser = KeyParser::default();
        assert_eq!(parser.push(b'q'), Some(Key::Quit));
        assert_eq!(parser.push(0x03), Some(Key::Quit));
        assert_eq!(parser.push(b'Q'), None);
        assert_eq!(parser.push(b'x'), None);

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

    fn signal_test_ctx(root: &Path) -> Ctx {
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
