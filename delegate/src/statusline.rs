//! `delegate statusline`: one line for Claude Code's status line.
//!
//! Claude Code runs this with a JSON object on stdin and shows whatever it prints,
//! so every failure here is silent and the exit code is always success. Only the
//! phase and stalled flag are read (`Depth::Phase`): no diffs, steps or log tails.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use crate::status::{self, Depth, TaskView};
use crate::task::Ctx;

/// Print the line, if there is one. Never fails: a broken status line must not
/// break Claude Code's own.
pub fn run() {
    if let Some(line) = project().and_then(|ctx| render(&ctx)) {
        println!("{line}");
    }
}

/// The project the status line describes: Claude Code's directory, else ours.
fn project() -> Option<Ctx> {
    let mut candidates: Vec<PathBuf> = stdin_dir().into_iter().collect();
    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir);
    }
    candidates.into_iter().find_map(|dir| {
        // This process does nothing else, so it may resolve the project from
        // Claude Code's directory instead of its own.
        std::env::set_current_dir(dir).ok()?;
        Ctx::discover().ok()
    })
}

/// `workspace.current_dir` from Claude Code's stdin JSON, else its `cwd`.
fn stdin_dir() -> Option<PathBuf> {
    if std::io::stdin().is_terminal() {
        return None;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let value: serde_json::Value = serde_json::from_str(&input).ok()?;
    let dir = value
        .get("workspace")
        .and_then(|workspace| workspace.get("current_dir"))
        .or_else(|| value.get("cwd"))
        .and_then(serde_json::Value::as_str)?;
    Some(PathBuf::from(dir))
}

/// One line for every unfinished task, then how many would merge now.
fn render(ctx: &Ctx) -> Option<String> {
    let views = status::collect(ctx, None, Depth::Phase).ok()?;
    let mut parts: Vec<String> = views
        .iter()
        .filter(|view| view.phase != "done" && view.phase != "not_started")
        .map(label)
        .collect();
    let ready = views.iter().filter(|view| view.ready_to_merge).count();
    if ready > 0 {
        parts.push(format!("{ready} ready to merge"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn label(view: &TaskView) -> String {
    if view.stalled {
        format!("{} stalled", view.id)
    } else {
        format!("{} {}", view.id, view.phase)
    }
}
