//! Check attribution and worker-family policy, using committed history rather than task state.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Worker, workers_config_path};
use crate::spec;
use crate::task::{Ctx, Task, git};

struct Check {
    id: String,
    folder: String,
    jobs: BTreeSet<String>,
    hidden: bool,
    hidden_path: Option<PathBuf>,
}

impl Check {
    fn label(&self) -> &str {
        if self.hidden {
            "a hidden check"
        } else {
            &self.id
        }
    }
}

fn scenarios(root: &Path, hidden: bool) -> Result<Vec<Check>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut checks = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let folder = entry.file_name().to_string_lossy().into_owned();
        let id = folder.split('-').next().unwrap_or_default();
        if !spec::scenario_id(id) || !entry.path().join("check").is_file() {
            continue;
        }
        let jobs = if let Ok(text) = fs::read_to_string(entry.path().join("scenario.md")) {
            text.lines()
                .filter_map(|line| line.trim().strip_prefix("- Job:"))
                .flat_map(|value| value.split(|c: char| c == ',' || c.is_whitespace()))
                .filter(|job| !job.is_empty())
                .map(str::to_string)
                .collect()
        } else {
            let label = if hidden { "a hidden check" } else { id };
            eprintln!(
                "warning: {label} has no readable scenario.md; skipping job-based hidden-check discovery for it; restore that file and retry"
            );
            BTreeSet::new()
        };
        checks.push(Check {
            id: id.into(),
            folder,
            jobs,
            hidden,
            hidden_path: hidden.then(|| entry.path().join("check")),
        });
    }
    checks.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(checks)
}

/// All attributed first-parent changes, following earlier paths through renames.
fn history_families(repo: &Path, base: &str, path: &str, label: &str) -> Result<BTreeSet<String>> {
    let history = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--follow",
            "--name-status",
            "--format=commit:%H",
            base,
            "--",
            path,
        ],
    )?;
    let mut families = BTreeSet::new();
    // --follow also follows copies. Count the copy's own commit, but not the
    // source check's earlier writers: only renames preserve check identity.
    for commit in history
        .lines()
        .take_while(|line| !line.starts_with('C'))
        .filter_map(|line| line.strip_prefix("commit:"))
    {
        let trailers = git(
            repo,
            &[
                "show",
                "-s",
                "--format=%(trailers:key=Worker,valueonly)",
                commit,
                "--",
            ],
        )?;
        let authors = trailers
            .lines()
            .map(str::parse::<Worker>)
            .collect::<Result<Vec<_>>>();
        let Ok(authors) = authors else {
            println!(
                "can't tell who changed {label} in {commit} (unreadable Worker trailer); running without that attribution"
            );
            continue;
        };
        if authors.is_empty() {
            println!(
                "can't tell who changed {label} in {commit} (no Worker trailer); running without that attribution"
            );
        }
        for author in authors {
            let family = author.family();
            println!("{label}: {family} family wrote or fixed the check in {commit}");
            families.insert(family);
        }
    }
    Ok(families)
}

fn writers(ctx: &Ctx, base: &str, check: &Check) -> Result<BTreeSet<String>> {
    let path = format!("evals/scenarios/{}/check", check.folder);
    let mut families = history_families(&ctx.root, base, &path, check.label())?;
    if let Some(path) = &check.hidden_path {
        match hidden_history(path) {
            Ok(writers) => families.extend(writers),
            Err(_) => eprintln!(
                "warning: hidden checks' writers couldn't be found in their source; using available project history; restore readable hidden-check git history and retry"
            ),
        }
    }
    Ok(families)
}

fn hidden_history(path: &Path) -> Result<BTreeSet<String>> {
    let path = fs::canonicalize(path)?;
    let folder = path.parent().context("hidden check has no parent folder")?;
    let repo = PathBuf::from(git(folder, &["rev-parse", "--show-toplevel"])?);
    let relative = path.strip_prefix(&repo)?.to_string_lossy();
    history_families(&repo, "HEAD", &relative, "a hidden check")
}

fn load_checks(ctx: &Ctx, extra_hidden: &[PathBuf]) -> Result<Vec<Check>> {
    let mut checks = scenarios(&ctx.root.join("evals/scenarios"), false)
        .context("reading Acceptance checks; restore readable scenario folders and retry")?;
    let hidden_roots: BTreeSet<_> = crate::commands::hidden_homes(ctx)
        .into_iter()
        .map(|home| home.join("_evals/hidden"))
        .chain(extra_hidden.iter().cloned())
        .collect();
    for root in hidden_roots {
        // Never include private paths or filesystem errors in diagnostics.
        match scenarios(&root.join("scenarios"), true) {
            Ok(hidden) => checks.extend(hidden),
            Err(_) => eprintln!(
                "warning: hidden checks' writers couldn't be found in a hidden folder; using other available checks; restore readable hidden scenario folders and retry"
            ),
        }
    }
    Ok(checks)
}

/// Preserve configured ordering and tier; fix tasks are limited to their checks' writers.
pub(crate) fn candidates(
    ctx: &Ctx,
    task: &Task,
    workers: Vec<Worker>,
    tier: &str,
    text: &str,
) -> Result<Vec<Worker>> {
    let mut extra_hidden = Vec::new();
    if let Some(path) = std::env::var_os("EVALS_HIDDEN").filter(|path| !path.is_empty()) {
        extra_hidden.push(PathBuf::from(path));
    }
    // Hidden names and content must never reach output, including filesystem errors.
    match crate::vault::hidden_project(ctx) {
        Ok(Some(project)) => extra_hidden.push(project.join("hidden")),
        Ok(None) => {}
        Err(_) => eprintln!(
            "warning: hidden checks' writers couldn't be found in the vault; using other available checks; restore or connect the vault with delegate setup --vault <url>"
        ),
    }
    let checks = load_checks(ctx, &extra_hidden)?;
    filter_candidates(ctx, task, workers, tier, text, &checks)
}

fn filter_candidates(
    ctx: &Ctx,
    task: &Task,
    mut workers: Vec<Worker>,
    tier: &str,
    text: &str,
    checks: &[Check],
) -> Result<Vec<Worker>> {
    let acceptance = spec::acceptance_scenarios(text);
    let fixes = spec::fixes_checks(text);
    let base = task.base(ctx)?;
    let jobs: BTreeSet<_> = checks
        .iter()
        // Fixing a check does not implement the job it judges. Only other,
        // explicitly named Acceptance checks bring in job-related hidden checks.
        .filter(|check| acceptance.contains(&check.id) && !fixes.contains(&check.id))
        .flat_map(|check| check.jobs.iter().cloned())
        .collect();
    for id in &fixes {
        if !checks.iter().any(|check| &check.id == id) {
            bail!(
                "fixes_checks: {id} names no existing check; correct the spec or restore its check before running"
            );
        }
    }
    let mut allowed = BTreeSet::new();
    let mut excluded = BTreeSet::new();
    for check in checks {
        let fixing = fixes.contains(&check.id);
        let judges =
            acceptance.contains(&check.id) || (check.hidden && !check.jobs.is_disjoint(&jobs));
        if !fixing && !judges {
            continue;
        }
        let families = match writers(ctx, &base, check) {
            Ok(families) => families,
            Err(_) if check.hidden => {
                eprintln!(
                    "warning: hidden checks' writers couldn't be found; using visible checks only; restore readable vault history and retry"
                );
                BTreeSet::new()
            }
            Err(error) => return Err(error),
        };
        for family in families {
            if fixing {
                println!(
                    "check fix: allowing the {family} family that wrote {}",
                    check.label()
                );
                allowed.insert(family);
            } else {
                println!(
                    "skipping the {family} family: {}'s check writers",
                    check.label()
                );
                excluded.insert(family);
            }
        }
    }
    for id in spec::checks_by(text) {
        let authors = task.check_writers(ctx, &id)?;
        if authors.is_empty() {
            println!(
                "can't tell who wrote {id}'s checks (no readable Worker trailer on its merge); running without that rule"
            );
        }
        for author in authors {
            let family = author.family();
            println!("skipping the {family} family: {id} wrote its checks");
            excluded.insert(family);
        }
    }
    workers.retain(|worker| {
        let family = worker.family();
        !excluded.contains(&family) && (fixes.is_empty() || allowed.contains(&family))
    });
    if workers.is_empty() {
        let skipped = excluded.into_iter().collect::<Vec<_>>().join(", ");
        if !fixes.is_empty() {
            let allowed = allowed.into_iter().collect::<Vec<_>>().join(", ");
            bail!(
                "no [implement].{tier} candidate remains for fixes_checks: {} (writer families: {}; excluded families: {skipped}); add a candidate from the writer families to [implement].{tier} in {}, or restore readable Worker attribution",
                fixes.join(", "),
                if allowed.is_empty() {
                    "unknown"
                } else {
                    &allowed
                },
                workers_config_path().display()
            );
        }
        bail!(
            "no [implement].{tier} candidate remains after excluding {skipped} families (Acceptance: {}; checks_by: {}); add a candidate of another family to [implement].{tier} in {}",
            acceptance.join(", "),
            spec::checks_by(text).join(", "),
            workers_config_path().display()
        );
    }
    Ok(workers)
}

/// Changed check paths belonging to Acceptance, minus explicitly authorized fixes.
pub(crate) fn forbidden_changes(text: &str, paths: &[String]) -> Vec<String> {
    let acceptance = spec::acceptance_scenarios(text);
    let fixes = spec::fixes_checks(text);
    paths
        .iter()
        .filter(|path| {
            let Some(folder) = path
                .strip_prefix("evals/scenarios/")
                .and_then(|p| p.strip_suffix("/check"))
            else {
                return false;
            };
            let id = folder.split('-').next().unwrap_or_default();
            !folder.contains('/')
                && acceptance.iter().any(|seen| seen == id)
                && !fixes.iter().any(|seen| seen == id)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repo(PathBuf);

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture() -> Result<(Repo, Ctx, Task)> {
        let repo =
            Repo(std::env::temp_dir().join(format!("delegate-writers-{}", uuid::Uuid::new_v4())));
        fs::create_dir_all(&repo.0)?;
        git(&repo.0, &["init", "-q", "-b", "main"])?;
        git(&repo.0, &["config", "user.name", "Test"])?;
        git(&repo.0, &["config", "user.email", "test@example.invalid"])?;
        git(&repo.0, &["config", "commit.gpgsign", "false"])?;
        let ctx = Ctx {
            root: repo.0.clone(),
            gitdir: repo.0.join(".git"),
            dhome: repo.0.join("tasks"),
        };
        let mut task = Task::load(&ctx, "t010")?;
        task.state.base = Some("main".into());
        Ok((repo, ctx, task))
    }

    fn change(ctx: &Ctx, id: &str, job: &str, worker: &str) -> Result<()> {
        let folder = ctx.root.join(format!("evals/scenarios/{id}-test"));
        fs::create_dir_all(&folder)?;
        fs::write(folder.join("scenario.md"), format!("- Job: {job}\n"))?;
        fs::write(folder.join("check"), format!("# {worker}\n"))?;
        git(&ctx.root, &["add", "evals"])?;
        git(
            &ctx.root,
            &[
                "commit",
                "-qm",
                &format!("t001: Write {id}\n\nWorker: {worker}"),
            ],
        )?;
        Ok(())
    }

    fn workers() -> Result<Vec<Worker>> {
        [
            "pi:gpt-test@high",
            "pi:deepseek-test@high",
            "pi:claude-test@high",
            "pi:gemini-test@high",
        ]
        .iter()
        .map(|worker| worker.parse())
        .collect()
    }

    #[test]
    fn history_counts_later_fixes_and_all_workers_but_not_other_files() -> Result<()> {
        let (_repo, ctx, _) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        git(
            &ctx.root,
            &[
                "commit",
                "--amend",
                "-qm",
                "t002: Fix\n\nWorker: pi:gpt-test@high\nWorker: pi:claude-test@high",
            ],
        )?;
        change(&ctx, "S002", "J2", "pi:gemini-test@high")?;
        // Legacy changes warn but must not erase older, valid attribution.
        fs::write(
            ctx.root.join("evals/scenarios/S001-test/check"),
            "# legacy\n",
        )?;
        git(&ctx.root, &["commit", "-qam", "legacy change"])?;
        assert_eq!(
            history_families(&ctx.root, "main", "evals/scenarios/S001-test/check", "S001")?,
            ["claude", "deepseek", "gpt"]
                .map(str::to_string)
                .into_iter()
                .collect()
        );
        Ok(())
    }

    #[test]
    fn hidden_writers_are_selected_by_job_without_excluding_unrelated_jobs() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        change(&ctx, "S002", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S003", "J2", "pi:claude-test@high")?;
        let mut checks = scenarios(&ctx.root.join("evals/scenarios"), false)?;
        checks[1].hidden = true;
        checks[2].hidden = true;
        assert_eq!(checks[1].label(), "a hidden check");
        let selected = filter_candidates(
            &ctx,
            &task,
            workers()?,
            "standard",
            "## Acceptance\n- [ ] `evals/run S001`\n",
            &checks,
        )?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["claude", "gemini"]
        );
        Ok(())
    }

    #[test]
    fn fix_exempts_only_named_checks_and_keeps_other_acceptance_exclusions() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        change(&ctx, "S002", "J1", "pi:gpt-test@high")?;
        let checks = scenarios(&ctx.root.join("evals/scenarios"), false)?;
        let text = "fixes_checks: S001\n## Acceptance\n- [ ] `evals/run S001 S002`\n";
        let selected = filter_candidates(&ctx, &task, workers()?, "standard", text, &checks)?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["deepseek"]
        );
        let error = filter_candidates(
            &ctx,
            &task,
            vec!["pi:gpt-test@high".parse()?],
            "fast",
            text,
            &checks,
        )
        .unwrap_err()
        .to_string();
        for want in ["deepseek", "gpt", "S001", "fast", "workers.toml"] {
            assert!(error.contains(want), "{error}");
        }
        Ok(())
    }

    #[test]
    fn reads_hidden_checks_from_current_legacy_and_explicit_homes() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        let legacy = Repo(ctx.root.with_file_name(format!(
            "{}.delegate",
            ctx.root.file_name().unwrap().to_string_lossy()
        )));
        let extra = ctx.root.join("custom-hidden");
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        change(&ctx, "S002", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S003", "J1", "pi:claude-test@high")?;
        change(&ctx, "S004", "J2", "pi:gemini-test@high")?;
        for (id, home) in [
            ("S002", ctx.dhome.join("_evals/hidden")),
            ("S003", legacy.0.join("_evals/hidden")),
            ("S004", extra.clone()),
        ] {
            let scenarios = home.join("scenarios");
            fs::create_dir_all(&scenarios)?;
            fs::rename(
                ctx.root.join(format!("evals/scenarios/{id}-test")),
                scenarios.join(format!("{id}-test")),
            )?;
        }
        let checks = load_checks(&ctx, &[extra])?;
        assert_eq!(checks.len(), 4);
        assert_eq!(checks.iter().filter(|check| check.hidden).count(), 3);
        let selected = filter_candidates(
            &ctx,
            &task,
            workers()?,
            "standard",
            "## Acceptance\n- [ ] `evals/run S001`\n",
            &checks,
        )?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["gemini"]
        );
        Ok(())
    }

    #[test]
    fn hidden_source_history_counts_later_fixes_outside_the_vault() -> Result<()> {
        let (_repo, ctx, _) = fixture()?;
        let (_hidden_repo, hidden_ctx, _) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        change(&hidden_ctx, "S001", "J1", "pi:gpt-test@high")?;
        change(&hidden_ctx, "S001", "J1", "pi:claude-test@high")?;
        let checks = scenarios(&hidden_ctx.root.join("evals/scenarios"), true)?;
        assert_eq!(
            writers(&ctx, "main", &checks[0])?,
            ["claude", "deepseek", "gpt"]
                .map(str::to_string)
                .into_iter()
                .collect()
        );
        Ok(())
    }

    #[test]
    fn hidden_acceptance_brings_in_other_hidden_checks_of_the_same_job() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        change(&ctx, "S002", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S003", "J2", "pi:claude-test@high")?;
        let mut checks = scenarios(&ctx.root.join("evals/scenarios"), false)?;
        for check in &mut checks {
            check.hidden = true;
        }
        let selected = filter_candidates(
            &ctx,
            &task,
            workers()?,
            "standard",
            "## Acceptance\n- [ ] `evals/run S001`\n",
            &checks,
        )?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["claude", "gemini"]
        );
        Ok(())
    }

    #[test]
    fn unreadable_scenario_metadata_does_not_block_unrelated_acceptance() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:gpt-test@high")?;
        let root = ctx.root.join("evals/scenarios");
        for (id, contents) in [("S002", None), ("S003", Some(vec![0xff]))] {
            let folder = root.join(format!("{id}-broken"));
            fs::create_dir_all(&folder)?;
            fs::write(folder.join("check"), "# check\n")?;
            if let Some(contents) = contents {
                fs::write(folder.join("scenario.md"), contents)?;
            }
        }
        for hidden in [false, true] {
            let checks = scenarios(&root, hidden)?;
            assert_eq!(checks.len(), 3);
            assert_eq!(checks[0].id, "S001");
            assert!(checks[1..].iter().all(|check| check.jobs.is_empty()));
            let selected = filter_candidates(
                &ctx,
                &task,
                workers()?,
                "standard",
                "## Acceptance\n- [ ] `evals/run S001`\n",
                &checks,
            )?;
            assert_eq!(
                selected.iter().map(Worker::family).collect::<Vec<_>>(),
                ["deepseek", "claude", "gemini"]
            );
        }
        Ok(())
    }

    #[test]
    fn unreadable_metadata_keeps_explicit_acceptance_and_fix_writers() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S002", "J1", "pi:gpt-test@high")?;
        change(&ctx, "S003", "J1", "pi:claude-test@high")?;
        let root = ctx.root.join("evals/scenarios");
        fs::remove_file(root.join("S001-test/scenario.md"))?;
        fs::write(root.join("S002-test/scenario.md"), [0xff])?;
        for hidden in [false, true] {
            let checks = scenarios(&root, hidden)?;
            assert_eq!(checks.len(), 3);
            assert!(checks[..2].iter().all(|check| check.jobs.is_empty()));
            let acceptance = "## Acceptance\n- [ ] `evals/run S001 S002`\n";
            let selected =
                filter_candidates(&ctx, &task, workers()?, "standard", acceptance, &checks)?;
            // S003 must not be brought in by job when S001/S002's metadata is unreadable.
            assert_eq!(
                selected.iter().map(Worker::family).collect::<Vec<_>>(),
                ["claude", "gemini"]
            );
            let selected = filter_candidates(
                &ctx,
                &task,
                workers()?,
                "standard",
                &format!("fixes_checks: S001, S002\n{acceptance}"),
                &checks,
            )?;
            assert_eq!(
                selected.iter().map(Worker::family).collect::<Vec<_>>(),
                ["gpt", "deepseek"]
            );
        }
        Ok(())
    }

    #[test]
    fn e_scenarios_apply_writer_family_and_merge_policy() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "E003", "J1", "pi:gpt-test@high")?;
        let checks = scenarios(&ctx.root.join("evals/scenarios"), false)?;
        let acceptance = "## Acceptance\n- [ ] `evals/run E003`\n";
        let selected = filter_candidates(&ctx, &task, workers()?, "standard", acceptance, &checks)?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["deepseek", "claude", "gemini"]
        );
        let fix = format!("fixes_checks: E003\n{acceptance}");
        let selected = filter_candidates(&ctx, &task, workers()?, "standard", &fix, &checks)?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["gpt"]
        );
        let paths = ["evals/scenarios/E003-test/check".to_string()];
        assert_eq!(forbidden_changes(acceptance, &paths), paths);
        assert_eq!(forbidden_changes(&fix, &paths), Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn merge_policy_allows_only_named_fixes_not_other_acceptance_checks() {
        let paths = [
            "evals/scenarios/S001-old/check",
            "evals/scenarios/S002-test/check",
            "evals/scenarios/S003-test/check",
            "evals/lib/helper.sh",
        ]
        .map(str::to_string);
        let text = "fixes_checks: S001\n## Acceptance\n- [ ] `evals/run S001 S002`\n";
        assert_eq!(
            forbidden_changes(text, &paths),
            ["evals/scenarios/S002-test/check"]
        );
    }

    #[test]
    fn review_fix_task_is_not_blocked_by_hidden_check_of_the_same_job_and_writer() -> Result<()> {
        let (_repo, ctx, task) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        change(&ctx, "S002", "J1", "pi:deepseek-test@high")?;
        let mut checks = scenarios(&ctx.root.join("evals/scenarios"), false)?;
        checks[1].hidden = true;
        let text = "fixes_checks: S001\n## Acceptance\n- [ ] `evals/run S001`\n";
        let selected = filter_candidates(&ctx, &task, workers()?, "standard", text, &checks)?;
        assert_eq!(
            selected.iter().map(Worker::family).collect::<Vec<_>>(),
            ["deepseek"]
        );
        Ok(())
    }

    #[test]
    fn review_renamed_scenario_folder_keeps_earlier_writers() -> Result<()> {
        let (_repo, ctx, _) = fixture()?;
        change(&ctx, "S001", "J1", "pi:deepseek-test@high")?;
        git(
            &ctx.root,
            &[
                "mv",
                "evals/scenarios/S001-test",
                "evals/scenarios/S001-renamed",
            ],
        )?;
        git(&ctx.root, &["add", "evals"])?;
        git(
            &ctx.root,
            &["commit", "-qm", "t002: Rename\n\nWorker: pi:gpt-test@high"],
        )?;
        let families = history_families(
            &ctx.root,
            "main",
            "evals/scenarios/S001-renamed/check",
            "S001",
        )?;
        assert!(families.contains("deepseek"), "{families:?}");
        Ok(())
    }
}
