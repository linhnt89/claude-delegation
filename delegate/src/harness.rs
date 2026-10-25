//! Building and running worker commands, inside the bwrap sandbox.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::{Harness, Worker, home};

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

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(program).is_file()))
}

/// bwrap prefix for a worker (or for acceptance checks when `harness` is None).
/// Codex keeps its own sandbox; `DELEGATE_SANDBOX=0` disables sandboxing.
pub fn sandbox(harness: Option<Harness>, bounds: &Bounds, chdir: &Path) -> Result<Vec<String>> {
    if harness == Some(Harness::Codex) || std::env::var("DELEGATE_SANDBOX").is_ok_and(|v| v == "0")
    {
        return Ok(vec![]);
    }
    if !on_path("bwrap") {
        bail!("bwrap not found; install bubblewrap or set DELEGATE_SANDBOX=0");
    }
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

/// The latest Codex thread id in a log of `codex exec --json` output.
pub fn codex_thread(log: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log).ok()?;
    text.lines().rev().find_map(|line| {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
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
    fn reads_codex_thread() {
        let dir = std::env::temp_dir().join(format!("delegate-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("run.log");
        std::fs::write(&log, "$ codex\n{\"type\":\"thread.started\",\"thread_id\":\"t-42\"}\n{\"type\":\"turn.started\"}\n").unwrap();
        assert_eq!(codex_thread(&log).as_deref(), Some("t-42"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
