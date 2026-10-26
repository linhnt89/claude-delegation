//! Building and running worker commands, inside the bwrap sandbox.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::config::{Harness, Worker, home};

#[path = "events.rs"]
pub(crate) mod events;

pub const WORKER_NOTE: &str = "You are a delegated worker, not the lead. Do only what .task/SPEC.md asks. \
     Ignore any instructions in CLAUDE.md about leading, delegating or distilling.";

/// A session shared by the question phase and the work phase.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub resume: bool,
}

/// What a worker may touch: everything is read-only, `hide` is replaced by empty dirs,
/// `ro` is re-exposed read-only, `rw` is writable.
#[derive(Debug, Default)]
pub struct Bounds {
    pub hide: Vec<PathBuf>,
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    /// Let nested `delegate where` locate tasks without opening hidden metadata.
    pub task_home: Option<PathBuf>,
}

fn state_dirs(harness: Option<Harness>) -> Vec<PathBuf> {
    let h = home();
    let mut dirs = match harness {
        Some(Harness::Pi) => vec![h.join(".pi")],
        Some(Harness::Claude) => vec![h.join(".claude"), h.join(".claude.json")],
        Some(Harness::Opencode) => vec![
            h.join(".local/share/opencode"),
            h.join(".config/opencode"),
            h.join(".local/state/opencode"),
        ],
        Some(Harness::Gemini) => vec![h.join(".gemini")],
        Some(Harness::Codex) | None => vec![],
    };
    dirs.push(h.join(".cache"));
    dirs
}

pub(crate) fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|d| {
            d.join(program)
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

/// Acceptance checks need bwrap even when every worker uses Codex's own sandbox.
pub(crate) fn needs_bwrap(
    candidates: impl IntoIterator<Item = Harness>,
    research: bool,
    sandbox_disabled: bool,
) -> bool {
    !sandbox_disabled && (!research || candidates.into_iter().any(|h| h != Harness::Codex))
}

pub(crate) fn require_bwrap() -> Result<()> {
    if !on_path("bwrap") {
        bail!("bwrap not found; install bubblewrap or set DELEGATE_SANDBOX=0");
    }
    Ok(())
}

/// bwrap prefix for a worker (or for acceptance checks when `harness` is None).
/// Codex keeps its own sandbox; `DELEGATE_SANDBOX=0` disables sandboxing.
pub fn sandbox(harness: Option<Harness>, bounds: &Bounds, chdir: &Path) -> Result<Vec<String>> {
    if harness == Some(Harness::Codex) || std::env::var("DELEGATE_SANDBOX").is_ok_and(|v| v == "0")
    {
        return Ok(vec![]);
    }
    require_bwrap()?;
    let mut sb: Vec<String> = [
        "bwrap",
        "--die-with-parent",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
    ]
    .map(String::from)
    .to_vec();
    sb.extend(["--tmpfs".into(), "/tmp".into()]);
    let s = |p: &Path| p.to_string_lossy().into_owned();
    for p in &bounds.hide {
        sb.extend(["--tmpfs".into(), s(p)]);
    }
    for p in &bounds.ro {
        sb.extend(["--ro-bind".into(), s(p), s(p)]);
    }
    for p in state_dirs(harness).iter().chain(&bounds.rw) {
        if p.exists() {
            sb.extend(["--bind".into(), s(p), s(p)]);
        }
    }
    // Hide the vault and task metadata even when data lives under a writable
    // harness directory. Then expose only explicitly granted descendants: task
    // worktrees now live inside data, but their sibling state must stay hidden.
    let data = crate::vault::data_dir();
    if data.exists() {
        sb.extend(["--tmpfs".into(), s(&data)]);
        for p in &bounds.ro {
            if p.starts_with(&data) && p != &data && p.exists() {
                sb.extend(["--ro-bind".into(), s(p), s(p)]);
            }
        }
        for p in &bounds.rw {
            if p.starts_with(&data) && p != &data && p.exists() {
                sb.extend(["--bind".into(), s(p), s(p)]);
            }
        }
    }
    if let Some(task_home) = &bounds.task_home {
        sb.extend(["--setenv".into(), "DELEGATE_HOME".into(), s(task_home)]);
    }
    sb.extend(["--chdir".into(), s(chdir)]);
    Ok(sb)
}

/// Command for one worker turn.
pub fn command(
    w: &Worker,
    prompt: &str,
    session: Option<&Session>,
    read_only: bool,
    out_file: Option<&Path>,
) -> Vec<String> {
    let mut c: Vec<String> = Vec::new();
    let model = (!w.model.is_empty()).then_some(w.model.as_str());
    let effort = w.effort.map(crate::config::Effort::as_str);
    let mut add = |args: &[&str]| c.extend(args.iter().map(|a| (*a).to_string()));
    match w.harness {
        Harness::Pi => {
            add(&["pi", "-p", "--approve"]);
            match session {
                Some(s) => add(&["--session-id", &s.id]),
                None => add(&["--no-session"]),
            }
            if let Some(m) = model {
                add(&["--model", m]);
            }
            if let Some(e) = effort {
                add(&["--thinking", e]);
            }
        }
        Harness::Claude => {
            add(&[
                "claude",
                "-p",
                "--dangerously-skip-permissions",
                "--setting-sources",
                "project",
            ]);
            add(&[
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--append-system-prompt",
                WORKER_NOTE,
            ]);
            match session {
                Some(s) if s.resume => add(&["--resume", &s.id]),
                Some(s) => add(&["--session-id", &s.id]),
                None => add(&["--no-session-persistence"]),
            }
            if let Some(m) = model {
                add(&["--model", m]);
            }
            if let Some(e) = effort {
                add(&["--effort", e]);
            }
            if read_only {
                add(&["--disallowedTools", "Edit,Write,NotebookEdit"]);
            }
        }
        Harness::Codex => {
            add(&["codex", "exec"]);
            if session.is_some_and(|s| s.resume) {
                add(&["resume"]);
            }
            let mode = if read_only {
                "read-only"
            } else {
                "workspace-write"
            };
            add(&["--json", "-c", &format!("sandbox_mode=\"{mode}\"")]);
            if let Some(m) = model {
                add(&["-m", m]);
            }
            if let Some(e) = effort {
                // Codex has no "max"; xhigh is its top setting.
                let e = if e == "max" { "xhigh" } else { e };
                add(&["-c", &format!("model_reasoning_effort=\"{e}\"")]);
            }
            if let Some(f) = out_file {
                add(&["-o", &f.to_string_lossy()]);
            }
            if let Some(s) = session.filter(|s| s.resume) {
                add(&[&s.id]);
            }
        }
        Harness::Opencode => {
            add(&["opencode", "run", "--auto"]);
            if let Some(s) = session {
                add(&["--session", &s.id]);
            }
            if let Some(m) = model {
                add(&["-m", m]);
            }
        }
        Harness::Gemini => {
            let mode = if read_only { "plan" } else { "auto_edit" };
            add(&["gemini", "--approval-mode", mode, "--skip-trust"]);
            if let Some(m) = model {
                add(&["-m", m]);
            }
            add(&["-p"]);
        }
    }
    c.push(prompt.to_string());
    c
}

/// Streaming flags are confined to run; reviews retain their existing output format.
pub(crate) fn streaming_command(
    w: &Worker,
    prompt: &str,
    session: Option<&Session>,
) -> Vec<String> {
    let mut cmd = command(w, prompt, session, false, None);
    let flags: &[&str] = match w.harness {
        Harness::Claude => &["--output-format", "stream-json", "--verbose"],
        Harness::Opencode => &["--format", "json"],
        Harness::Pi => &["--mode", "json"],
        Harness::Codex | Harness::Gemini => &[],
    };
    let position = cmd.len().saturating_sub(1);
    cmd.splice(
        position..position,
        flags.iter().map(|flag| (*flag).to_owned()),
    );
    cmd
}

/// Read stdout incrementally while stderr continues directly to the ordinary log.
pub(crate) fn run_streamed(
    cmd: &[String],
    cwd: &Path,
    log: &Path,
    stream: &mut events::Stream,
) -> Result<i32> {
    let stderr = match OpenOptions::new().create(true).append(true).open(log) {
        Ok(mut lf) => {
            if let Err(error) = writeln!(
                lf,
                "\n$ {} ...",
                cmd.iter().take(12).cloned().collect::<Vec<_>>().join(" ")
            ) {
                let _ = writeln!(
                    lf,
                    "warning: writing run.log: {error}; check disk space and permissions"
                );
            }
            Stdio::from(lf)
        }
        Err(_) => Stdio::null(),
    };
    // A socket gives us safe, bounded reads without new dependencies or unsafe
    // fcntl calls. Descendants may inherit stdout long after the worker exits.
    let (pipe, output) = UnixStream::pair().context("opening worker stdout")?;
    pipe.set_read_timeout(Some(Duration::from_millis(100)))?;
    let (program, args) = cmd.split_first().context("empty command")?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(output)))
        .stderr(stderr)
        .spawn()
        .with_context(|| format!("starting {program}"))?;
    let mut exited = None;
    let drained = stream.drain_until(pipe, log, || {
        if exited.is_none() && child.try_wait().context("checking worker exit")?.is_some() {
            exited = Some(Instant::now());
        }
        Ok(exited.is_some_and(|at| at.elapsed() >= Duration::from_secs(1)))
    });
    let status = child.wait().context("waiting for worker")?;
    drained?;
    Ok(status.code().unwrap_or(-1))
}

/// Whether a harness can continue the question-phase session in the work phase.
pub fn can_resume(h: Harness) -> bool {
    !matches!(h, Harness::Gemini)
}

/// Fresh session id for harnesses that take one up front (Codex assigns its own).
pub fn new_session(h: Harness) -> Option<Session> {
    matches!(h, Harness::Pi | Harness::Claude | Harness::Opencode).then(|| Session {
        id: uuid::Uuid::new_v4().to_string(),
        resume: false,
    })
}

/// Run a command, appending stderr (and stdout unless `stdout_file`) to `log`. Returns the exit code.
pub fn run_logged(
    cmd: &[String],
    cwd: &Path,
    log: &Path,
    stdout_file: Option<&Path>,
) -> Result<i32> {
    let mut lf = OpenOptions::new().create(true).append(true).open(log)?;
    let shown: Vec<&str> = cmd.iter().take(12).map(String::as_str).collect();
    writeln!(
        lf,
        "\n$ {}{}",
        shown.join(" "),
        if cmd.len() > 12 { " ..." } else { "" }
    )?;
    let stdout = match stdout_file {
        Some(f) => Stdio::from(File::create(f)?),
        None => Stdio::from(lf.try_clone()?),
    };
    let (program, args) = cmd.split_first().context("empty command")?;
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::from(lf))
        .status()
        .with_context(|| format!("starting {program}"))?;
    Ok(status.code().unwrap_or(-1))
}

fn last_log_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        // Skip the "$ <command>" header run_logged writes; only the worker's own words help.
        .filter(|line| !line.trim().is_empty() && !line.starts_with("$ "))
        .take(3)
        .map(|line| line.chars().take(300).collect())
        .collect();
    lines.reverse();
    lines
}

/// Last three non-empty lines written since this attempt's starting byte offset.
pub(crate) fn log_tail(log: &Path, start: u64) -> Result<Vec<String>> {
    let mut file = File::open(log)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(last_log_lines(&String::from_utf8_lossy(&bytes)))
}

/// The latest Codex thread id in a log of `codex exec --json` output.
pub fn codex_thread(log: &Path) -> Option<String> {
    // Run stdout now lives in stream.jsonl; retain legacy-log compatibility.
    let text = std::fs::read(log.with_file_name("stream.jsonl"))
        .or_else(|_| std::fs::read(log))
        .ok()?;
    text.split(|byte| *byte == b'\n').rev().find_map(|line| {
        let v: serde_json::Value = serde_json::from_slice(line).ok()?;
        (v["type"] == "thread.started")
            .then(|| v["thread_id"].as_str().map(String::from))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Worker {
        s.parse().unwrap()
    }

    #[test]
    fn data_is_hidden_after_state_and_worktree_binds() {
        if std::env::var_os("DELEGATE_TEST_DATA_HIDE").is_some() {
            let data = crate::vault::data_dir();
            let root = home();
            let bounds = Bounds {
                rw: vec![root.join(".cache")],
                ..Bounds::default()
            };
            for harness in [Some(Harness::Pi), Some(Harness::Opencode), None] {
                let args = sandbox(harness, &bounds, &root).unwrap();
                let data_hide = args
                    .windows(2)
                    .position(|w| w == ["--tmpfs", data.to_str().unwrap()])
                    .unwrap();
                let last_bind = args.iter().rposition(|a| a == "--bind").unwrap();
                assert!(data_hide > last_bind, "{args:?}");
            }
            assert_eq!(
                sandbox(Some(Harness::Codex), &bounds, &root).unwrap(),
                Vec::<String>::new()
            );
            let wt = data.join("checkouts/test-project/t001/worktree");
            let admin = data.join("git/worktrees/task");
            std::fs::create_dir_all(&wt).unwrap();
            std::fs::create_dir_all(&admin).unwrap();
            let bounds = Bounds {
                ro: vec![data.join("git")],
                rw: vec![admin.clone(), wt.clone()],
                task_home: Some(data.join("checkouts/test-project")),
                ..Bounds::default()
            };
            let args = sandbox(Some(Harness::Pi), &bounds, &wt).unwrap();
            let hide = args
                .windows(2)
                .position(|w| w == ["--tmpfs", data.to_str().unwrap()])
                .unwrap();
            for path in [wt, admin] {
                let bind = args
                    .windows(3)
                    .rposition(|w| w == ["--bind", path.to_str().unwrap(), path.to_str().unwrap()])
                    .unwrap();
                assert!(bind > hide, "{args:?}");
            }
            assert!(args.windows(3).any(|w| w
                == [
                    "--setenv",
                    "DELEGATE_HOME",
                    data.join("checkouts/test-project").to_str().unwrap()
                ]));
            return;
        }
        let root =
            std::env::temp_dir().join(format!("delegate-data-hide-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".cache/delegate")).unwrap();
        std::fs::create_dir(root.join(".pi")).unwrap();
        std::fs::create_dir(root.join("bin")).unwrap();
        let bwrap = root.join("bin/bwrap");
        std::fs::write(&bwrap, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "harness::tests::data_is_hidden_after_state_and_worktree_binds",
            ])
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join(".cache"))
            .env("PATH", root.join("bin"))
            .env("DELEGATE_TEST_DATA_HIDE", "1")
            .env_remove("DELEGATE_SANDBOX")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn bwrap_is_needed_for_workers_or_acceptance_checks() {
        for sandbox_disabled in [false, true] {
            for research in [false, true] {
                assert_eq!(
                    needs_bwrap([Harness::Codex], research, sandbox_disabled),
                    !sandbox_disabled && !research
                );
                for harness in [
                    Harness::Pi,
                    Harness::Claude,
                    Harness::Opencode,
                    Harness::Gemini,
                ] {
                    assert_eq!(
                        needs_bwrap([Harness::Codex, harness], research, sandbox_disabled),
                        !sandbox_disabled
                    );
                }
            }
        }
    }

    #[test]
    fn log_tail_selects_last_three_non_empty_lines_in_order() {
        assert_eq!(last_log_lines("\n \t\n"), Vec::<String>::new());
        assert_eq!(last_log_lines("first\n\nsecond\n"), ["first", "second"]);
        assert_eq!(
            last_log_lines("old\nfirst\n \nsecond\r\n\t\nthird\n\n"),
            ["first", "second", "third"]
        );
        assert_eq!(
            last_log_lines("\n$ bwrap --die-with-parent pi -p\nError: 401 Unauthorized\n"),
            ["Error: 401 Unauthorized"]
        );
    }

    #[test]
    fn log_tail_reads_only_current_attempt() {
        let log = std::env::temp_dir().join(format!("delegate-log-{}", uuid::Uuid::new_v4()));
        let previous = "old round\nold candidate\n";
        std::fs::write(&log, format!("{previous}\n401 Unauthorized\n\n")).unwrap();
        assert_eq!(
            log_tail(&log, previous.len() as u64).unwrap(),
            ["401 Unauthorized"]
        );
        assert_eq!(
            log_tail(&log, std::fs::metadata(&log).unwrap().len()).unwrap(),
            Vec::<String>::new()
        );
        std::fs::remove_file(log).unwrap();
    }

    #[test]
    fn pi_command_maps_model_and_effort() {
        let s = Session {
            id: "abc".into(),
            resume: false,
        };
        let c = command(
            &w("pi:openai-codex/gpt-6.1-sol@high"),
            "go",
            Some(&s),
            false,
            None,
        );
        assert_eq!(
            c,
            [
                "pi",
                "-p",
                "--approve",
                "--session-id",
                "abc",
                "--model",
                "openai-codex/gpt-6.1-sol",
                "--thinking",
                "high",
                "go"
            ]
        );
    }

    #[test]
    fn claude_command_resumes_and_isolates() {
        let s = Session {
            id: "u".into(),
            resume: true,
        };
        let c = command(&w("claude:sonnet@max"), "go", Some(&s), false, None);
        assert!(c.windows(2).any(|p| p == ["--resume", "u"]));
        assert!(c.windows(2).any(|p| p == ["--effort", "max"]));
        assert!(c.windows(2).any(|p| p == ["--setting-sources", "project"]));
        assert_eq!(c.last().unwrap(), "go");
    }

    #[test]
    fn codex_command_resume_order_and_effort_cap() {
        let s = Session {
            id: "thread-1".into(),
            resume: true,
        };
        let c = command(&w("codex:gpt-6.1-sol@max"), "go", Some(&s), false, None);
        assert_eq!(&c[..3], ["codex", "exec", "resume"]);
        assert!(c.contains(&"model_reasoning_effort=\"xhigh\"".to_string()));
        assert_eq!(&c[c.len() - 2..], ["thread-1", "go"]);
        let c = command(
            &w("codex:gpt-6.1-sol"),
            "go",
            None,
            true,
            Some(Path::new("/o.md")),
        );
        assert!(c.contains(&"sandbox_mode=\"read-only\"".to_string()));
        assert!(c.windows(2).any(|p| p == ["-o", "/o.md"]));
    }

    #[test]
    fn run_stream_flags_do_not_change_review_commands() {
        for (worker, flags) in [
            ("pi:test", vec!["--mode", "json"]),
            (
                "claude:test",
                vec!["--output-format", "stream-json", "--verbose"],
            ),
            ("opencode:test", vec!["--format", "json"]),
            ("codex:test", vec![]),
            ("gemini:test", vec![]),
        ] {
            let worker = w(worker);
            let plain = command(&worker, "go", None, false, None);
            let mut expected = plain.clone();
            expected.splice(
                expected.len() - 1..expected.len() - 1,
                flags.iter().map(ToString::to_string),
            );
            assert_eq!(streaming_command(&worker, "go", None), expected);
            assert_eq!(command(&worker, "go", None, false, None), plain);
        }
    }

    #[test]
    fn codex_resume_reads_raw_stream_despite_bad_bytes() {
        let dir = std::env::temp_dir().join(format!("delegate-thread-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("run.log");
        std::fs::write(&log, "plain output\n").unwrap();
        std::fs::write(
            dir.join("stream.jsonl"),
            b"\xff\n{\"type\":\"thread.started\",\"thread_id\":\"resumed\"}\ninvalid\n",
        )
        .unwrap();
        assert_eq!(codex_thread(&log).as_deref(), Some("resumed"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_codex_thread() {
        let dir = std::env::temp_dir().join(format!("delegate-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("run.log");
        std::fs::write(&log, "$ codex\n{\"type\":\"thread.started\",\"thread_id\":\"t-42\"}\n{\"type\":\"turn.started\"}\n").unwrap();
        assert_eq!(codex_thread(&log).as_deref(), Some("t-42"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn streamed_logging_errors_do_not_change_worker_exit_code() {
        use std::os::unix::fs::symlink;

        for name in ["stream.jsonl", "events.jsonl", "run.log"] {
            let dir =
                std::env::temp_dir().join(format!("delegate-log-error-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            symlink("/dev/full", dir.join(name)).unwrap();
            let mut stream = events::Stream::new(&dir, Harness::Pi, &dir);
            let cmd = ["sh", "-c", "echo plain; echo '{\"type\":\"tool_execution_start\",\"toolCallId\":\"a\",\"toolName\":\"read\",\"args\":{\"path\":\"file\"}}'; exit 7"].map(str::to_owned);
            assert_eq!(
                run_streamed(&cmd, &dir, &dir.join("run.log"), &mut stream).unwrap(),
                7
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn streamed_worker_keeps_exit_code_and_drains_late_lines_during_grace() {
        let dir = std::env::temp_dir().join(format!("delegate-grace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut stream = events::Stream::new(&dir, Harness::Pi, &dir);
        let cmd = ["sh", "-c", "(sleep 0.2; echo final) & echo done; exit 7"].map(str::to_owned);
        assert_eq!(
            run_streamed(&cmd, &dir, &dir.join("run.log"), &mut stream).unwrap(),
            7
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("stream.jsonl")).unwrap(),
            "done\nfinal\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A worker that leaves a background process holding stdout must not block the run
    /// after the worker itself exits (the old `run_logged` returned at child exit).
    #[test]
    fn run_streamed_returns_when_worker_exits_even_if_a_descendant_keeps_stdout_open() {
        let dir = std::env::temp_dir().join(format!("delegate-rev-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut stream = events::Stream::new(&dir, Harness::Pi, &dir);
        let cmd: Vec<String> = ["sh", "-c", "sleep 20 & echo done"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let started = Instant::now();
        let rc = run_streamed(&cmd, &dir, &dir.join("run.log"), &mut stream).unwrap();
        assert_eq!(rc, 0);
        assert!(
            started.elapsed().as_secs() < 5,
            "run_streamed blocked {}s until the background process closed stdout",
            started.elapsed().as_secs()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
