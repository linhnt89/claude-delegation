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
) -> Result<Attempt> {
    let task_dir = t.wt.join(".task");
    let sb = harness::sandbox(Some(w.harness), &task_bounds(ctx, t)?, &t.wt)?;
    let run = |session: Option<&Session>, prompt: &str| -> Result<i32> {
        let cmd = [
            sb.clone(),
            harness::command(w, prompt, session, false, None),
        ]
        .concat();
        harness::run_logged(&cmd, &t.wt, log, None)
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
    let rc = match &resumed {
        Some(s) => run(Some(s), prompts::WORK)?,
        None => run(None, &prompts::full(&t.id))?,
    };
    Ok(Attempt::Worked(rc))
}

pub fn run(ctx: &Ctx, a: &RunArgs) -> Result<()> {
    let mut t = Task::load(ctx, &a.id)?;
    let spec_text = t.spec_text()?;
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
    let implemented = t.state.rounds.iter().any(|r| r.status == RoundStatus::Done);
    let ask = !a.research
        && match a.questions {
            QuestionPhase::Always => true,
            QuestionPhase::Never => false,
            QuestionPhase::Auto => !implemented,
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
        fs::copy(t.dir.join("CHECKS.txt"), &checks_path)
            .context("copying failed checks into .task/CHECKS.txt; restore the task's CHECKS.txt and retry")?;
    } else {
        match fs::remove_file(&checks_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).context("removing stale .task/CHECKS.txt; check file permissions and retry");
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
        exit: None,
        checks: None,
    });

    let mut outcome = None;
    for w in &available {
        println!("round {number}: {w} ...");
        let log_start = match fs::metadata(&log) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        t.state.rounds[round_index].worker = Some(w.to_string());
        t.save()?;
        match implement_attempt(ctx, &t, w, ask, a.research, &log)? {
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
    if a.research {
        t.state.rounds[round_index].status = RoundStatus::Done;
        t.save()?;
        println!("research report: {}", report.display());
        if !t.changed_paths(ctx)?.is_empty() {
            println!("warning: the researcher changed files; review or discard them");
        }
        return Ok(());
    }
    if t.changed_paths(ctx)?.is_empty() {
        println!("no changes: {w} changed nothing");
    } else {
        t.state.rounds[round_index].checks = Some(run_checks(ctx, &t)?);
    }
    t.state.rounds[round_index].status = RoundStatus::Done;
    t.save()?;
    summary(ctx, &t)
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
fn run_checks(ctx: &Ctx, t: &Task) -> Result<Checks> {
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
    ensure_finished(&t)?;
    if !t.wt.is_dir() || t.last_done().is_none() {
        bail!("{id} has no implemented round");
    }
    if t.worktree_dirty()? {
        bail!(
            "commit your changes in {} first; checks run on the committed branch",
            t.wt.display()
        );
    }
    let checks = run_checks(ctx, &t)?;
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
    for w in &workers {
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
        let Some(r) = t.last_round() else {
            println!("{id:<10} not run");
            continue;
        };
        let no_changes =
            r.status == RoundStatus::Done && r.checks.is_none() && t.changed_paths(ctx).is_ok_and(|p| p.is_empty());
        let mut status = r.status_label(no_changes);
        if let Some(rv) = &t.state.review
            && t.wt.is_dir()
            && t.head(ctx).is_ok_and(|h| h == rv.commit)
        {
            let high = rv.high.map_or_else(|| "?".into(), |n| n.to_string());
            write!(status, ", reviewed ({high} high)")?;
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

pub fn merge(ctx: &Ctx, id: &str, accept_findings: bool) -> Result<()> {
    let t = Task::load(ctx, id)?;
    ensure_finished(&t)?;
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
    let mut problems = Vec::new();
    match &implemented.checks {
        Some(c) if c.commit != head => problems.push(
            "acceptance checks haven't run on the final commit (run: delegate check)".to_string(),
        ),
        None => problems.push("acceptance checks haven't run (run: delegate check)".to_string()),
        Some(c) => match c.outcome {
            CheckOutcome::Pass => {}
            CheckOutcome::NoCommands => problems.push(
                "the spec has no runnable acceptance commands (lines like: - [ ] `command`)".into(),
            ),
            CheckOutcome::Fail => problems.push(format!(
                "acceptance checks failed ({})",
                t.dir.join("CHECKS.txt").display()
            )),
        },
    }
    let review = t.state.review.as_ref().filter(|r| r.commit == head);
    for rule in ProjectRules::load(&ctx.root)?.matching(&paths)? {
        if let Some(min) = rule.min_tier
            && implemented.tier.is_none_or(|used| used < min)
        {
            problems.push(format!(
                "rule {} needs tier >= {min}, final round ran at {}",
                rule.label(),
                implemented.tier_label()
            ));
        }
        if rule.review == Some(ReviewRule::Always) && review.is_none() {
            problems.push(format!(
                "rule {} needs a review of the final commit (run: delegate review)",
                rule.label()
            ));
        }
        if let (Some(max), Some(used)) = (rule.max_tier, implemented.tier)
            && used > max
        {
            eprintln!(
                "warning: rule {} suggests tier <= {max}, used {used}",
                rule.label()
            );
        }
    }
    if let Some(r) = review
        && r.high != Some(0)
        && !accept_findings
    {
        let n = r.high.map_or_else(|| "unknown".into(), |n| n.to_string());
        problems.push(format!(
            "review found {n} high-severity issue(s); fix them in another round, or merge with --accept-findings if the user accepts them"
        ));
    }
    if !problems.is_empty() {
        bail!("merge refused:\n  - {}", problems.join("\n  - "));
    }

    let text = t.spec_text()?;
    let title = spec::title(&text, id);
    let goal = spec::goal(&text);
    let msg = if goal.is_empty() {
        title.clone()
    } else {
        format!("{title}\n\n{goal}")
    };
    git(&ctx.root, &["merge", "-q", "--squash", &t.branch])?;
    git(&ctx.root, &["commit", "-q", "-m", &msg])?;
    println!("squash-merged {} into {base}: {title}", t.branch);
    remove(ctx, &t)
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
    remove(ctx, &Task::load(ctx, id)?)
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
}
