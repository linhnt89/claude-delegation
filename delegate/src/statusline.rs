//! `delegate statusline`: one line for Claude Code's status line.
//!
//! Claude Code runs this with a JSON object on stdin and shows whatever it prints,
//! so every failure here is silent and the exit code is always success. Only the
//! phase and stalled flag are read (`Depth::Phase`): no diffs, steps or log tails.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use crate::status::{self, Depth, TaskView};
use crate::task::Ctx;

/// Print the line, if there is one. Never fails: a broken status line must not
/// break Claude Code's own.
pub fn run() {
    let input = stdin_input();
    if let Some(ctx) = project(input.dir) {
        if let Some(line) = render(&ctx, input.session.as_deref()) {
            println!("{line}");
        }
    } else {
        let _ = crate::checkout::cleanup();
    }
}

/// The project the status line describes: Claude Code's directory, else ours.
fn project(dir: Option<PathBuf>) -> Option<Ctx> {
    let mut candidates: Vec<PathBuf> = dir.into_iter().collect();
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

#[derive(Default)]
struct Input {
    dir: Option<PathBuf>,
    session: Option<String>,
}

fn stdin_input() -> Input {
    if std::io::stdin().is_terminal() {
        return Input::default();
    }
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return Input::default();
    }
    parse_input(&input)
}

/// Read the directory and viewer independently: either may be absent.
fn parse_input(input: &str) -> Input {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return Input::default();
    };
    let dir = value
        .get("workspace")
        .and_then(|workspace| workspace.get("current_dir"))
        .or_else(|| value.get("cwd"))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from);
    let session = value
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .filter(|session| !session.is_empty())
        .map(str::to_owned);
    Input { dir, session }
}

fn render(ctx: &Ctx, session: Option<&str>) -> Option<String> {
    let views = status::collect(ctx, None, Depth::Phase).ok()?;
    render_views(&views, session)
}

/// Own unfinished tasks and ready count, followed by dimmed counts for other sessions.
///
/// Readiness depends on this checkout, so other sessions' finished tasks count by
/// their passed checks instead: they may be ready in their own checkout.
fn render_views(views: &[TaskView], session: Option<&str>) -> Option<String> {
    let session = session.filter(|session| !session.is_empty());
    let mut parts = Vec::new();
    let mut ready = 0;
    let mut others = BTreeMap::new();
    for view in views {
        let own = session.is_none() || view.session.as_deref() == session;
        match view.phase.as_str() {
            "not_started" => continue,
            "done" if own && !view.ready_to_merge => continue,
            "done" if !own && view.outcome.as_deref() != Some("pass") => continue,
            _ => {}
        }
        if own {
            if view.phase == "done" {
                ready += 1;
            } else {
                parts.push(label(view));
            }
        } else {
            let name = if view.session.is_none() {
                "by hand"
            } else {
                view.session_name.as_str()
            };
            let group = others
                .entry(view.session.as_deref())
                .or_insert((name, 0, 0));
            group.1 += 1;
            group.2 += usize::from(view.stalled);
        }
    }
    if ready > 0 {
        parts.push(format!("{ready} ready to merge"));
    }
    let own = parts.join(" · ");
    let mut groups: Vec<_> = others.into_iter().collect();
    groups.sort_by_key(|(id, (name, _, _))| (id.is_none(), *name, *id));
    let other = groups
        .into_iter()
        .map(|(_, (name, count, stalled))| {
            if stalled > 0 {
                format!("{name} {count} ({stalled} stalled)")
            } else {
                format!("{name} {count}")
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    match (own.is_empty(), other.is_empty()) {
        (true, true) => None,
        (false, true) => Some(own),
        (true, false) => Some(format!("\x1b[2m│ {other}\x1b[0m")),
        (false, false) => Some(format!("{own} \x1b[2m│ {other}\x1b[0m")),
    }
}

fn label(view: &TaskView) -> String {
    if view.stalled {
        format!("{} stalled", view.id)
    } else {
        format!("{} {}", view.id, view.phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: &str, phase: &str, session: Option<&str>, name: &str) -> TaskView {
        TaskView {
            id: id.into(),
            title: String::new(),
            goal: String::new(),
            session: session.map(str::to_owned),
            session_name: name.into(),
            phase: phase.into(),
            outcome: (phase == "done").then(|| "pass".into()),
            worker: None,
            tier: String::new(),
            pid: None,
            started_at: None,
            last_activity_at: None,
            stalled: false,
            changed_files: vec![],
            steps: vec![],
            steps_available: false,
            ready_to_merge: phase == "done",
            merge_blockers: vec![],
            next: None,
            log: String::new(),
            log_tail: String::new(),
            elapsed_secs: 0,
            activity_age_secs: 0,
        }
    }

    fn scene() -> Vec<TaskView> {
        let mut stalled = view("t032", "implement", Some("b"), "pane-polish");
        stalled.stalled = true;
        vec![
            view("t021", "implement", Some("a"), "shop"),
            view("t022", "done", Some("a"), "shop"),
            view("t030", "implement", Some("b"), "pane-polish"),
            view("t031", "implement", None, "by hand"),
            stalled,
        ]
    }

    #[test]
    fn no_session_shows_every_task_as_own() {
        let expected = Some(
            "t021 implement · t030 implement · t031 implement · t032 stalled · 1 ready to merge"
                .into(),
        );
        assert_eq!(render_views(&scene(), None), expected);
        assert_eq!(render_views(&scene(), Some("")), expected);
    }

    #[test]
    fn own_only_has_no_dimmed_part() {
        let views = [
            view("t021", "implement", Some("a"), "shop"),
            view("t022", "done", Some("a"), "shop"),
        ];
        assert_eq!(
            render_views(&views, Some("a")),
            Some("t021 implement · 1 ready to merge".into())
        );
    }

    #[test]
    fn other_sessions_are_dimmed_counts_with_stalls_and_by_hand_last() {
        assert_eq!(
            render_views(&scene(), Some("a")),
            Some("t021 implement · 1 ready to merge \x1b[2m│ pane-polish 2 (1 stalled) · by hand 1\x1b[0m".into())
        );
        assert_eq!(
            render_views(&scene(), Some("b")),
            Some("t030 implement · t032 stalled \x1b[2m│ shop 2 · by hand 1\x1b[0m".into())
        );
    }

    #[test]
    fn others_only_are_sorted_by_name_without_leading_space() {
        let views = [
            view("t001", "implement", None, "ignored"),
            view("t002", "done", Some("b"), "zebra"),
            view("t003", "implement", Some("c"), "alpha"),
            view("t004", "implement", Some("b"), "zebra"),
        ];
        assert_eq!(
            render_views(&views, Some("a")),
            Some("\x1b[2m│ alpha 1 · zebra 2 · by hand 1\x1b[0m".into())
        );
    }

    #[test]
    fn same_named_sessions_remain_separate_groups() {
        let views = [
            view("t001", "implement", Some("b"), "shop"),
            view("t002", "implement", Some("c"), "shop"),
        ];
        assert_eq!(
            render_views(&views, Some("a")),
            Some("\x1b[2m│ shop 1 · shop 1\x1b[0m".into())
        );
    }

    #[test]
    fn interrupted_ready_task_is_counted_once_by_label_or_group() {
        let mut interrupted = view("t001", "interrupted", Some("a"), "shop");
        interrupted.ready_to_merge = true;
        let views = [interrupted];
        for session in [None, Some("a")] {
            assert_eq!(
                render_views(&views, session),
                Some("t001 interrupted".into())
            );
        }
        assert_eq!(
            render_views(&views, Some("b")),
            Some("\x1b[2m│ shop 1\x1b[0m".into())
        );
    }

    #[test]
    fn inactive_tasks_are_not_counted_anywhere() {
        let mut done = view("t001", "done", Some("a"), "shop");
        done.ready_to_merge = false;
        done.outcome = Some("fail".into());
        let mut not_started = view("t002", "not_started", Some("b"), "pane-polish");
        not_started.ready_to_merge = true;
        let views = [done, not_started];
        for session in [None, Some("a"), Some("b"), Some("c")] {
            assert_eq!(render_views(&views, session), None);
            assert_eq!(render_views(&[], session), None);
        }
    }

    #[test]
    fn other_sessions_count_passed_tasks_not_ready_in_this_checkout() {
        let mut passed = view("t001", "done", Some("b"), "shop");
        passed.ready_to_merge = false;
        let mut failed = view("t002", "done", Some("b"), "shop");
        failed.outcome = Some("fail".into());
        failed.ready_to_merge = true;
        let mut own = view("t003", "done", Some("a"), "home");
        own.ready_to_merge = false;
        let views = [passed, failed, own];
        assert_eq!(
            render_views(&views, Some("a")),
            Some("\x1b[2m│ shop 1\x1b[0m".into())
        );
        assert_eq!(render_views(&views, None), Some("1 ready to merge".into()));
    }

    #[test]
    fn multiple_stalled_tasks_are_counted_within_their_group() {
        let mut views = scene();
        views[2].stalled = true;
        assert_eq!(
            render_views(&views, Some("a")),
            Some("t021 implement · 1 ready to merge \x1b[2m│ pane-polish 2 (2 stalled) · by hand 1\x1b[0m".into())
        );
    }

    #[test]
    fn stdin_parses_session_and_prefers_workspace_directory() {
        let input = parse_input(
            r#"{"session_id":"a","cwd":"/fallback","workspace":{"current_dir":"/project"}}"#,
        );
        assert_eq!(input.dir, Some(PathBuf::from("/project")));
        assert_eq!(input.session.as_deref(), Some("a"));
        let input = parse_input(r#"{"session_id":"b","cwd":"/fallback"}"#);
        assert_eq!(input.dir, Some(PathBuf::from("/fallback")));
        assert_eq!(input.session.as_deref(), Some("b"));
    }

    #[test]
    fn stdin_fields_are_optional_and_invalid_input_is_silent() {
        for json in ["", "not json", "null", "{}", r#"{"session_id":42}"#] {
            let input = parse_input(json);
            assert_eq!(input.dir, None);
            assert_eq!(input.session, None);
        }
        for json in [
            r#"{"cwd":"/project"}"#,
            r#"{"session_id":"","cwd":"/project"}"#,
        ] {
            let input = parse_input(json);
            assert_eq!(input.dir, Some(PathBuf::from("/project")));
            assert_eq!(input.session, None);
        }
        let input = parse_input(r#"{"session_id":"a"}"#);
        assert_eq!(input.dir, None);
        assert_eq!(input.session.as_deref(), Some("a"));
    }
}
