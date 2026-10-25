//! The `delegate` subcommands.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::config::{Harness, ProjectRules, ReviewRule, Role, Tier, Worker, WorkersConfig};
use crate::harness::{self, Bounds, Session};
use crate::prompts;
use crate::spec;
use crate::status::{self, Operation};
use crate::task::{CheckOutcome, Checks, Ctx, ReviewState, Round, RoundStatus, Task, git};

pub const SPEC_TEMPLATE: &str = include_str!("../templates/SPEC.md");
pub const PRUNE_TEMPLATE: &str = include_str!("../templates/PRUNE.md");
const REPORT_TEMPLATE: &str = include_str!("../templates/REPORT.md");
const CHECK_TIMEOUT: Duration = Duration::from_mins(15);

fn task_bounds(ctx: &Ctx, t: &Task) -> Result<Bounds> {
    let admin = git(&t.wt, &["rev-parse", "--absolute-git-dir"])?;
    Ok(Bounds {
        hide: vec![ctx.root.clone(), ctx.dhome.clone()],
        ro: vec![ctx.gitdir.clone()],
        rw: vec![admin.into(), t.wt.clone()],
    })
}

/// First non-empty line of a file, if it exists.
fn first_line(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
}

// ---------------------------------------------------------------- new

pub fn new(ctx: &Ctx, id: &str, prune: bool) -> Result<()> {
    let t = Task::load(ctx, id)?;
    if t.spec.exists() {
        bail!("{} already exists", t.spec.display());
    }
    fs::create_dir_all(&t.dir)?;
    let template = if prune { PRUNE_TEMPLATE } else { SPEC_TEMPLATE };
    fs::write(&t.spec, template.replace("<id>", &t.id))?;
    println!("{}", t.spec.display());
    Ok(())
}

// ---------------------------------------------------------------- run

/// Whether the worker first answers READY or asks blocking questions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionPhase {
    /// Only until a round has been implemented.
    Auto,
    Always,
    Never,
}

#[derive(Debug)]
pub struct RunArgs {
    pub id: String,
    pub tier: Option<Tier>,
    pub worker: Option<String>,
    pub questions: QuestionPhase,
    pub dry_run: bool,
    /// Read-only research: research tiers, no question phase, no checks; the report is the result.
    pub research: bool,
    pub background: bool,
}

fn pick_tier(
    t: &Task,
    role: Role,
    given: Option<Tier>,
    cfg: &WorkersConfig,
    rules: &ProjectRules,
    spec_text: &str,
) -> Result<(Tier, String)> {
    if let Some(tier) = given {
        return Ok((tier, "given".into()));
    }
    let last_tier = t.last_round().and_then(|r| r.tier);
    let (mut tier, mut why) = match (t.last_round(), last_tier) {
        (Some(r), Some(prev))
            if r.status == RoundStatus::Done
                && r.checks
                    .as_ref()
                    .is_some_and(|c| c.outcome == CheckOutcome::Fail) =>
        {
            (
                prev.next(),
                format!("stepped up after failed checks at {prev}"),
            )
        }
        (Some(_), Some(prev)) => (prev, "same as previous round".into()),
        _ => (cfg.default_tier(role), "default".into()),
    };
    if role == Role::Research {
        return Ok((tier, why));
    }
    for rule in rules.matching(&spec::scope_paths(spec_text))? {
        if let Some(min) = rule.min_tier.filter(|m| tier < *m) {
            tier = min;
            why = format!("raised by .delegate.toml rule for {}", rule.label());
        }
    }
    Ok((tier, why))
}

enum Attempt {
    NotStarted,
    Questions,
    Worked(i32),
}

fn ensure_task_excluded(ctx: &Ctx) -> Result<()> {
    let exclude = ctx.gitdir.join("info/exclude");
    fs::create_dir_all(exclude.parent().context("bad git dir")?)?;
    let current = fs::read_to_string(&exclude).unwrap_or_default();
    if !current.lines().any(|l| l == ".task/") {
        let sep = if current.is_empty() || current.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        fs::write(&exclude, format!("{current}{sep}.task/\n"))?;
    }
    Ok(())
}

fn implement_attempt(
    ctx: &Ctx,
    t: &Task,
    w: &Worker,
    ask: bool,
    research: bool,
    log: &Path,
    operation: &Operation,
) -> Result<Attempt> {
    let task_dir = t.wt.join(".task");
    let mut stream = harness::events::Stream::new(&t.dir, w.harness, &t.wt);
    let sb = harness::sandbox(Some(w.harness), &task_bounds(ctx, t)?, &t.wt)?;
    let mut run = |session: Option<&Session>, prompt: &str| -> Result<i32> {
        let cmd = [sb.clone(), harness::streaming_command(w, prompt, session)].concat();
        harness::run_streamed(&cmd, &t.wt, log, &mut stream)
    };
    let mut session = harness::new_session(w.harness);

    if !ask {
        let prompt = if research {
            prompts::RESEARCH.replace("{id}", &t.id)
        } else {
            prompts::full(&t.id)
        };
        let rc = run(session.as_ref(), &prompt)?;
        let started = rc == 0 || task_dir.join("REPORT.md").exists() || t.worktree_dirty()?;
        return Ok(if started {
            Attempt::Worked(rc)
        } else {
            Attempt::NotStarted
        });
    }

    let qfile = task_dir.join("QUESTIONS.md");
    let rc = run(session.as_ref(), &prompts::ASK.replace("{id}", &t.id))?;
    if rc != 0 && !qfile.exists() {
        return Ok(Attempt::NotStarted);
    }
    let answer = first_line(&qfile).unwrap_or_default().to_uppercase();
    if answer.starts_with("QUESTIONS") {
        return Ok(Attempt::Questions);
    }
    if !answer.starts_with("READY") {
        println!("  no READY/QUESTIONS answer; continuing to the work phase");
    }
    if w.harness == Harness::Codex {
        session = harness::codex_thread(log).map(|id| Session { id, resume: false });
    }
    let resumed = session
        .filter(|_| harness::can_resume(w.harness))
        .map(|s| Session { resume: true, ..s });
    operation.phase("implement")?;
    let rc = match &resumed {
        Some(s) => run(Some(s), prompts::WORK)?,
        None => run(None, &prompts::full(&t.id))?,
    };
    Ok(Attempt::Worked(rc))
}

/// Paths with uncommitted work in the worktree, tracked or untracked (ignored files excluded).
fn worktree_changed_paths(t: &Task) -> Result<Vec<String>> {
    let mut paths: Vec<String> = git(&t.wt, &["diff", "--name-only", "HEAD"])?
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    paths.extend(
        git(&t.wt, &["ls-files", "--others", "--exclude-standard"])?
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string),
    );
    Ok(paths)
}

fn asks_questions(t: &Task, a: &RunArgs) -> bool {
    !a.research
        && match a.questions {
            QuestionPhase::Always => true,
            QuestionPhase::Never => false,
            QuestionPhase::Auto => !t.state.rounds.iter().any(|r| r.status == RoundStatus::Done),
        }
}

pub fn run(ctx: &Ctx, a: &RunArgs) -> Result<()> {
    let mut t = Task::load(ctx, &a.id)?;
    status::ensure_idle(&t)?;
    let mut operation = None;
    // The parent already preflighted this detached run. Publish the child's own
    // identity before repeating even the spec and dirty-worktree checks.
    if !a.dry_run && std::env::var("_DELEGATE_BACKGROUND_RUN").as_deref() == Ok(a.id.as_str()) {
        let mut child_operation = Operation::acquire(&t, "run")?;
        t = Task::load(ctx, &a.id)?;
        child_operation.start(
            if asks_questions(&t, a) {
                "questions"
            } else {
                "implement"
            },
            a.worker.as_deref(),
            &a.tier
                .map_or_else(|| "custom".into(), |tier| tier.to_string()),
            "0/0",
        )?;
        operation = Some(child_operation);
    }
    if !a.dry_run && t.dir.is_dir() {
        if operation.is_none() {
            operation = Some(Operation::acquire(&t, "run")?);
        }
        harness::events::reset(&t.dir);
    }
    if !a.dry_run && a.research && t.wt.is_dir() && t.worktree_dirty()? {
        bail!(
            "{}: the worktree has uncommitted changes; commit or discard them before a research run",
            t.id
        );
    }
    let spec_text = t.spec_text()?;
    // State may have changed between the first snapshot and obtaining the lock.
    t = Task::load(ctx, &a.id)?;
    let ask = asks_questions(&t, a);
    let phase = if ask { "questions" } else { "implement" };
    let cfg = WorkersConfig::load()?;
    let rules = ProjectRules::load(&ctx.root)?;
    let role = if a.research {
        Role::Research
    } else {
        Role::Implement
    };
    let (tier, why, workers) = if let Some(w) = &a.worker {
        (
            a.tier,
            "explicit worker".to_string(),
            vec![w.parse::<Worker>()?],
        )
    } else {
        let (tier, why) = pick_tier(&t, role, a.tier, &cfg, &rules, &spec_text)?;
        (Some(tier), why, cfg.candidates(role, tier, None)?)
    };
    let tier_label = tier.map_or_else(|| "custom".into(), |x| x.to_string());
    println!(
        "task {}: {} @ {tier_label} ({why}), question phase: {}",
        t.id,
        role.name(),
        if ask { "yes" } else { "no" }
    );
    if a.dry_run {
        for w in &workers {
            let iso = if w.harness == Harness::Codex {
                "own sandbox"
            } else {
                "bwrap"
            };
            println!("  candidate: {w}  [{iso}]");
        }
        return Ok(());
    }

    if harness::needs_bwrap(
        workers.iter().map(|w| w.harness),
        a.research,
        std::env::var("DELEGATE_SANDBOX").is_ok_and(|v| v == "0"),
    ) {
        harness::require_bwrap()?;
    }
    let mut available = Vec::new();
    let mut missing = Vec::new();
    let mut failures = String::new();
    for w in workers {
        if harness::on_path(w.harness.name()) {
            available.push(w);
        } else {
            let reason = format!("{} is not installed (not on PATH)", w.harness.name());
            println!("  {w} can't start: {reason}");
            writeln!(failures, "  {w}: {reason}")?;
            missing.push(reason);
        }
    }
    if available.is_empty() {
        let config_path = std::env::var_os("DELEGATE_WORKERS")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_file())
            .unwrap_or_else(crate::config::user_config_path);
        let fix = if a.worker.is_some() {
            "install it or pick another --worker".to_string()
        } else {
            format!(
                "install it or change [{}].{tier_label} in {}",
                role.name(),
                config_path.display()
            )
        };
        missing.dedup();
        bail!("no candidate can start: {}; {fix}", missing.join("; "));
    }

    if a.background {
        drop(operation);
        return background(a, &t);
    }
    let operation = operation.as_mut().context("missing run operation")?;
    operation.start(
        phase,
        available.first().map(ToString::to_string).as_deref(),
        &tier_label,
        &format!("1/{}", available.len()),
    )?;
    ensure_task_excluded(ctx)?;
    if !t.wt.is_dir() {
        let base = t.base(ctx)?;
        git(
            &ctx.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &t.branch,
                &t.wt.to_string_lossy(),
                &base,
            ],
        )?;
        t.state.base = Some(base);
    }
    let task_dir = t.wt.join(".task");
    fs::create_dir_all(&task_dir)?;
    fs::write(task_dir.join("SPEC.md"), &spec_text)?;
    fs::write(task_dir.join("REPORT.template.md"), REPORT_TEMPLATE)?;
    let checks_path = task_dir.join("CHECKS.txt");
    // The last finished round: rounds after it (failed to start, unfinished) left the code as is.
    if t.last_done().is_some_and(|r| {
        r.checks
            .as_ref()
            .is_some_and(|c| c.outcome == CheckOutcome::Fail)
    }) {
        fs::copy(t.dir.join("CHECKS.txt"), &checks_path).context(
            "copying failed checks into .task/CHECKS.txt; restore the task's CHECKS.txt and retry",
        )?;
    } else {
        match fs::remove_file(&checks_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .context("removing stale .task/CHECKS.txt; check file permissions and retry");
            }
        }
    }
    for f in ["REPORT.md", "QUESTIONS.md", "REVIEW.md", "DIFF.patch"] {
        let _ = fs::remove_file(task_dir.join(f));
    }
    let log = t.dir.join("run.log");
    let number = u32::try_from(t.state.rounds.len())? + 1;
    let round_index = t.state.rounds.len();
    t.state.rounds.push(Round {
        number,
        tier,
        worker: None,
        status: RoundStatus::Running,
        research: a.research,
        exit: None,
        checks: None,
    });

    let mut outcome = None;
    for (index, w) in available.iter().enumerate() {
        operation.update(
            phase,
            Some(&w.to_string()),
            &tier_label,
            &format!("{}/{}", index + 1, available.len()),
        )?;
        println!("round {number}: {w} ...");
        let log_start = match fs::metadata(&log) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        t.state.rounds[round_index].worker = Some(w.to_string());
        t.save()?;
        match implement_attempt(ctx, &t, w, ask, a.research, &log, operation)? {
            Attempt::NotStarted => {
                println!("  {w} failed to start, trying next candidate");
                writeln!(failures, "  {w}:")?;
                for line in harness::log_tail(&log, log_start)? {
                    println!("    {line}");
                    writeln!(failures, "    {line}")?;
                }
            }
            other => {
                outcome = Some((w.clone(), other));
                break;
            }
        }
    }
    let Some((w, attempt)) = outcome else {
        t.state.rounds[round_index].fail_to_start();
        t.save()?;
        bail!("no candidate could start:\n{failures}see {}", log.display());
    };

    if let Attempt::Questions = attempt {
        let questions = t.dir.join("QUESTIONS.md");
        fs::copy(task_dir.join("QUESTIONS.md"), &questions)?;
        t.state.rounds[round_index].status = RoundStatus::Questions;
        t.save()?;
        println!(
            "\n{w} has questions before starting ({}):\n",
            questions.display()
        );
        println!("{}", fs::read_to_string(&questions)?);
        println!("Answer them in the spec (e.g. a '## Answers' section), then run again.");
        return Ok(());
    }
    let rc = if let Attempt::Worked(rc) = attempt {
        rc
    } else {
        -1
    };

    t.state.rounds[round_index].exit = Some(rc);
    t.save()?;
    if rc != 0 {
        println!("warning: {w} exited with code {rc}; its work may be incomplete");
    }

    let report = t.dir.join("REPORT.md");
    if task_dir.join("REPORT.md").exists() {
        fs::copy(task_dir.join("REPORT.md"), &report)?;
    } else {
        fs::write(
            &report,
            format!(
                "# Report {}\n\n## Status\nblocked\n\nNo REPORT.md was written. Exit code {rc}; see run.log.\n",
                t.id
            ),
        )?;
    }
    if a.research {
        let changed = worktree_changed_paths(&t)?;
        if !changed.is_empty() {
            git(&t.wt, &["reset", "-q", "--hard", "HEAD"])?;
            git(&t.wt, &["clean", "-q", "-fd"])?;
        }
        t.state.rounds[round_index].status = RoundStatus::Done;
        t.save()?;
        println!("research report: {}", report.display());
        if !changed.is_empty() {
            println!(
                "warning: the researcher changed files; not committed, reverted: {}",
                changed.join(", ")
            );
        }
        return Ok(());
    }
    git(&t.wt, &["add", "-A"])?;
    if !git(&t.wt, &["diff", "--cached", "--name-only"])?.is_empty() {
        git(
            &t.wt,
            &[
                "commit",
                "-q",
                "-m",
                &format!("task {} round {number}", t.id),
            ],
        )?;
    }
    if t.changed_paths(ctx)?.is_empty() {
        println!("no changes: {w} changed nothing");
    } else {
        t.state.rounds[round_index].checks = Some(run_checks(ctx, &t, operation)?);
    }
    t.state.rounds[round_index].status = RoundStatus::Done;
    t.save()?;
    summary(ctx, &t)
}

/// Re-exec a preflighted run through the setsid program, with no terminal descriptors.
fn background(a: &RunArgs, t: &Task) -> Result<()> {
    use std::process::Stdio;
    if !harness::on_path("setsid") {
        bail!("setsid not found; install util-linux or run without --background");
    }
    let output = fs::File::create(t.dir.join("run.out"))?;
    let mut command = Command::new("setsid");
    command
        .arg(std::env::current_exe()?)
        .arg("run")
        .arg(&a.id)
        .env("_DELEGATE_BACKGROUND_RUN", &a.id);
    if let Some(tier) = a.tier {
        command.arg("--tier").arg(tier.to_string());
    }
    if let Some(worker) = &a.worker {
        command.arg("--worker").arg(worker);
    }
    match a.questions {
        QuestionPhase::Always => {
            command.arg("--ask");
        }
        QuestionPhase::Never => {
            command.arg("--no-ask");
        }
        QuestionPhase::Auto => {}
    }
    if a.research {
        command.arg("--research");
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .spawn()
        .context("starting detached runner with setsid")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(live) = status::read_live(t)?
            && live.pid == child.id()
        {
            // Even a very short run can finish before the parent is scheduled again.
            println!(
                "{} is running in the background; follow it with: delegate status {}",
                t.id, t.id
            );
            return Ok(());
        }
        if let Some(exit) = child.try_wait()? {
            let output = fs::read_to_string(t.dir.join("run.out")).unwrap_or_default();
            if exit.success() {
                println!(
                    "{} is running in the background; follow it with: delegate status {}",
                    t.id, t.id
                );
                return Ok(());
            }
            bail!("background run failed: {}", output.trim());
        }
        if std::time::Instant::now() >= deadline {
            // Do not leave a runner behind when startup could not be confirmed.
            child.kill()?;
            child.wait()?;
            bail!(
                "background runner did not start; inspect {}; a retry may work",
                t.dir.join("run.out").display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------- checks

fn run_with_timeout(cmd: &[String], cwd: &Path) -> Result<(i32, String)> {
    let (program, args) = cmd.split_first().context("empty command")?;
    let out_path =
        std::env::temp_dir().join(format!("delegate-check-{}.log", uuid::Uuid::new_v4()));
    let out = fs::File::create(&out_path)?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .with_context(|| format!("starting {program}"))?;
    let start = std::time::Instant::now();
    let rc = loop {
        if let Some(status) = child.try_wait()? {
            break status.code().unwrap_or(-1);
        }
        if start.elapsed() > CHECK_TIMEOUT {
            child.kill()?;
            child.wait()?;
            break 124;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let text = fs::read_to_string(&out_path).unwrap_or_default();
    let _ = fs::remove_file(&out_path);
    Ok((
        rc,
        if rc == 124 {
            format!("{text}\ntimed out after {}s", CHECK_TIMEOUT.as_secs())
        } else {
            text
        },
    ))
}

/// Run the spec's acceptance commands in the sandbox on the committed branch.
fn run_checks(ctx: &Ctx, t: &Task, operation: &Operation) -> Result<Checks> {
    operation.phase("checks")?;
    let cmds = spec::acceptance_commands(&t.spec_text()?);
    let mut lines = Vec::new();
    let mut all_ok = true;
    if cmds.is_empty() {
        lines.push(
            "no runnable acceptance commands in the spec (lines like: - [ ] `command`)".to_string(),
        );
    }
    let sb = harness::sandbox(None, &task_bounds(ctx, t)?, &t.wt)?;
    for c in &cmds {
        let full = [sb.clone(), vec!["bash".into(), "-c".into(), c.clone()]].concat();
        let (rc, out) = run_with_timeout(&full, &t.wt)?;
        all_ok &= rc == 0;
        lines.push(format!("{}  {c}", if rc == 0 { "PASS" } else { "FAIL" }));
        if rc != 0 {
            let tail: Vec<&str> = out.trim().lines().collect();
            lines.extend(
                tail[tail.len().saturating_sub(15)..]
                    .iter()
                    .map(|l| format!("      {l}")),
            );
        }
    }
    // Leave the worktree exactly at the committed branch.
    git(&t.wt, &["reset", "-q", "--hard", "HEAD"])?;
    git(&t.wt, &["clean", "-q", "-fd"])?;
    let outcome = match (cmds.is_empty(), all_ok) {
        (true, _) => CheckOutcome::NoCommands,
        (false, true) => CheckOutcome::Pass,
        (false, false) => CheckOutcome::Fail,
    };
    fs::write(t.dir.join("CHECKS.txt"), lines.join("\n") + "\n")?;
    let commit = t.head(ctx)?;
    println!("{}", lines.join("\n"));
    Ok(Checks { outcome, commit })
}

fn summary(ctx: &Ctx, t: &Task) -> Result<()> {
    let r = t.last_round().context("no rounds")?;
    let base = t.base(ctx)?;
    let diff = git(
        &ctx.root,
        &["diff", "--stat", &format!("{base}...{}", t.branch)],
    )?;
    let checks = r.checks.as_ref().map_or("not run", |c| c.outcome.label());
    let mut text = String::new();
    writeln!(
        text,
        "task:     {}  round {}  {}  {}",
        t.id,
        r.number,
        r.tier_label(),
        r.worker.as_deref().unwrap_or("-")
    )?;
    writeln!(
        text,
        "checks:   {checks} ({})",
        t.dir.join("CHECKS.txt").display()
    )?;
    writeln!(text, "branch:   {} (base {base})", t.branch)?;
    writeln!(text, "worktree: {}", t.wt.display())?;
    writeln!(text, "report:   {}", t.dir.join("REPORT.md").display())?;
    writeln!(text, "diff:")?;
    for l in diff.lines() {
        writeln!(text, "  {l}")?;
    }
    fs::write(t.dir.join("SUMMARY.txt"), &text)?;
    print!("{text}");
    Ok(())
}

pub fn check(ctx: &Ctx, id: &str) -> Result<()> {
    let mut t = Task::load(ctx, id)?;
    status::ensure_idle(&t)?;
    ensure_finished(&t)?;
    if !t.wt.is_dir() || t.last_done().is_none() {
        bail!("{id} has no implemented round");
    }
    let mut operation = Operation::acquire(&t, "checks")?;
    t = Task::load(ctx, id)?;
    operation.start(
        "checks",
        t.last_done().and_then(|r| r.worker.as_deref()),
        &t.last_done()
            .map_or_else(|| "custom".into(), Round::tier_label),
        "1/1",
    )?;
    ensure_finished(&t)?;
    if t.worktree_dirty()? {
        bail!(
            "commit your changes in {} first; checks run on the committed branch",
            t.wt.display()
        );
    }
    let checks = run_checks(ctx, &t, &operation)?;
    if let Some(r) = t.last_done_mut() {
        r.checks = Some(checks);
    }
    t.save()?;
    summary(ctx, &t)
}

// ---------------------------------------------------------------- review

fn ensure_finished(t: &Task) -> Result<()> {
    if let Some(r) = t.last_round()
        && r.status == RoundStatus::Running
    {
        bail!(
            "{}: round {} did not finish; run it again or discard it",
            t.id,
            r.number
        );
    }
    Ok(())
}

pub fn review(
    ctx: &Ctx,
    id: &str,
    tier: Option<Tier>,
    worker: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let mut t = Task::load(ctx, id)?;
    status::ensure_idle(&t)?;
    ensure_finished(&t)?;
    if !t.dir.is_dir() {
        bail!("{id} has no implemented round to review");
    }
    let mut operation = if dry_run {
        None
    } else {
        Some(Operation::acquire(&t, "review")?)
    };
    if let Some(operation) = operation.as_mut() {
        operation.start(
            "review",
            worker,
            &tier.map_or_else(|| "custom".into(), |t| t.to_string()),
            "0/0",
        )?;
    }
    t = Task::load(ctx, id)?;
    ensure_finished(&t)?;
    let implementer = t.last_done().and_then(|r| r.worker.clone());
    let Some(implementer) = implementer.filter(|_| t.wt.is_dir()) else {
        bail!("{id} has no implemented round to review");
    };
    if t.changed_paths(ctx)?.is_empty() {
        bail!("{id} has no changes to review");
    }
    let cfg = WorkersConfig::load()?;
    let tier = tier.unwrap_or(cfg.default_tier(Role::Review));
    let family = implementer.parse::<Worker>()?.family();
    let workers = match worker {
        Some(w) => vec![w.parse()?],
        None => cfg.candidates(Role::Review, tier, Some(&family))?,
    };
    println!("task {id}: review @ {tier}, implementer family {family} is skipped");
    if dry_run {
        for w in &workers {
            println!("  candidate: {w}");
        }
        return Ok(());
    }

    let operation = operation.as_mut().context("missing review operation")?;
    operation.update(
        "review",
        workers.first().map(ToString::to_string).as_deref(),
        &tier.to_string(),
        &format!("1/{}", workers.len()),
    )?;
    let task_dir = t.wt.join(".task");
    for f in ["REPORT.md", "QUESTIONS.md", "REVIEW.md"] {
        let _ = fs::remove_file(task_dir.join(f));
    }
    let base = t.base(ctx)?;
    fs::write(
        task_dir.join("DIFF.patch"),
        git(&ctx.root, &["diff", &format!("{base}...{}", t.branch)])? + "\n",
    )?;
    let log = t.dir.join("review.log");
    let prompt = prompts::REVIEW.replace("{id}", id);
    let review_file = task_dir.join("REVIEW.md");

    let mut used = None;
    for (index, w) in workers.iter().enumerate() {
        operation.update(
            "review",
            Some(&w.to_string()),
            &tier.to_string(),
            &format!("{}/{}", index + 1, workers.len()),
        )?;
        println!("review: {w} ...");
        let sb = harness::sandbox(Some(w.harness), &task_bounds(ctx, &t)?, &t.wt)?;
        let rc = harness::run_logged(
            &[sb, harness::command(w, &prompt, None, false, None)].concat(),
            &t.wt,
            &log,
            None,
        )?;
        if rc == 0 || review_file.exists() {
            used = Some(w.clone());
            break;
        }
        println!("  {w} failed to start, trying next candidate");
    }
    let Some(w) = used else {
        bail!("no reviewer could start; see {}", log.display())
    };

    let text = fs::read_to_string(&review_file)
        .unwrap_or_else(|_| "FINDINGS: ? high\nNo REVIEW.md was written; see review.log.\n".into());
    fs::write(t.dir.join("REVIEW.md"), &text)?;
    // Tests the reviewer added become a patch; the branch itself stays untouched.
    git(&t.wt, &["add", "-A"])?;
    let patch = git(&t.wt, &["diff", "--cached", "HEAD"])?;
    let patch_file = t.dir.join("REVIEW.patch");
    let _ = fs::remove_file(&patch_file);
    if !patch.is_empty() {
        fs::write(&patch_file, patch + "\n")?;
    }
    git(&t.wt, &["reset", "-q", "--hard", "HEAD"])?;
    git(&t.wt, &["clean", "-q", "-fd"])?;

    t.state.review = Some(ReviewState {
        worker: w.to_string(),
        tier: Some(tier),
        commit: t.head(ctx)?,
        high: prompts::high_findings(&text),
    });
    t.save()?;
    println!(
        "\nreview by {w}: {}",
        text.lines().next().unwrap_or("(empty)")
    );
    println!("  {}", t.dir.join("REVIEW.md").display());
    if patch_file.exists() {
        println!("  failing tests as a patch: {}", patch_file.display());
    }
    Ok(())
}

// ---------------------------------------------------------------- plan review

pub fn plan_review(
    ctx: &Ctx,
    ids: &[String],
    tier: Option<Tier>,
    worker: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let cfg = WorkersConfig::load()?;
    let tier = tier.unwrap_or(cfg.default_tier(Role::PlanReview));
    let planner = cfg.planner_family.clone();
    let workers = match worker {
        Some(w) => vec![w.parse()?],
        None => cfg.candidates(Role::PlanReview, tier, planner.as_deref())?,
    };
    let ids: Vec<String> = if ids.is_empty() {
        let mut found: Vec<String> = fs::read_dir(&ctx.dhome)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter(|e| e.path().join("SPEC.md").is_file())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.starts_with('_'))
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        found
    } else {
        ids.to_vec()
    };
    for id in &ids {
        if !ctx.dhome.join(id).join("SPEC.md").is_file() {
            bail!("no spec for {id}");
        }
    }
    println!(
        "plan review @ {tier} of PLAN.md + {} spec(s), skipping the {} family",
        ids.len(),
        planner.as_deref().unwrap_or("(none)")
    );
    if dry_run {
        for w in &workers {
            println!("  candidate: {w}");
        }
        return Ok(());
    }

    let work = ctx.dhome.join("_plan-review");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(work.join("specs"))?;
    for id in &ids {
        fs::copy(
            ctx.dhome.join(id).join("SPEC.md"),
            work.join("specs").join(format!("{id}.md")),
        )?;
    }
    let out = work.join("PLAN_REVIEW.md");
    let log = work.join("run.log");
    let prompt = prompts::PLAN_REVIEW.replace("{specs}", &work.join("specs").to_string_lossy());
    let bounds = Bounds {
        hide: vec![ctx.dhome.clone()],
        ro: vec![ctx.root.clone(), work.clone()],
        rw: vec![],
    };

    for w in &workers {
        println!("plan review: {w} ...");
        let sb = harness::sandbox(Some(w.harness), &bounds, &ctx.root)?;
        let codex = w.harness == Harness::Codex;
        let cmd = [
            sb,
            harness::command(w, &prompt, None, true, codex.then_some(out.as_path())),
        ]
        .concat();
        let rc = harness::run_logged(&cmd, &ctx.root, &log, (!codex).then_some(out.as_path()))?;
        if rc == 0 && first_line(&out).is_some() {
            println!(
                "\nplan review by {w}: {}\n  {}",
                first_line(&out).unwrap_or_default(),
                out.display()
            );
            return Ok(());
        }
        println!("  {w} failed to start, trying next candidate");
    }
    bail!("no plan reviewer could start; see {}", log.display())
}

// ---------------------------------------------------------------- list, merge, discard

fn failed_checks_problem(t: &Task) -> String {
    let checks = t.dir.join("CHECKS.txt");
    let text = fs::read_to_string(&checks).unwrap_or_default();
    let mut message = String::from("acceptance checks failed:");
    for line in text.lines().filter(|line| line.starts_with("FAIL  ")) {
        message.push_str("\n    ");
        message.push_str(line);
    }
    message.push_str("\n    ");
    message.push_str(&checks.display().to_string());
    message
}

pub fn list(ctx: &Ctx) -> Result<()> {
    let mut ids: Vec<String> = fs::read_dir(&ctx.dhome)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('_') && ctx.dhome.join(n).join("SPEC.md").is_file())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    if ids.is_empty() {
        println!("no tasks");
    }
    for id in ids {
        let t = Task::load(ctx, &id)?;
        let live = status::read_live(&t)?;
        let phase = status::phase(&t, live.as_ref());
        let live_label = match phase.as_str() {
            "interrupted" => Some("interrupted".to_string()),
            "questions" | "implement" | "checks" | "review" => Some(format!("running ({phase})")),
            _ => None,
        };
        let Some(r) = t.last_round() else {
            println!("{id:<10} {}", live_label.as_deref().unwrap_or("not run"));
            continue;
        };
        let no_changes = r.status == RoundStatus::Done
            && r.checks.is_none()
            && t.changed_paths(ctx).is_ok_and(|p| p.is_empty());
        let mut status = live_label.unwrap_or_else(|| r.status_label(no_changes));
        if let Some(rv) = &t.state.review
            && t.wt.is_dir()
            && t.head(ctx).is_ok_and(|h| h == rv.commit)
        {
            let high = rv.high.map_or_else(|| "?".into(), |n| n.to_string());
            write!(status, ", reviewed by {} ({high} high)", rv.worker)?;
        }
        println!(
            "{id:<10} round {}  {:<8} {status}  [{}]",
            r.number,
            r.tier_label(),
            r.worker.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

/// Read-only merge policy shared by the merge transaction and every status view.
#[derive(Debug)]
pub(crate) struct MergeReadiness {
    pub blockers: Vec<String>,
    pub next: Option<String>,
    refusal: Option<String>,
    warnings: Vec<String>,
}

pub(crate) fn merge_readiness(
    ctx: &Ctx,
    t: &Task,
    accept_findings: bool,
    owned: Option<&Operation>,
) -> MergeReadiness {
    let id = &t.id;
    let mut problems = Vec::new();
    let mut next = None;
    let mut warnings = Vec::new();
    let checked = (|| -> Result<()> {
        if owned.is_none() {
            status::ensure_idle(t)?;
        }
        ensure_finished(t)?;
        let Some(implemented) = t.last_done() else {
            bail!("{id} has no implemented round")
        };
        let base = t.base(ctx)?;
        if git(&ctx.root, &["rev-parse", "--abbrev-ref", "HEAD"])? != base {
            bail!("check out {base} first");
        }
        if !git(
            &ctx.root,
            &["status", "--porcelain", "--untracked-files=no"],
        )?
        .is_empty()
        {
            bail!("commit or stash changes in {} first", ctx.root.display());
        }
        let paths = t.changed_paths(ctx)?;
        if paths.is_empty() {
            bail!(
                "{} has no changes to merge (use: delegate discard {id})",
                t.branch
            );
        }

        let head = t.head(ctx)?;
        let mut block = |message: String, command: String| {
            if problems.is_empty() {
                next = Some(command);
            }
            problems.push(message);
        };
        match &implemented.checks {
            Some(c) if c.commit != head => block(
                "acceptance checks haven't run on the final commit (run: delegate check)".into(),
                format!("delegate check {id}"),
            ),
            None => block(
                "acceptance checks haven't run (run: delegate check)".into(),
                format!("delegate check {id}"),
            ),
            Some(c) => match c.outcome {
                CheckOutcome::Pass => {}
                CheckOutcome::NoCommands => block(
                    "the spec has no runnable acceptance commands (lines like: - [ ] `command`)"
                        .into(),
                    format!(
                        "add acceptance commands to {}, then delegate check {id}",
                        t.spec.display()
                    ),
                ),
                CheckOutcome::Fail => block(failed_checks_problem(t), format!("delegate run {id}")),
            },
        }
        let review = t.state.review.as_ref().filter(|r| r.commit == head);
        for rule in ProjectRules::load(&ctx.root)?.matching(&paths)? {
            let matched = rule.matching_paths(&paths)?;
            let mut changed = matched
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if matched.len() > 5 {
                changed.push_str(", ...");
            }
            if let Some(min) = rule.min_tier
                && implemented.tier.is_none_or(|used| used < min)
            {
                block(
                    format!(
                        "rule {} needs tier >= {min}, final round ran at {} (changed: {changed})",
                        rule.label(),
                        implemented.tier_label()
                    ),
                    format!("delegate run {id} --tier {min}"),
                );
            }
            if rule.review == Some(ReviewRule::Always) && review.is_none() {
                block(
                    format!(
                        "rule {} needs a review of the final commit (run: delegate review) (changed: {changed})",
                        rule.label()
                    ),
                    format!("delegate review {id}"),
                );
            }
            if let (Some(max), Some(used)) = (rule.max_tier, implemented.tier)
                && used > max
            {
                warnings.push(format!(
                    "warning: rule {} suggests tier <= {max}, used {used}",
                    rule.label()
                ));
            }
        }
        if let Some(r) = review
            && r.high != Some(0)
            && !accept_findings
        {
            let n = r.high.map_or_else(|| "unknown".into(), |n| n.to_string());
            block(
                format!(
                    "review found {n} high-severity issue(s); fix them in another round, or merge with --accept-findings if the user accepts them"
                ),
                format!("delegate run {id}"),
            );
        }
        if !problems.is_empty() {
            bail!("merge refused:\n  - {}", problems.join("\n  - "));
        }

        // A missing/unreadable spec is also a merge refusal, not a transaction failure.
        t.spec_text()?;
        Ok(())
    })();
    let refusal = checked.err().map(|e| e.to_string());
    if problems.is_empty()
        && let Some(error) = &refusal
    {
        problems.push(error.clone());
    }
    let next = next.or_else(|| {
        problems.first().map(|problem| {
            if problem.contains("delegate discard") {
                format!("delegate discard {id}")
            } else if let Some(base) = problem
                .strip_prefix("check out ")
                .and_then(|s| s.strip_suffix(" first"))
            {
                format!("git checkout {base}")
            } else if problem.contains("commit or stash") {
                "git stash".into()
            } else {
                format!("delegate run {id}")
            }
        })
    });
    MergeReadiness {
        blockers: problems,
        next,
        refusal,
        warnings,
    }
}

#[allow(clippy::too_many_lines)] // Keep the merge transaction and its rollback together.
pub fn merge(ctx: &Ctx, id: &str, accept_findings: bool) -> Result<()> {
    let t = Task::load(ctx, id)?;
    let operation = if t.dir.is_dir() {
        Some(Operation::acquire(&t, "merge")?)
    } else {
        None
    };
    let t = if operation.is_some() {
        Task::load(ctx, id)?
    } else {
        t
    };
    let readiness = merge_readiness(ctx, &t, accept_findings, operation.as_ref());
    for warning in readiness.warnings {
        eprintln!("{warning}");
    }
    if let Some(refusal) = readiness.refusal {
        bail!("{refusal}");
    }
    let base = t.base(ctx)?;
    let text = t.spec_text()?;
    let title = spec::title(&text, id);
    let goal = spec::goal(&text);
    let msg = if goal.is_empty() {
        title.clone()
    } else {
        format!("{title}\n\n{goal}")
    };
    let saved_head = git(&ctx.root, &["rev-parse", "HEAD"])?;
    let merged = git(&ctx.root, &["merge", "-q", "--squash", &t.branch])
        .and_then(|_| git(&ctx.root, &["commit", "-q", "-m", &msg]));
    if let Err(error) = merged {
        // A failed driver or commit hook can dirty paths beyond the conflicts.
        // Always reset, even if enumerating the unmerged paths itself fails.
        let paths = match git(&ctx.root, &["diff", "--name-only", "--diff-filter=U"]) {
            Ok(paths) => paths.lines().collect::<Vec<_>>().join(", "),
            Err(error) => format!("(could not list unmerged paths: {error:#})"),
        };
        let mut diagnostics = vec![format!("Git failure: {error:#}")];
        if let Err(error) = git(&ctx.root, &["reset", "-q", "--hard", &saved_head]) {
            diagnostics.push(format!("rollback reset failed: {error:#}"));
        }
        let restored_head = git(&ctx.root, &["rev-parse", "HEAD"]);
        let remaining = git(
            &ctx.root,
            &["status", "--porcelain", "--untracked-files=no"],
        );
        if !restored_head.as_ref().is_ok_and(|head| head == &saved_head)
            || !remaining.as_ref().is_ok_and(String::is_empty)
        {
            diagnostics.push(format!(
                "rollback verification failed: expected HEAD {saved_head}; HEAD: {}; remaining tracked status:\n{}",
                restored_head.unwrap_or_else(|error| format!("unavailable: {error:#}")),
                remaining.unwrap_or_else(|error| format!("unavailable: {error:#}")),
            ));
        }
        // Only a real conflict gets the "merge base into the task" advice; anything else
        // (nothing to commit, a hook, an untracked file in the way) gets git's own reason.
        if paths.is_empty() {
            bail!(
                "merge refused: git could not merge {} into {base}; {base} is unchanged\n{}",
                t.branch,
                diagnostics.join("\n"),
            );
        }
        bail!(
            "merge refused: {} conflicts with {base} in: {paths}\nmerge {base} into the task in {}, resolve, commit, then: delegate check {id}\n{}",
            t.branch,
            t.wt.display(),
            diagnostics.join("\n"),
        );
    }
    println!("squash-merged {} into {base}: {title}", t.branch);
    remove(ctx, &t).map_err(|error| {
        anyhow::anyhow!(
            "merged, but cleaning up {id} failed: {error:#}; run: delegate discard {id}"
        )
    })
}

fn remove(ctx: &Ctx, t: &Task) -> Result<()> {
    if t.wt.is_dir() {
        git(
            &ctx.root,
            &["worktree", "remove", "--force", &t.wt.to_string_lossy()],
        )?;
    }
    git(&ctx.root, &["worktree", "prune"])?;
    let _ = git(&ctx.root, &["branch", "-q", "-D", &t.branch]);
    if t.dir.exists() {
        fs::remove_dir_all(&t.dir)?;
    }
    println!("removed task state for {}", t.id);
    Ok(())
}

pub fn discard(ctx: &Ctx, id: &str) -> Result<()> {
    let t = Task::load(ctx, id)?;
    status::ensure_idle(&t)?;
    let _operation = if t.dir.is_dir() {
        Some(Operation::acquire(&t, "discard")?)
    } else {
        None
    };
    remove(ctx, &t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TestRepo(PathBuf);

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn merge_fixture() -> Result<(TestRepo, Ctx, Task)> {
        let repo =
            TestRepo(std::env::temp_dir().join(format!("delegate-test-{}", uuid::Uuid::new_v4())));
        let root = repo.0.join("repo");
        fs::create_dir_all(&root)?;
        git(&root, &["init", "-q", "-b", "main"])?;
        git(&root, &["config", "user.name", "Test"])?;
        git(&root, &["config", "user.email", "test@example.invalid"])?;
        git(&root, &["config", "commit.gpgsign", "false"])?;
        fs::write(root.join("code"), "base\n")?;
        fs::write(root.join("other"), "untouched\n")?;
        git(&root, &["add", "."])?;
        git(&root, &["commit", "-qm", "base"])?;
        let ctx = Ctx {
            gitdir: root.join(".git"),
            root,
            dhome: repo.0.join("tasks"),
        };
        let mut t = Task::load(&ctx, "t001")?;
        fs::create_dir_all(&t.dir)?;
        fs::write(&t.spec, "# Task t001: Change code\n")?;
        git(
            &ctx.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &t.branch,
                &t.wt.to_string_lossy(),
                "main",
            ],
        )?;
        fs::write(t.wt.join("code"), "task change\n")?;
        git(&t.wt, &["commit", "-qam", "task change"])?;
        t.state.base = Some("main".into());
        t.state.rounds = vec![Round {
            number: 1,
            tier: Some(Tier::Standard),
            worker: Some("pi:test".into()),
            status: RoundStatus::Done,
            research: false,
            exit: Some(0),
            checks: Some(Checks {
                outcome: CheckOutcome::Pass,
                commit: t.head(&ctx)?,
            }),
        }];
        t.save()?;
        Ok((repo, ctx, t))
    }

    fn assert_failed_merge_restored(ctx: &Ctx, t: &Task, head: &str) -> Result<()> {
        assert_eq!(git(&ctx.root, &["rev-parse", "HEAD"])?, head);
        assert_eq!(
            git(
                &ctx.root,
                &["status", "--porcelain", "--untracked-files=no"]
            )?,
            ""
        );
        assert_eq!(fs::read_to_string(ctx.root.join("other"))?, "untouched\n");
        assert_eq!(fs::read_to_string(t.wt.join("code"))?, "task change\n");
        assert_eq!(git(&ctx.root, &["rev-parse", &t.branch])?, t.head(ctx)?);
        assert!(t.spec.is_file());
        assert_eq!(
            fs::read_to_string(t.dir.join("STATE.json"))?,
            serde_json::to_string_pretty(&t.state)? + "\n"
        );
        Ok(())
    }

    #[test]
    fn conflicting_merge_restores_main_and_keeps_task() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(ctx.root.join("code"), "main change\n")?;
        git(&ctx.root, &["commit", "-qam", "main change"])?;
        let head = git(&ctx.root, &["rev-parse", "HEAD"])?;
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        assert!(error.contains(&format!(
            "merge refused: {} conflicts with main in: code",
            t.branch
        )));
        assert!(error.contains(&format!(
            "merge main into the task in {}, resolve, commit, then: delegate check t001",
            t.wt.display()
        )));
        assert_failed_merge_restored(&ctx, &t, &head)?;
        assert_eq!(fs::read_to_string(ctx.root.join("code"))?, "main change\n");
        Ok(())
    }

    #[test]
    fn failing_merge_driver_restores_its_other_tracked_edits() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(ctx.root.join(".gitattributes"), "code merge=broken\n")?;
        fs::write(ctx.root.join("code"), "main change\n")?;
        git(&ctx.root, &["add", "."])?;
        git(&ctx.root, &["commit", "-qm", "main change and driver"])?;
        git(
            &ctx.root,
            &[
                "config",
                "merge.broken.driver",
                "printf 'driver side effect\\n' > other; printf ran > driver-ran; exit 1",
            ],
        )?;
        fs::write(ctx.root.join("untracked"), "keep me\n")?;
        let head = git(&ctx.root, &["rev-parse", "HEAD"])?;
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        assert!(error.contains("conflicts with main in: code"));
        assert!(!error.contains("rollback verification failed"));
        assert_failed_merge_restored(&ctx, &t, &head)?;
        assert_eq!(fs::read_to_string(ctx.root.join("code"))?, "main change\n");
        assert_eq!(fs::read_to_string(ctx.root.join("driver-ran"))?, "ran");
        assert_eq!(fs::read_to_string(ctx.root.join("untracked"))?, "keep me\n");
        Ok(())
    }

    #[cfg(unix)]
    fn install_commit_hook(ctx: &Ctx, script: &str) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let hooks = ctx.gitdir.join("test-hooks");
        fs::create_dir_all(&hooks)?;
        git(
            &ctx.root,
            &["config", "core.hooksPath", &hooks.to_string_lossy()],
        )?;
        let hook = hooks.join("pre-commit");
        fs::write(&hook, script)?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn failing_squash_commit_restores_main() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        install_commit_hook(
            &ctx,
            "#!/bin/sh\nprintf 'hook side effect\\n' > other\nexit 1\n",
        )?;
        let head = git(&ctx.root, &["rev-parse", "HEAD"])?;
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        assert!(error.contains("Git failure: git commit"));
        assert!(!error.contains("rollback verification failed"));
        assert_failed_merge_restored(&ctx, &t, &head)?;
        assert_eq!(fs::read_to_string(ctx.root.join("code"))?, "base\n");
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn failed_rollback_reports_remaining_tracked_changes() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        install_commit_hook(
            &ctx,
            "#!/bin/sh\nprintf 'hook side effect\\n' > other\n: > .git/index.lock\nexit 1\n",
        )?;
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        assert!(error.contains("rollback reset failed:"));
        assert!(error.contains("rollback verification failed:"));
        assert!(error.contains("remaining tracked status:"));
        assert!(error.contains("M  code"));
        assert!(error.contains(" M other"));
        assert!(t.wt.is_dir());
        assert!(t.dir.join("STATE.json").is_file());
        Ok(())
    }

    #[test]
    fn cleanup_failure_keeps_successful_merge() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        git(&ctx.root, &["worktree", "lock", &t.wt.to_string_lossy()])?;
        let head = git(&ctx.root, &["rev-parse", "HEAD"])?;
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        assert!(error.starts_with("merged, but cleaning up t001 failed:"));
        assert!(error.ends_with("; run: delegate discard t001"));
        assert_ne!(git(&ctx.root, &["rev-parse", "HEAD"])?, head);
        assert_eq!(fs::read_to_string(ctx.root.join("code"))?, "task change\n");
        assert_eq!(git(&ctx.root, &["status", "--porcelain"])?, "");
        assert!(t.wt.is_dir());
        assert!(t.dir.join("STATE.json").is_file());
        Ok(())
    }

    #[test]
    fn unfinished_round_blocks_merge_and_review_without_touching_work() -> Result<()> {
        let repo =
            TestRepo(std::env::temp_dir().join(format!("delegate-test-{}", uuid::Uuid::new_v4())));
        let root = repo.0.join("repo");
        fs::create_dir_all(&root)?;
        git(&root, &["init", "-q", "-b", "main"])?;
        git(&root, &["config", "user.name", "Test"])?;
        git(&root, &["config", "user.email", "test@example.invalid"])?;
        fs::write(root.join("code"), "base\n")?;
        git(&root, &["add", "code"])?;
        git(&root, &["commit", "-qm", "base"])?;
        let ctx = Ctx {
            gitdir: root.join(".git"),
            root,
            dhome: repo.0.join("tasks"),
        };
        let mut t = Task::load(&ctx, "t001")?;
        fs::create_dir_all(&t.dir)?;
        fs::write(&t.spec, "# Task t001\n")?;
        git(
            &ctx.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &t.branch,
                &t.wt.to_string_lossy(),
                "main",
            ],
        )?;
        fs::write(t.wt.join("code"), "completed work\n")?;
        git(&t.wt, &["commit", "-qam", "completed work"])?;
        let checked_commit = t.head(&ctx)?;
        t.state.base = Some("main".into());
        t.state.rounds = vec![
            Round {
                number: 1,
                tier: Some(Tier::Standard),
                worker: Some("pi:test".into()),
                status: RoundStatus::Done,
                research: false,
                exit: Some(0),
                checks: Some(Checks {
                    outcome: CheckOutcome::Pass,
                    commit: checked_commit.clone(),
                }),
            },
            Round {
                number: 2,
                tier: Some(Tier::Standard),
                worker: Some("pi:test".into()),
                status: RoundStatus::Running,
                research: false,
                exit: None,
                checks: None,
            },
        ];
        t.save()?;
        fs::write(t.wt.join("code"), "unfinished work\n")?;
        fs::write(t.wt.join("new-file"), "untracked work\n")?;
        let task_dir = t.wt.join(".task");
        fs::create_dir_all(&task_dir)?;
        fs::write(task_dir.join("REPORT.md"), "unfinished report\n")?;
        let state = fs::read(t.dir.join("STATE.json"))?;
        let main = git(&ctx.root, &["rev-parse", "HEAD"])?;
        let dirty = git(&t.wt, &["status", "--porcelain"])?;
        let expected = "t001: round 2 did not finish; run it again or discard it";

        for option in [false, true] {
            assert_eq!(
                merge(&ctx, &t.id, option).unwrap_err().to_string(),
                expected
            );
            assert_eq!(
                review(&ctx, &t.id, None, None, option)
                    .unwrap_err()
                    .to_string(),
                expected
            );
            assert_eq!(git(&ctx.root, &["rev-parse", "HEAD"])?, main);
            assert_eq!(t.head(&ctx)?, checked_commit);
            assert_eq!(git(&t.wt, &["status", "--porcelain"])?, dirty);
            assert_eq!(fs::read_to_string(t.wt.join("code"))?, "unfinished work\n");
            assert_eq!(
                fs::read_to_string(t.wt.join("new-file"))?,
                "untracked work\n"
            );
            assert_eq!(
                fs::read_to_string(task_dir.join("REPORT.md"))?,
                "unfinished report\n"
            );
            assert_eq!(fs::read(t.dir.join("STATE.json"))?, state);
        }
        Ok(())
    }

    #[test]
    fn run_clears_old_streams_before_preflight_can_fail() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(t.wt.join("code"), "dirty research worktree\n")?;
        for missing_spec in [false, true] {
            fs::write(
                t.dir.join("events.jsonl"),
                "{\"id\":1,\"at\":\"2026-10-25T00:00:00Z\",\"kind\":\"read\",\"target\":\"old\",\"result\":null}\n",
            )?;
            fs::write(t.dir.join("stream.jsonl"), "old output\n")?;
            if missing_spec {
                fs::remove_file(&t.spec)?;
            }
            let args = RunArgs {
                id: t.id.clone(),
                tier: None,
                worker: None,
                questions: QuestionPhase::Never,
                dry_run: false,
                research: !missing_spec,
                background: false,
            };
            let error = run(&ctx, &args).unwrap_err().to_string();
            assert!(
                error.contains(if missing_spec {
                    "not found"
                } else {
                    "uncommitted changes"
                }),
                "{error}"
            );
            for name in ["events.jsonl", "stream.jsonl"] {
                assert_eq!(fs::read(t.dir.join(name))?, Vec::<u8>::new());
            }
            if !missing_spec {
                let views = status::collect(&ctx, Some(&t.id), status::Depth::Phase)?;
                assert!(!views[0].steps_available);
                assert_eq!(views[0].steps, Vec::<serde_json::Value>::new());
            }
        }
        Ok(())
    }

    #[test]
    fn live_operation_refuses_all_competing_task_operations() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        let mut operation = Operation::acquire(&t, "implement")?;
        operation.start("implement", Some("pi:test"), "standard", "1/1")?;
        let args = RunArgs {
            id: t.id.clone(),
            tier: None,
            worker: None,
            questions: QuestionPhase::Auto,
            dry_run: false,
            research: false,
            background: false,
        };
        let expected = format!(
            "t001 is running (implement, pid {}); wait for it, or stop it with: kill {}",
            std::process::id(),
            std::process::id()
        );
        for result in [
            run(&ctx, &args),
            check(&ctx, &t.id),
            review(&ctx, &t.id, None, None, false),
            merge(&ctx, &t.id, false),
            discard(&ctx, &t.id),
        ] {
            assert_eq!(result.unwrap_err().to_string(), expected);
        }
        let views = status::collect(&ctx, Some(&t.id), status::Depth::Phase)?;
        assert_eq!(views[0].phase, "implement");
        assert_eq!(views[0].pid, Some(std::process::id()));
        assert!(!views[0].ready_to_merge);
        assert!(views[0].next.is_none());
        drop(operation);
        assert_eq!(status::read_live(&t)?.unwrap().phase, "done");
        Ok(())
    }

    #[test]
    fn exclusive_lock_blocks_operations_before_live_state_is_written() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        let operation = Operation::acquire(&t, "review")?;
        let error = status::ensure_idle(&t).unwrap_err().to_string();
        assert!(error.contains(&format!("review, pid {}", std::process::id())));
        let views = status::collect(&ctx, None, status::Depth::Phase)?;
        assert!(!views[0].ready_to_merge);
        drop(operation);
        status::ensure_idle(&t)?;
        Ok(())
    }

    #[test]
    fn dirty_check_error_finishes_its_live_state() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(t.wt.join("code"), "not committed\n")?;
        assert!(
            check(&ctx, &t.id)
                .unwrap_err()
                .to_string()
                .contains("commit your changes")
        );
        let live = status::read_live(&t)?.unwrap();
        assert_eq!(live.phase, "done");
        assert!(!live.alive());
        let _next = Operation::acquire(&t, "checks")?;
        Ok(())
    }

    #[test]
    fn dead_check_and_review_override_a_passing_round() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        for phase in ["checks", "review"] {
            let mut operation = Operation::acquire(&t, phase)?;
            operation.start(phase, Some("pi:reviewer"), "deep", "1/1")?;
            drop(operation);
            let mut live = status::read_live(&t)?.unwrap();
            live.phase = phase.into();
            live.pid_start = "0".into(); // A reused pid must never count as this operation.
            fs::write(t.dir.join("status.json"), serde_json::to_vec(&live)?)?;
            let views = status::collect(&ctx, None, status::Depth::Full)?;
            assert_eq!(views[0].phase, "interrupted");
            assert_eq!(views[0].outcome, None);
            assert_eq!(views[0].pid, None);
            assert!(!views[0].stalled);
            assert!(
                views[0]
                    .next
                    .as_ref()
                    .unwrap()
                    .contains("delegate discard t001")
            );
        }
        Ok(())
    }

    fn file_snapshot(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>, std::time::SystemTime)>> {
        let mut result = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                result.extend(file_snapshot(&entry.path())?);
            } else {
                result.push((
                    entry.path(),
                    fs::read(entry.path())?,
                    entry.metadata()?.modified()?,
                ));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    #[test]
    fn status_is_read_only_and_includes_committed_dirty_and_untracked_files() -> Result<()> {
        let (repo, ctx, t) = merge_fixture()?;
        fs::write(t.wt.join("code"), "task change\nuncommitted\n")?;
        fs::write(t.wt.join("untracked"), "new file")?;
        fs::create_dir(t.wt.join(".task"))?;
        fs::write(t.wt.join(".task/private"), "not a changed file\n")?;
        // An existing idle lock must remain byte-for-byte and mtime identical too.
        drop(Operation::acquire(&t, "checks")?);
        let before = file_snapshot(&repo.0)?;
        let views = status::collect(&ctx, Some(&t.id), status::Depth::Full)?;
        let view = &views[0];
        assert_eq!(view.phase, "done");
        assert_eq!(view.outcome.as_deref(), Some("pass"));
        assert!(view.ready_to_merge);
        assert_eq!(view.next.as_deref(), Some("delegate merge t001"));
        assert_eq!(view.changed_files.len(), 2);
        assert_eq!(view.changed_files[0].path, "code");
        assert_eq!(
            (view.changed_files[0].added, view.changed_files[0].removed),
            (2, 1)
        );
        assert_eq!(view.changed_files[1].path, "untracked");
        assert_eq!(view.changed_files[1].added, 1);
        status::render_rows(&views);
        status::render_detail(view);
        let brief = status::collect(&ctx, None, status::Depth::Phase)?;
        assert!(brief[0].changed_files.is_empty());
        assert_eq!(brief[0].log_tail, "");
        assert_eq!(file_snapshot(&repo.0)?, before);
        Ok(())
    }

    #[test]
    fn status_and_merge_share_review_blockers_and_next_action() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(
            ctx.root.join(".delegate.toml"),
            "[[rules]]\npaths = [\"code\"]\nreview = \"always\"\n",
        )?;
        let views = status::collect(&ctx, None, status::Depth::Full)?;
        assert!(!views[0].ready_to_merge);
        assert_eq!(views[0].next.as_deref(), Some("delegate review t001"));
        let error = merge(&ctx, &t.id, false).unwrap_err().to_string();
        for blocker in &views[0].merge_blockers {
            assert!(error.contains(blocker));
        }
        assert!(status::render_rows(&views).contains("needs review"));
        fs::write(
            ctx.root.join(".delegate.toml"),
            "[[rules]]\npaths = [\"code\"]\nmin_tier = \"deep\"\nreview = \"always\"\n",
        )?;
        let views = status::collect(&ctx, None, status::Depth::Phase)?;
        assert_eq!(
            views[0].next.as_deref(),
            Some("delegate run t001 --tier deep")
        );
        Ok(())
    }

    #[test]
    fn research_run_refuses_dirty_worktree() -> Result<()> {
        let repo =
            TestRepo(std::env::temp_dir().join(format!("delegate-test-{}", uuid::Uuid::new_v4())));
        let root = repo.0.join("repo");
        fs::create_dir_all(&root)?;
        git(&root, &["init", "-q", "-b", "main"])?;
        git(&root, &["config", "user.name", "Test"])?;
        git(&root, &["config", "user.email", "test@example.invalid"])?;
        fs::write(root.join("code"), "base\n")?;
        git(&root, &["add", "code"])?;
        git(&root, &["commit", "-qm", "base"])?;
        let ctx = Ctx {
            gitdir: root.join(".git"),
            root,
            dhome: repo.0.join("tasks"),
        };
        let t = Task::load(&ctx, "t001")?;
        fs::create_dir_all(&t.dir)?;
        git(
            &ctx.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &t.branch,
                &t.wt.to_string_lossy(),
                "main",
            ],
        )?;
        fs::write(t.wt.join("code"), "unfinished work\n")?;
        fs::write(t.wt.join("new-file"), "untracked work\n")?;
        let head = t.head(&ctx)?;
        let args = RunArgs {
            id: t.id.clone(),
            tier: None,
            worker: None,
            questions: QuestionPhase::Auto,
            dry_run: false,
            research: true,
            background: false,
        };

        assert_eq!(
            run(&ctx, &args).unwrap_err().to_string(),
            "t001: the worktree has uncommitted changes; commit or discard them before a research run"
        );
        assert_eq!(t.head(&ctx)?, head);
        assert_eq!(fs::read_to_string(t.wt.join("code"))?, "unfinished work\n");
        assert_eq!(
            fs::read_to_string(t.wt.join("new-file"))?,
            "untracked work\n"
        );
        Ok(())
    }

    #[test]
    fn corrupt_live_state_is_local_and_views_remain_read_only() -> Result<()> {
        let (repo, ctx, t) = merge_fixture()?;
        let other = Task::load(&ctx, "t002")?;
        fs::create_dir_all(&other.dir)?;
        fs::write(&other.spec, "# Task t002\n")?;
        let operation = Operation::acquire(&other, "implement")?;
        // Publish without a heartbeat so snapshot comparisons cannot race the test.
        operation.update("implement", Some("pi:test"), "standard", "1/1")?;
        for corrupt in ["", "{", "{}", r#"{"phase":"implement","pid":1}"#] {
            fs::write(t.dir.join("status.json"), corrupt)?;
            let before = file_snapshot(&repo.0)?;
            for depth in [status::Depth::Phase, status::Depth::Full] {
                let views = status::collect(&ctx, None, depth)?;
                assert_eq!(views.len(), 2);
                assert_eq!(views[0].phase, "done");
                assert!(views[0].ready_to_merge);
                assert_eq!(views[1].phase, "implement");
            }
            list(&ctx)?;
            assert_eq!(
                merge_readiness(&ctx, &t, false, None).blockers,
                Vec::<String>::new()
            );
            assert_eq!(file_snapshot(&repo.0)?, before);
        }
        // A read error, not just invalid JSON, is also a missing live snapshot.
        fs::remove_file(t.dir.join("status.json"))?;
        fs::create_dir(t.dir.join("status.json"))?;
        assert!(status::collect(&ctx, None, status::Depth::Phase).is_ok());
        discard(&ctx, &t.id)?;
        operation.phase("done")?;
        Ok(())
    }

    #[test]
    fn stale_atomic_files_are_cleaned_only_by_an_exclusive_operation() -> Result<()> {
        let (repo, ctx, t) = merge_fixture()?;
        for name in [
            ".status-killed.tmp",
            ".STATE-killed.tmp",
            ".status-keep.txt",
        ] {
            fs::write(t.dir.join(name), "leftover")?;
        }
        fs::create_dir(t.dir.join(".status-directory.tmp"))?;
        let before = file_snapshot(&repo.0)?;
        status::collect(&ctx, None, status::Depth::Full)?;
        list(&ctx)?;
        assert_eq!(file_snapshot(&repo.0)?, before);
        let operation = Operation::acquire(&t, "checks")?;
        assert!(!t.dir.join(".status-killed.tmp").exists());
        assert!(!t.dir.join(".STATE-killed.tmp").exists());
        assert!(t.dir.join(".status-keep.txt").exists());
        assert!(t.dir.join(".status-directory.tmp").is_dir());
        fs::write(t.dir.join(".STATE-active.tmp"), "in flight")?;
        assert!(Operation::acquire(&t, "review").is_err());
        assert!(t.dir.join(".STATE-active.tmp").exists());
        drop(operation);
        let _next = Operation::acquire(&t, "review")?;
        assert!(!t.dir.join(".STATE-active.tmp").exists());
        Ok(())
    }

    #[test]
    fn detached_child_publishes_live_before_slow_preflight() -> Result<()> {
        const CHILD: &str = "_DELEGATE_TEST_PREFLIGHT_ROOT";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let ctx = Ctx {
                gitdir: root.join(".git"),
                root,
                dhome: PathBuf::from(std::env::var_os("DELEGATE_HOME").unwrap()),
            };
            return run(
                &ctx,
                &RunArgs {
                    id: "t001".into(),
                    tier: None,
                    worker: Some("codex:gpt-test@high".into()),
                    questions: QuestionPhase::Never,
                    dry_run: false,
                    research: false,
                    background: false,
                },
            );
        }
        let (_repo, ctx, t) = merge_fixture()?;
        fs::remove_file(&t.spec)?;
        assert!(Command::new("mkfifo").arg(&t.spec).status()?.success());
        // Block the repeated spec preflight indefinitely. The child's live state
        // must be visible even when this takes longer than startup's deadline.
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "commands::tests::detached_child_publishes_live_before_slow_preflight",
            ])
            .env(CHILD, &ctx.root)
            .env("DELEGATE_HOME", &ctx.dhome)
            .env("_DELEGATE_BACKGROUND_RUN", &t.id)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let mut observed = None;
        for _ in 0..200 {
            observed = status::read_live(&t)?;
            if observed.is_some() || child.try_wait()?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(2100));
        let still_running = child.try_wait()?.is_none();
        let _ = child.kill();
        child.wait()?;
        assert!(
            still_running,
            "the repeated spec preflight should still be blocked"
        );
        let live = observed.context("the child must publish before reading its spec")?;
        assert_eq!(live.pid, child.id());
        assert_eq!(live.phase, "implement");
        fs::remove_file(&t.spec)?;
        fs::write(&t.spec, "# Task t001\n")?;
        assert_eq!(
            status::collect(&ctx, None, status::Depth::Phase)?[0].phase,
            "interrupted"
        );
        Ok(())
    }

    #[test]
    fn review_evidence_corrupt_status_json_does_not_break_other_views() -> Result<()> {
        let (_repo, ctx, t) = merge_fixture()?;
        fs::write(t.dir.join("status.json"), "")?;
        assert!(status::collect(&ctx, None, status::Depth::Phase).is_ok());
        assert!(list(&ctx).is_ok());
        assert!(discard(&ctx, &t.id).is_ok());
        Ok(())
    }

    #[test]
    fn review_evidence_status_reader_does_not_block_a_starting_operation() -> Result<()> {
        let (_repo, _ctx, t) = merge_fixture()?;
        drop(Operation::acquire(&t, "checks")?);
        let reader = fs::File::open(t.dir.join("operation.lock"))?;
        reader.try_lock_shared()?; // what ensure_idle holds, briefly, for every status poll
        assert!(Operation::acquire(&t, "checks").is_ok());
        Ok(())
    }
}
