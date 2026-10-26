//! Refreshing, read-only task views without persistent terminal state.
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::status::{self, Depth, TaskView};
use crate::task::Ctx;

pub fn run(ctx: &Ctx, id: Option<&str>, interval: f64) -> Result<()> {
    let interval = refresh_interval(interval)?;
    let stdout = io::stdout();
    let terminal = stdout.is_terminal();
    let mut output = stdout.lock();
    // Default signal termination is intentional: no alternate screen or hidden
    // cursor means there is no terminal state to restore on Ctrl-C or SIGTERM.
    loop {
        let views = status::collect(ctx, id, Depth::Full)?;
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        draw_frame(&mut output, terminal, &views, id.is_some(), seconds)
            .context("writing watch output; check the terminal or output destination")?;
        std::thread::sleep(interval);
    }
}

fn refresh_interval(seconds: f64) -> Result<Duration> {
    let interval = Duration::try_from_secs_f64(seconds)
        .context("invalid watch interval; use a finite positive number of seconds")?;
    if interval.is_zero() {
        bail!("invalid watch interval; use a finite positive number of seconds");
    }
    Ok(interval)
}

fn draw_frame(
    output: &mut impl Write,
    terminal: bool,
    views: &[TaskView],
    detail: bool,
    seconds: u64,
) -> io::Result<()> {
    if terminal {
        write!(output, "\x1b[H\x1b[2J")?;
    }
    writeln!(
        output,
        "delegate watch   refreshed {:02}:{:02}:{:02} UTC",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )?;
    let text = if detail && let Some(view) = views.first() {
        status::render_detail(view, status::use_color(terminal))
    } else {
        status::render_rows(views, status::use_color(terminal))
    };
    write!(output, "{text}")?;
    // Pipes must see each frame immediately, even when the process is killed.
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::ChangedFile;

    fn task_view() -> TaskView {
        TaskView {
            id: "t001".into(),
            phase: "interrupted".into(),
            outcome: None,
            worker: Some("pi:test".into()),
            tier: "standard".into(),
            pid: None,
            started_at: None,
            last_activity_at: None,
            stalled: false,
            changed_files: vec![ChangedFile {
                path: "shop/pricing.py".into(),
                added: 3,
                removed: 1,
            }],
            steps: vec![],
            steps_available: false,
            ready_to_merge: false,
            merge_blockers: vec![],
            next: Some("delegate run t001 or delegate discard t001".into()),
            log: "run.log".into(),
            log_tail: "last line".into(),
            elapsed_secs: 65,
            activity_age_secs: 5,
        }
    }

    #[test]
    fn intervals_must_be_positive_finite_and_representable() -> Result<()> {
        assert_eq!(refresh_interval(2.0)?, Duration::from_secs(2));
        assert_eq!(refresh_interval(0.05)?, Duration::from_millis(50));
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX, 1e-20] {
            assert!(refresh_interval(seconds).is_err(), "accepted {seconds}");
        }
        Ok(())
    }

    #[test]
    fn piped_frames_repeat_headers_without_terminal_codes() -> Result<()> {
        let mut output = Vec::new();
        draw_frame(&mut output, false, &[], false, 0)?;
        draw_frame(&mut output, false, &[], false, 86_399)?;
        assert_eq!(
            String::from_utf8(output)?,
            concat!(
                "delegate watch   refreshed 00:00:00 UTC\n",
                "no tasks; start one with: delegate new <id>\n",
                "delegate watch   refreshed 23:59:59 UTC\n",
                "no tasks; start one with: delegate new <id>\n"
            )
        );
        Ok(())
    }

    #[test]
    fn terminal_frames_only_clear_and_reposition() -> Result<()> {
        let mut output = Vec::new();
        for _ in 0..2 {
            draw_frame(&mut output, true, &[], false, 86_400)?;
        }
        let text = String::from_utf8(output)?;
        assert_eq!(text.matches("\x1b[H\x1b[2Jdelegate watch").count(), 2);
        assert_eq!(text.matches('\x1b').count(), 4);
        assert!(text.contains("00:00:00 UTC"));
        Ok(())
    }

    #[test]
    fn frames_use_status_rows_and_detail() -> Result<()> {
        let views = [task_view()];
        for detail in [false, true] {
            let mut output = Vec::new();
            draw_frame(&mut output, false, &views, detail, 0)?;
            let expected = if detail {
                status::render_detail(&views[0], false)
            } else {
                status::render_rows(&views, false)
            };
            assert_eq!(
                String::from_utf8(output)?,
                format!("delegate watch   refreshed 00:00:00 UTC\n{expected}")
            );
            assert!(expected.contains("shop/pricing.py"));
            assert!(expected.contains("delegate discard t001"));
        }
        Ok(())
    }

    #[test]
    fn file_names_in_rows_are_bounded() {
        let mut view = task_view();
        for index in 0..4 {
            view.changed_files.push(ChangedFile {
                path: format!("file{index}.rs"),
                added: 0,
                removed: 0,
            });
        }
        let text = status::render_rows(&[view], false);
        let row = text
            .lines()
            .find(|line| line.trim_start().starts_with("t001 "))
            .unwrap();
        assert!(row.contains("5  shop/pricing.py"));
        assert!(row.contains("more"));
        assert!(row.chars().count() <= 100);
    }
}
