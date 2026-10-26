//! Blocking, read-only wait for task events: finished, interrupted or stalled.
//!
//! Polls `status::collect` and prints the first event for each watched task. Like
//! every other view it never writes task state.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::status::{self, Depth, TaskView};
use crate::task::Ctx;

/// Phases that mean the runner is still doing something.
fn is_live(phase: &str) -> bool {
    matches!(phase, "questions" | "implement" | "checks" | "review")
}

pub fn run(ctx: &Ctx, ids: &[String], timeout: Option<&str>) -> Result<ExitCode> {
    let timeout = timeout.map(parse_duration).transpose()?;
    let watched = watched_ids(ctx, ids)?;
    if watched.is_empty() {
        println!("nothing is running; start one with: delegate run --background <id>");
        return Ok(ExitCode::SUCCESS);
    }
    let deadline = match timeout {
        Some(limit) => Some(
            Instant::now()
                .checked_add(limit)
                .context("timeout is too large; use a smaller duration")?,
        ),
        None => None,
    };
    let interval = Duration::from_secs(1);
    loop {
        let views = status::collect(ctx, None, Depth::Phase)?;
        let mut lines = Vec::new();
        for id in &watched {
            if let Some(view) = views.iter().find(|view| &view.id == id)
                && let Some(line) = event_line(ctx, view)
            {
                lines.push(line);
            }
        }
        if !lines.is_empty() {
            for line in lines {
                println!("{line}");
            }
            return Ok(ExitCode::SUCCESS);
        }
        if !watched
            .iter()
            .any(|id| views.iter().any(|view| &view.id == id))
        {
            println!("nothing is running; start one with: delegate run --background <id>");
            return Ok(ExitCode::SUCCESS);
        }
        let now = Instant::now();
        let Some(deadline) = deadline else {
            std::thread::sleep(interval);
            continue;
        };
        if now >= deadline {
            println!(
                "nothing happened yet; phases: {}",
                phase_summary(&views, &watched)
            );
            return Ok(ExitCode::from(3));
        }
        std::thread::sleep(interval.min(deadline.saturating_duration_since(now)));
    }
}

fn watched_ids(ctx: &Ctx, requested: &[String]) -> Result<Vec<String>> {
    let views = status::collect(ctx, None, Depth::Phase)?;
    if requested.is_empty() {
        return Ok(views
            .into_iter()
            .filter(|view| is_live(&view.phase))
            .map(|view| view.id)
            .collect());
    }
    let known: Vec<&str> = views.iter().map(|view| view.id.as_str()).collect();
    let mut watched = Vec::new();
    for id in requested {
        if !known.contains(&id.as_str()) {
            bail!(
                "unknown task '{id}'; known tasks: {}",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            );
        }
        if !watched.contains(id) {
            watched.push(id.clone());
        }
    }
    Ok(watched)
}

fn event_line(ctx: &Ctx, view: &TaskView) -> Option<String> {
    if view.phase == "done" {
        let outcome = view.outcome.as_deref().unwrap_or("unknown");
        let next = view
            .next
            .clone()
            .unwrap_or_else(|| format!("delegate status {}", view.id));
        return Some(format!("{} done, {outcome}; next: {next}", view.id));
    }
    if view.phase == "interrupted" {
        return Some(format!(
            "{} interrupted; next: delegate run {} or delegate discard {}",
            view.id, view.id, view.id
        ));
    }
    if view.stalled {
        return Some(stalled_line(ctx, view));
    }
    None
}

fn stalled_line(ctx: &Ctx, view: &TaskView) -> String {
    let last = last_step(ctx, view).unwrap_or_else(|| "none".to_string());
    let next = view
        .next
        .clone()
        .unwrap_or_else(|| format!("delegate status {}", view.id));
    format!(
        "{} stalled in {} for {}; last step: {last}; next: {next}",
        view.id,
        view.phase,
        duration(view.activity_age_secs)
    )
}

fn last_step(ctx: &Ctx, view: &TaskView) -> Option<String> {
    let steps = if view.steps.is_empty() {
        // A phase-only snapshot may leave out steps; ask for the full one then.
        status::collect(ctx, Some(&view.id), Depth::Full)
            .ok()
            .and_then(|mut views| views.pop())
            .map_or_else(Vec::new, |view| view.steps)
    } else {
        view.steps.clone()
    };
    steps.last().map(describe_step)
}

fn describe_step(step: &serde_json::Value) -> String {
    let field = |name: &str| step[name].as_str().unwrap_or("").replace(['\n', '\r'], " ");
    let text = [field("kind"), field("target"), field("result")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        "none".to_string()
    } else {
        text
    }
}

fn phase_summary(views: &[TaskView], watched: &[String]) -> String {
    let parts: Vec<_> = watched
        .iter()
        .filter_map(|id| {
            views
                .iter()
                .find(|view| &view.id == id)
                .map(|view| format!("{} {}", view.id, view.phase))
        })
        .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn duration(seconds: u64) -> String {
    if seconds >= 60 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn parse_duration(text: &str) -> Result<Duration> {
    let text = text.trim();
    let (digits, seconds_per_unit) = match text.strip_suffix('s') {
        Some(digits) => (digits, 1),
        None => match text.strip_suffix('m') {
            Some(digits) => (digits, 60),
            None => (text, 1),
        },
    };
    let value: u64 = digits.parse().with_context(|| {
        format!("invalid timeout '{text}'; use seconds or a duration like 90s or 25m")
    })?;
    let seconds = value
        .checked_mul(seconds_per_unit)
        .context("timeout is too large; use a smaller duration")?;
    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_accept_seconds_and_suffixed_durations() -> Result<()> {
        assert_eq!(parse_duration("5")?, Duration::from_secs(5));
        assert_eq!(parse_duration("90s")?, Duration::from_secs(90));
        assert_eq!(parse_duration("25m")?, Duration::from_mins(25));
        assert_eq!(parse_duration(" 30s ")?.as_secs(), 30);
        for bad in ["", "s", "m", "1.5m", "ten", "-5", "5h"] {
            assert!(parse_duration(bad).is_err(), "accepted {bad}");
        }
        Ok(())
    }

    #[test]
    fn durations_are_short_and_readable() {
        assert_eq!(duration(6), "6s");
        assert_eq!(duration(65), "1m5s");
    }

    #[test]
    fn stalled_event_preserves_details_and_offers_the_live_hint() {
        let ctx = Ctx {
            root: "unused".into(),
            dhome: "unused".into(),
            gitdir: "unused".into(),
        };
        let view = TaskView {
            id: "t001".into(),
            phase: "implement".into(),
            outcome: None,
            worker: None,
            tier: "custom".into(),
            pid: Some(12345),
            started_at: None,
            last_activity_at: None,
            stalled: true,
            changed_files: vec![],
            steps: vec![serde_json::json!({
                "kind": "edit", "target": "shop/pricing.py", "result": "ok",
            })],
            steps_available: true,
            ready_to_merge: false,
            merge_blockers: vec![],
            next: Some("delegate status t001  ·  kill 12345".into()),
            log: String::new(),
            log_tail: String::new(),
            elapsed_secs: 500,
            activity_age_secs: 425,
        };
        assert_eq!(
            event_line(&ctx, &view).as_deref(),
            Some(
                "t001 stalled in implement for 7m5s; last step: edit shop/pricing.py ok; next: delegate status t001  ·  kill 12345"
            )
        );
    }

    #[test]
    fn steps_are_described_by_kind_target_and_result() {
        let step = serde_json::json!({
            "kind": "run",
            "target": "cargo test",
            "result": "line\nline",
        });
        assert_eq!(describe_step(&step), "run cargo test line line");
        assert_eq!(describe_step(&serde_json::json!({})), "none");
    }
}
