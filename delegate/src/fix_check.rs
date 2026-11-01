//! Building and validating one-command check-fix tasks (`delegate fix-check`).

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use anyhow::{Context, Result, bail};

use crate::check_writers::{self, Check};
use crate::commands::{self, QuestionPhase, RunArgs};
use crate::config::{Role, Tier, WorkersConfig};
use crate::spec;
use crate::task::{Ctx, Task, git};

#[derive(Debug)]
pub struct FixCheckArgs {
    pub scenarios: Vec<String>,
    pub from: String,
    pub reason: Option<String>,
    pub tier: Option<Tier>,
    pub worker: Option<String>,
    pub background: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandBlock {
    label: String,
    command: String,
    body: Vec<String>,
}

fn parse_checks_txt(text: &str) -> Vec<CommandBlock> {
    let mut blocks: Vec<CommandBlock> = Vec::new();
    for line in text.lines() {
        if let Some(command) = line.strip_prefix("PASS  ") {
            blocks.push(CommandBlock {
                label: "PASS".into(),
                command: command.into(),
                body: Vec::new(),
            });
        } else if let Some(command) = line.strip_prefix("FAIL  ") {
            blocks.push(CommandBlock {
                label: "FAIL".into(),
                command: command.into(),
                body: Vec::new(),
            });
        } else if let Some(command) = line.strip_prefix("CHECK ERROR  ") {
            blocks.push(CommandBlock {
                label: "CHECK ERROR".into(),
                command: command.into(),
                body: Vec::new(),
            });
        } else if let Some(last) = blocks.last_mut()
            && (line.starts_with("      ") || line.trim().is_empty())
        {
            last.body.push(line.to_string());
        }
    }
    blocks
}

fn scenarios_in_command(command: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for word in command.split(|c: char| !c.is_ascii_alphanumeric()) {
        if spec::scenario_id(word) && !ids.iter().any(|seen| seen == word) {
            ids.push(word.to_string());
        }
    }
    ids
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScenarioVerdict {
    Pass,
    Fail,
}

/// Extract `(scenario_id, verdict, hidden)` from a single `evals/run` verdict line.
/// Only reads the scenario's own parenthesized id (`(S001)` or `(S001, hidden)`),
/// never words inside the trailing `: <reason>` text.
fn parse_verdict_line(line: &str) -> Option<(String, ScenarioVerdict, bool)> {
    let trimmed = line.trim();
    let verdict = if trimmed.starts_with("FAIL ")
        || trimmed.starts_with("HUMAN ")
        || trimmed.starts_with("check error ")
    {
        ScenarioVerdict::Fail
    } else if trimmed.starts_with("pass ")
        || trimmed.starts_with("skip ")
        || trimmed.starts_with("no check ")
    {
        ScenarioVerdict::Pass
    } else {
        return None;
    };
    let head = if trimmed.starts_with("check error ") {
        let idx = trimmed.find("):")?;
        &trimmed[..=idx]
    } else {
        trimmed
    };
    let open = head.rfind('(')?;
    let inside = head.get(open + 1..)?.strip_suffix(')')?;
    let (id, hidden) = match inside.split_once(',') {
        Some((id, rest)) => (id.trim(), rest.trim() == "hidden"),
        None => (inside.trim(), false),
    };
    if !spec::scenario_id(id) {
        return None;
    }
    Some((id.to_string(), verdict, hidden))
}

fn redact_hidden_ids(line: &str, hidden_ids: &BTreeSet<String>) -> String {
    if hidden_ids.is_empty() {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.is_empty() {
            return;
        }
        if hidden_ids.contains(token.as_str()) {
            out.push_str("<hidden>");
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() {
            token.push(ch);
        } else {
            flush(&mut token, &mut out);
            out.push(ch);
        }
    }
    flush(&mut token, &mut out);
    out
}

#[cfg(test)]
pub(crate) fn failed_scenarios_in_checks(text: &str) -> Vec<String> {
    failed_scenarios_in_checks_filtered(text, &[], &BTreeSet::new())
}

fn failed_scenarios_in_checks_filtered(
    text: &str,
    visible_ids: &[String],
    hidden_ids: &BTreeSet<String>,
) -> Vec<String> {
    let blocks = parse_checks_txt(text);
    let mut failed = Vec::new();
    let mut record = |id: String, hidden: bool| {
        if hidden || hidden_ids.contains(&id) {
            return;
        }
        if !visible_ids.is_empty() && !visible_ids.iter().any(|v| v == &id) {
            return;
        }
        if !failed.contains(&id) {
            failed.push(id);
        }
    };
    for block in &blocks {
        if block.label == "PASS" {
            continue;
        }
        let cmd_ids = scenarios_in_command(&block.command);
        let mut saw_verdicts = false;
        for line in &block.body {
            if let Some((id, verdict, hidden)) = parse_verdict_line(line) {
                saw_verdicts = true;
                if verdict == ScenarioVerdict::Fail {
                    record(id, hidden);
                }
            }
        }
        if !saw_verdicts {
            for id in cmd_ids {
                record(id, false);
            }
        }
    }
    failed
}

#[cfg(test)]
fn failing_output_for_scenarios(text: &str, scenarios: &[String]) -> String {
    failing_output_for_scenarios_filtered(text, scenarios, &BTreeSet::new())
}

fn failing_output_for_scenarios_filtered(
    text: &str,
    scenarios: &[String],
    hidden_ids: &BTreeSet<String>,
) -> String {
    let mut hidden_ids = hidden_ids.clone();
    let blocks = parse_checks_txt(text);
    for block in &blocks {
        for line in &block.body {
            if let Some((id, _, true)) = parse_verdict_line(line) {
                hidden_ids.insert(id);
            }
        }
    }
    let mut matched = Vec::new();
    for block in &blocks {
        if block.label == "PASS" {
            continue;
        }
        let mut saw_verdicts = false;
        let mut selected_lines: Vec<String> = Vec::new();
        let mut header_lines: Vec<String> = Vec::new();
        let mut footer_lines: Vec<String> = Vec::new();
        let mut capturing = false;
        let mut in_footer = false;
        for line in &block.body {
            let trimmed = line.trim();
            if let Some((id, verdict, hidden)) = parse_verdict_line(line) {
                saw_verdicts = true;
                in_footer = false;
                capturing = !hidden
                    && !hidden_ids.contains(&id)
                    && verdict == ScenarioVerdict::Fail
                    && scenarios.iter().any(|s| s == &id);
                if capturing {
                    selected_lines.push(redact_hidden_ids(line, &hidden_ids));
                }
            } else if saw_verdicts
                && (trimmed.starts_with("-- ")
                    || trimmed.starts_with("results: ")
                    || trimmed.starts_with("(no hidden scenarios found)"))
            {
                capturing = false;
                in_footer = true;
                footer_lines.push(redact_hidden_ids(line, &hidden_ids));
            } else if !saw_verdicts {
                header_lines.push(redact_hidden_ids(line, &hidden_ids));
            } else if capturing {
                selected_lines.push(redact_hidden_ids(line, &hidden_ids));
            } else if in_footer {
                footer_lines.push(redact_hidden_ids(line, &hidden_ids));
            }
        }
        if saw_verdicts {
            if !selected_lines.is_empty() {
                let mut rendered = format!(
                    "{}  {}",
                    block.label,
                    redact_hidden_ids(&block.command, &hidden_ids)
                );
                for line in header_lines
                    .iter()
                    .chain(selected_lines.iter())
                    .chain(footer_lines.iter())
                {
                    rendered.push('\n');
                    rendered.push_str(line);
                }
                matched.push(rendered);
            }
        } else {
            let cmd_ids = scenarios_in_command(&block.command);
            let hits = scenarios.iter().any(|id| {
                cmd_ids.contains(id)
                    || block.body.iter().any(|l| {
                        l.split(|c: char| !c.is_ascii_alphanumeric())
                            .any(|w| w == id)
                    })
            });
            if hits {
                let mut rendered = format!(
                    "{}  {}",
                    block.label,
                    redact_hidden_ids(&block.command, &hidden_ids)
                );
                for line in &block.body {
                    rendered.push('\n');
                    rendered.push_str(&redact_hidden_ids(line, &hidden_ids));
                }
                matched.push(rendered);
            }
        }
    }
    if matched.is_empty() {
        redact_hidden_ids(text.trim_end(), &hidden_ids)
    } else {
        matched.join("\n")
    }
}

/// Format acceptance command output for `CHECKS.txt`: keep every failing scenario's output whole,
/// and shorten non-scenario command output to the last 15 lines.
pub(crate) fn format_check_output(command: &str, out: &str) -> Vec<String> {
    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let all_lines: Vec<&str> = trimmed.lines().collect();
    let has_scenarios = !scenarios_in_command(command).is_empty()
        || all_lines.iter().any(|line| {
            let t = line.trim_start();
            t.starts_with("evals: ")
                || t.starts_with("FAIL ")
                || t.starts_with("pass ")
                || t.starts_with("check error ")
        });
    if has_scenarios {
        all_lines
            .into_iter()
            .map(|line| format!("      {line}"))
            .collect()
    } else {
        let tail = &all_lines[all_lines.len().saturating_sub(15)..];
        tail.iter().map(|line| format!("      {line}")).collect()
    }
}

fn parse_task_number(s: &str) -> Option<u32> {
    s.trim().strip_prefix('t')?.parse::<u32>().ok()
}

fn plan_table_task_numbers(plan_text: &str) -> Vec<u32> {
    let mut nums = Vec::new();
    for section_name in ["Next", "Tasks"] {
        for line in spec::section(plan_text, section_name).lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') {
                continue;
            }
            for cell in trimmed.split('|') {
                if let Some(n) = parse_task_number(cell) {
                    nums.push(n);
                }
            }
        }
    }
    nums
}

pub(crate) fn next_task_id(ctx: &Ctx) -> String {
    let mut max = fs::read_dir(&ctx.dhome)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| parse_task_number(&entry.file_name().to_string_lossy()))
        .max()
        .unwrap_or(0);
    if let Ok(refs) = git(
        &ctx.root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/task/",
        ],
    ) {
        for line in refs.lines() {
            if let Some(id) = line.strip_prefix("task/")
                && let Some(n) = parse_task_number(id)
            {
                max = max.max(n);
            }
        }
    }
    if let Ok(plan) = fs::read_to_string(ctx.root.join("PLAN.md")) {
        for n in plan_table_task_numbers(&plan) {
            max = max.max(n);
        }
    }
    let next = max + 1;
    if next < 1000 {
        format!("t{next:03}")
    } else {
        format!("t{next}")
    }
}

fn check_problems_from_report(report_text: &str) -> String {
    let problems = spec::section(report_text, "Check problems").trim();
    if problems.is_empty() {
        "(no Check problems section recorded in REPORT.md)".to_string()
    } else {
        problems.to_string()
    }
}

fn fence_for(text: &str) -> String {
    let mut ticks = 3;
    while text.contains(&"`".repeat(ticks)) {
        ticks += 1;
    }
    "`".repeat(ticks)
}

fn build_fix_spec(
    id: &str,
    from_id: &str,
    scenarios: &[String],
    checks_map: &[(String, String, String)],
    failing_output: &str,
    check_problems: &str,
    reason: &str,
) -> String {
    let scenario_list = scenarios.join(", ");
    let run_list = scenarios.join(" ");
    let reason = reason
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let scope_in = checks_map
        .iter()
        .map(|(_, folder, _)| format!("`evals/scenarios/{folder}/check`"))
        .collect::<Vec<_>>()
        .join(", ");
    let context_items = checks_map
        .iter()
        .map(|(_, folder, _)| {
            format!(
                "- `evals/scenarios/{folder}/scenario.md`\n- `evals/scenarios/{folder}/check`"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut spec = String::new();
    let _ = writeln!(
        spec,
        "# Task {id}: Fix {scenario_list} check(s) (from {from_id})\n"
    );
    let _ = writeln!(spec, "fixes_checks: {scenario_list}");
    let _ = writeln!(spec, "from_task: {from_id}");
    let _ = writeln!(spec, "size: small\n");
    let _ = writeln!(spec, "## Goal");
    let _ = writeln!(
        spec,
        "Fix {scenario_list}'s check (flagged from {from_id}): {reason}\n"
    );
    let _ = writeln!(spec, "## Lead's reason");
    let _ = writeln!(spec, "  {reason}\n");
    let _ = writeln!(spec, "## Context (read these, nothing more)");
    let _ = writeln!(spec, "- `AGENTS.md`");
    let _ = writeln!(spec, "{context_items}\n");
    let _ = writeln!(spec, "## Scope");
    let _ = writeln!(spec, "- In: {scope_in}");
    let _ = writeln!(
        spec,
        "- Out: product files and every check not listed in `fixes_checks:`\n"
    );
    for (sc_id, folder, sc_text) in checks_map {
        let fence = fence_for(sc_text);
        let _ = writeln!(
            spec,
            "## Scenario {sc_id} (`evals/scenarios/{folder}/scenario.md`)"
        );
        let _ = writeln!(spec, "{fence}markdown\n{}\n{fence}\n", sc_text.trim_end());
    }
    let out_fence = fence_for(failing_output);
    let _ = writeln!(
        spec,
        "## Failing output from {from_id}'s `CHECKS.txt`"
    );
    let _ = writeln!(
        spec,
        "{out_fence}text\n{}\n{out_fence}\n",
        failing_output.trim_end()
    );
    let prob_fence = fence_for(check_problems);
    let _ = writeln!(
        spec,
        "## Check problems from the implementer's REPORT.md on {from_id} (the work being judged)"
    );
    let _ = writeln!(
        spec,
        "{prob_fence}text\n{}\n{prob_fence}\n",
        check_problems.trim_end()
    );
    let _ = writeln!(spec, "## Contract");
    let _ = writeln!(
        spec,
        "- Compare each check in `fixes_checks:` against its `scenario.md`, the failing output above, and the lead's reason."
    );
    let _ = writeln!(
        spec,
        "- If the check is wrong, fix only the check so it matches `scenario.md`."
    );
    let _ = writeln!(
        spec,
        "- If the check is already right and the failure is a product bug in {from_id}, change no files and write `Status: check is right` in `.task/REPORT.md` with your explanation.\n"
    );
    let _ = writeln!(spec, "## Acceptance");
    let _ = writeln!(spec, "- [ ] `evals/run {run_list}`\n");
    let _ = writeln!(spec, "## Constraints");
    let _ = writeln!(
        spec,
        "- Touch only the checks named in `fixes_checks:`. Do not edit product code or `scenario.md`.\n"
    );
    let _ = writeln!(spec, "## If unsure");
    let _ = writeln!(
        spec,
        "STOP. Don't guess. Write the question under \"Open questions\" in `.task/REPORT.md` and finish.\n"
    );
    let _ = writeln!(spec, "## When done");
    let _ = writeln!(spec, "- Do not commit; the runner commits for you.");
    let _ = writeln!(
        spec,
        "- Write `.task/REPORT.md` from `.task/REPORT.template.md`, 40 lines max."
    );
    spec
}

pub fn fix_check(ctx: &Ctx, args: &FixCheckArgs) -> Result<()> {
    let _lead = crate::autopilot::authorize(ctx)?;
    let reason = args
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing --reason; pass --reason \"<why the check is wrong>\" to record why this check fix is needed"
            )
        })?;
    if args.scenarios.is_empty() {
        bail!("name at least one scenario id (e.g. delegate fix-check S001 --from t003 --reason \"...\")");
    }
    let mut requested = Vec::new();
    for raw in &args.scenarios {
        for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !spec::scenario_id(part) {
                bail!("'{part}' is not a valid scenario id (expected e.g. S001); pass the failing scenario id to delegate fix-check");
            }
            if !requested.iter().any(|seen| seen == part) {
                requested.push(part.to_string());
            }
        }
    }
    let source = Task::load(ctx, &args.from)?;
    if !source.dir.is_dir() {
        bail!(
            "source task {} does not exist; pass --from <task> for a task whose last round failed checks",
            source.id
        );
    }
    let checks_path = source.dir.join("CHECKS.txt");
    let checks_text = fs::read_to_string(&checks_path).with_context(|| {
        format!(
            "{} has no CHECKS.txt ({}); run delegate check {} or delegate run {} first",
            source.id,
            checks_path.display(),
            source.id,
            source.id
        )
    })?;
    let visible_checks = check_writers::scenarios(&ctx.root.join("evals/scenarios"), false)?;
    let visible_ids: Vec<String> = visible_checks.iter().map(|c| c.id.clone()).collect();
    let hidden_ids = check_writers::hidden_scenario_ids(ctx);
    let failed_in_round =
        failed_scenarios_in_checks_filtered(&checks_text, &visible_ids, &hidden_ids);
    for sc in &requested {
        if !failed_in_round.contains(sc) {
            let failed_list = if failed_in_round.is_empty() {
                "none".to_string()
            } else {
                failed_in_round.join(", ")
            };
            bail!(
                "{sc} did not fail in {}'s last round (failed in {}'s last round: {failed_list}); pass a scenario that failed in {}",
                source.id,
                source.id,
                checks_path.display()
            );
        }
    }

    let mut checks_map: Vec<(String, String, String)> = Vec::new();
    for sc in &requested {
        let Some(found): Option<&Check> = visible_checks.iter().find(|c| &c.id == sc) else {
            bail!(
                "fixes_checks: {sc} names no existing check in evals/scenarios/; restore its check before running delegate fix-check"
            );
        };
        let sc_path = ctx
            .root
            .join("evals/scenarios")
            .join(&found.folder)
            .join("scenario.md");
        let sc_text = fs::read_to_string(&sc_path).with_context(|| {
            format!(
                "reading {}; restore the scenario file before running delegate fix-check",
                sc_path.display()
            )
        })?;
        checks_map.push((sc.clone(), found.folder.clone(), sc_text));
    }

    let report_text = fs::read_to_string(source.dir.join("REPORT.md"))
        .or_else(|_| fs::read_to_string(source.wt.join(".task/REPORT.md")))
        .unwrap_or_default();
    let check_problems =
        redact_hidden_ids(&check_problems_from_report(&report_text), &hidden_ids);
    let failing_output =
        failing_output_for_scenarios_filtered(&checks_text, &requested, &hidden_ids);
    let new_id = next_task_id(ctx);
    let spec_text = build_fix_spec(
        &new_id,
        &source.id,
        &requested,
        &checks_map,
        &failing_output,
        &check_problems,
        reason,
    );

    // Preflight candidate selection before creating any task folder or branch (S102).
    let cfg = WorkersConfig::load()?;
    let tier = args.tier.unwrap_or_else(|| cfg.default_tier(Role::Implement));
    let tier_label = tier.to_string();
    let base = source.base(ctx)?;
    let mut probe = Task::load(ctx, &new_id)?;
    probe.state.base = Some(base);
    let raw_workers = match &args.worker {
        Some(w) => vec![w.parse()?],
        None => cfg.candidates(Role::Implement, tier, None)?,
    };
    let _eligible = check_writers::candidates(ctx, &probe, raw_workers, &tier_label, &spec_text)?;

    commands::new(ctx, &new_id, false)?;
    let fix_task = Task::load(ctx, &new_id)?;
    fs::write(&fix_task.spec, &spec_text)?;

    let run_result = commands::run(
        ctx,
        &RunArgs {
            id: new_id.clone(),
            tier: args.tier,
            worker: args.worker.clone(),
            questions: QuestionPhase::Never,
            dry_run: false,
            research: false,
            background: args.background,
        },
    );
    if let Err(error) = run_result {
        // Clean up if starting failed before any work so a refusal leaves no task behind.
        let _ = commands::discard(ctx, &new_id);
        return Err(error);
    }
    Ok(())
}

/// Check whether `REPORT.md` in `t` declares `Status: check is right`, and if so extract
/// the worker's explanation.
pub(crate) fn check_is_right_verdict(t: &Task) -> Option<String> {
    let text = fs::read_to_string(t.dir.join("REPORT.md"))
        .or_else(|_| fs::read_to_string(t.wt.join(".task/REPORT.md")))
        .ok()?;
    let mut lines = text.lines();
    let mut found = false;
    let mut body = Vec::new();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Status:")
            && rest.trim().eq_ignore_ascii_case("check is right")
        {
            found = true;
            for remaining in lines {
                if remaining.starts_with("## ") {
                    break;
                }
                body.push(remaining);
            }
            break;
        }
        if trimmed == "## Status" {
            for next in lines.by_ref() {
                let nt = next.trim();
                if nt.is_empty() || nt.starts_with("<!--") {
                    continue;
                }
                if nt.eq_ignore_ascii_case("check is right")
                    || nt
                        .strip_prefix("Status:")
                        .is_some_and(|r| r.trim().eq_ignore_ascii_case("check is right"))
                {
                    found = true;
                    for remaining in lines {
                        if remaining.starts_with("## ") {
                            break;
                        }
                        body.push(remaining);
                    }
                }
                break;
            }
            break;
        }
    }
    if !found {
        return None;
    }
    let explanation = body.join("\n").trim().to_string();
    let explanation = if explanation.is_empty() {
        text.lines()
            .skip_while(|l| !l.trim().to_ascii_lowercase().contains("check is right"))
            .skip(1)
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        explanation
    };
    Some(explanation)
}

/// Append the check-writer's `check is right` verdict as a product failure onto the source task's
/// `CHECKS.txt` so the implementer receives it on its next round (S103).
pub(crate) fn record_check_is_right_on_source(
    ctx: &Ctx,
    fix_task: &Task,
    spec_text: &str,
    verdict_reason: &str,
) -> Result<()> {
    let Some(from_id) = spec::from_task(spec_text) else {
        return Ok(());
    };
    let source = Task::load(ctx, &from_id)?;
    if !source.dir.is_dir() {
        return Ok(());
    }
    let scenarios = spec::fixes_checks(spec_text);
    let label = if scenarios.is_empty() {
        "check".to_string()
    } else {
        scenarios.join(", ")
    };
    let checks_file = source.dir.join("CHECKS.txt");
    let mut existing = fs::read_to_string(&checks_file).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    let _ = writeln!(
        existing,
        "\nProduct failure ({label}: check judged right by {}): {}",
        fix_task.id,
        verdict_reason.trim()
    );
    fs::write(&checks_file, &existing)?;
    if source.wt.join(".task").is_dir() {
        let _ = fs::write(source.wt.join(".task/CHECKS.txt"), &existing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_failed_scenarios_and_preserves_full_failing_output() {
        let mut raw = String::from("evals: selected  (2026-11-01 12:00)\n");
        raw.push_str("pass     Tax calculation  (S002)\n");
        raw.push_str("FAIL     Pricing rules  (S001)\n");
        for i in 1..=25 {
            let _ = writeln!(raw, "         detail line {i} for S001 early and late output");
        }
        raw.push_str("-- 1 passed, 1 failed, 0 check error, 0 need a human, 0 skipped, 0 not automated yet\n");
        let formatted = format_check_output("evals/run S001 S002", &raw);
        let checks_txt = format!(
            "FAIL  evals/run S001 S002\n{}\nPASS  evals/run S003\n",
            formatted.join("\n")
        );
        assert!(
            checks_txt.contains("detail line 1 for S001"),
            "must not truncate to last 15 lines: {checks_txt}"
        );
        assert!(checks_txt.contains("detail line 25 for S001"));
        assert_eq!(failed_scenarios_in_checks(&checks_txt), vec!["S001"]);
    }

    #[test]
    fn non_scenario_output_is_shortened_to_last_15_lines() {
        let mut raw = String::new();
        for i in 1..=25 {
            let _ = writeln!(raw, "compiler error line {i}");
        }
        let formatted = format_check_output("cargo test", &raw);
        assert_eq!(formatted.len(), 15);
        assert_eq!(formatted[0], "      compiler error line 11");
        assert_eq!(formatted[14], "      compiler error line 25");
    }

    #[test]
    fn build_fix_spec_includes_scenario_failure_check_problems_and_reason() {
        let checks = vec![(
            "S001".to_string(),
            "S001-pricing".to_string(),
            "# S001 Pricing\n- Job: J1\nContract: stderr.\n".to_string(),
        )];
        let spec = build_fix_spec(
            "t004",
            "t003",
            &["S001".to_string()],
            &checks,
            "FAIL  evals/run S001\n      S001 mismatch",
            "S001 reads stdout instead of stderr.",
            "the check reads stdout",
        );
        assert!(spec.contains("fixes_checks: S001"));
        assert!(spec.contains("from_task: t003"));
        assert!(spec.contains("- [ ] `evals/run S001`"));
        assert!(spec.contains("# S001 Pricing\n- Job: J1\nContract: stderr."));
        assert!(spec.contains("FAIL  evals/run S001\n      S001 mismatch"));
        assert!(spec.contains("S001 reads stdout instead of stderr."));
        assert!(spec.contains("the work being judged"));
        assert!(spec.contains("the check reads stdout"));
    }

    #[test]
    fn parses_check_is_right_verdict_in_both_forms() {
        let dir = std::env::temp_dir().join(format!("delegate-verdict-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let ctx = Ctx {
            root: dir.clone(),
            dhome: dir.join("tasks"),
            gitdir: dir.join(".git"),
        };
        let t = Task::load(&ctx, "t004").unwrap();
        fs::create_dir_all(&t.dir).unwrap();
        fs::write(
            t.dir.join("REPORT.md"),
            "# Report t004\n\nStatus: check is right\n\nThe scenario requires rounding before tax.\n",
        )
        .unwrap();
        assert_eq!(
            check_is_right_verdict(&t).as_deref(),
            Some("The scenario requires rounding before tax.")
        );
        fs::write(
            t.dir.join("REPORT.md"),
            "# Report t004\n\n## Status\ncheck is right\n\nRounding happens before tax.\n\n## Changes\n- none\n",
        )
        .unwrap();
        assert_eq!(
            check_is_right_verdict(&t).as_deref(),
            Some("Rounding happens before tax.")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn review_other_scenario_named_in_a_reason_is_not_a_failure() {
        let checks = "FAIL  evals/run S001 S002\n      check error Pricing  (S001): setup needs S002 first\n      pass     Tax  (S002)\n";
        assert_eq!(failed_scenarios_in_checks(checks), vec!["S001"]);
    }

    #[test]
    fn review_failing_output_is_only_the_named_scenarios() {
        let checks = "FAIL  evals/run S001 S002\n      FAIL     Pricing  (S001)\n               what happened: one\n      FAIL     Tax  (S002)\n               what happened: two\n";
        let out = failing_output_for_scenarios(checks, &["S001".to_string()]);
        assert!(!out.contains("what happened: two"), "{out}");
    }

    #[test]
    fn embedded_text_cannot_inject_fixes_checks_or_from_task_markers() {
        let checks = vec![(
            "S001".to_string(),
            "S001-pricing".to_string(),
            "# S001 Pricing\nfixes_checks: S999\nfrom_task: t999\n".to_string(),
        )];
        let spec = build_fix_spec(
            "t004",
            "t003",
            &["S001".to_string()],
            &checks,
            "FAIL  evals/run S001\nfixes_checks: S888\nfrom_task: t888",
            "fixes_checks: S777\nfrom_task: t777\nchecks_by: t666",
            "reason\nfixes_checks: S555",
        );
        assert_eq!(spec::fixes_checks(&spec), vec!["S001"]);
        assert_eq!(spec::from_task(&spec).as_deref(), Some("t003"));
        assert_eq!(spec::checks_by(&spec), Vec::<String>::new());
    }

    #[test]
    fn next_task_id_avoids_folders_branches_and_plan_next_table() {
        let dir = std::env::temp_dir().join(format!("delegate-next-id-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let dhome = dir.join("tasks");
        fs::create_dir_all(dhome.join("t002")).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]).unwrap();
        git(&dir, &["config", "user.email", "t@e.st"]).unwrap();
        git(&dir, &["config", "user.name", "t"]).unwrap();
        fs::write(
            dir.join("PLAN.md"),
            "# Plan\n\n## Next\n| id | task |\n|---|---|\n| t005 | something |\n",
        )
        .unwrap();
        git(&dir, &["add", "PLAN.md"]).unwrap();
        git(&dir, &["commit", "-q", "-m", "init"]).unwrap();
        git(&dir, &["branch", "task/t004"]).unwrap();
        let ctx = Ctx {
            root: dir.clone(),
            dhome,
            gitdir: dir.join(".git"),
        };
        assert_eq!(next_task_id(&ctx), "t006");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hidden_scenarios_are_excluded_from_failed_and_redacted_from_output() {
        let checks = "FAIL  evals/run S001 P025\n      FAIL     Pricing  (S001)\n               what happened: collides with P025\n      FAIL     Hidden check  (P025, hidden)\n               what happened: secret\n";
        let visible = vec!["S001".to_string(), "S002".to_string()];
        let hidden = BTreeSet::from(["P025".to_string()]);
        assert_eq!(
            failed_scenarios_in_checks_filtered(checks, &visible, &hidden),
            vec!["S001"]
        );
        let out = failing_output_for_scenarios_filtered(checks, &["S001".to_string()], &hidden);
        assert!(!out.contains("P025"), "{out}");
        assert!(!out.contains("secret"), "{out}");
        assert!(out.contains("Pricing  (S001)"), "{out}");
    }
}
