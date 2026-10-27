//! Restartable import of the old sibling home. The checkout data lock is held by resolve.
//! Sources survive until a complete, checked copy exists (and hidden files are committed).

use std::fmt::Write;
use std::fs::{self, File};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::checkout::{atomic_write, repair};
use crate::task::Ctx;
use crate::vault::{self, Vault, git};

const JOURNAL: &str = ".legacy-migration.json";

#[derive(Deserialize, Serialize)]
struct Entry {
    path: PathBuf,
    hidden: bool,
    copied: bool,
}

#[derive(Deserialize, Serialize)]
struct Journal {
    source: PathBuf,
    project: String,
    entries: Vec<Entry>,
    done: bool,
}

fn children(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn has_files(path: &Path) -> Result<bool> {
    for child in children(path)? {
        if !fs::symlink_metadata(&child)?.is_dir() || has_files(&child)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn discover(source: &Path) -> Result<Vec<Entry>> {
    let mut paths = Vec::new();
    for path in children(source)? {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir()
            && !name.starts_with(['_', '.'])
            && (path.join("SPEC.md").is_file()
                || path.join("STATE.json").is_file()
                || path.join("worktree/.git").is_file())
        {
            paths.push((PathBuf::from(name.as_ref()), false));
        }
    }
    for (path, hidden) in [
        ("_plan-review", false),
        ("_evals/results", false),
        ("_evals/hidden/scenarios", true),
    ] {
        if source.join(path).exists() {
            paths.push((PathBuf::from(path), hidden));
        }
    }
    Ok(paths
        .into_iter()
        .map(|(path, hidden)| Entry {
            path,
            hidden,
            copied: false,
        })
        .collect())
}

fn save(home: &Path, journal: &Journal, tick: &mut impl FnMut() -> Result<()>) -> Result<()> {
    atomic_write(&home.join(JOURNAL), &serde_json::to_vec(journal)?)?;
    tick()
}

fn conflict(source: &Path, dest: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "migration conflict: {} differs from {}; reconcile these files and retry delegate (both kept)",
        source.display(),
        dest.display()
    )
}

fn same(source: &Path, dest: &Path) -> Result<bool> {
    let a = fs::symlink_metadata(source)?;
    let b = match fs::symlink_metadata(dest) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dest.display())),
    };
    if a.file_type() != b.file_type() {
        return Ok(false);
    }
    if a.is_symlink() {
        return Ok(fs::read_link(source)? == fs::read_link(dest)?);
    }
    if a.is_file() {
        return Ok(a.permissions() == b.permissions() && fs::read(source)? == fs::read(dest)?);
    }
    Ok(a.is_dir())
}

// A single scratch file, outside imported trees. An interrupted copy never exposes
// a partial destination; the next invocation can safely replace this scratch copy.
fn copy_tree(
    source: &Path,
    dest: &Path,
    scratch: &Path,
    tick: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let kind = fs::symlink_metadata(source)?.file_type();
    let existing = match fs::symlink_metadata(dest) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    if existing && !same(source, dest)? {
        return Err(conflict(source, dest));
    }
    if kind.is_dir() {
        if !existing {
            fs::create_dir_all(dest)?;
            tick()?;
        }
        for path in children(source)? {
            copy_tree(
                &path,
                &dest.join(path.file_name().unwrap_or_default()),
                scratch,
                tick,
            )?;
        }
        fs::set_permissions(dest, fs::metadata(source)?.permissions())?;
    } else if !existing {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if scratch.symlink_metadata().is_ok() {
            fs::remove_file(scratch)?;
        }
        if kind.is_file() {
            fs::copy(source, scratch)?;
            File::open(scratch)?.sync_all()?;
        } else if kind.is_symlink() {
            symlink(fs::read_link(source)?, scratch)?;
        } else {
            bail!(
                "cannot migrate {}; only files, directories and symlinks are supported; relocate this special file and retry (source kept)",
                source.display()
            );
        }
        tick()?;
        fs::rename(scratch, dest)?;
        tick()?;
    }
    Ok(())
}

// Do not recursively delete an unchecked source: even a file added after copying
// must be preserved. Cleanup can itself be interrupted between any two files.
fn remove_copied(source: &Path, dest: &Path, tick: &mut impl FnMut() -> Result<()>) -> Result<()> {
    let kind = match fs::symlink_metadata(source) {
        Ok(m) => m.file_type(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if !same(source, dest)? {
        return Err(conflict(source, dest));
    }
    if kind.is_dir() {
        for path in children(source)? {
            remove_copied(
                &path,
                &dest.join(path.file_name().unwrap_or_default()),
                tick,
            )?;
        }
        fs::remove_dir(source)?;
    } else {
        fs::remove_file(source)?;
    }
    tick()
}

fn prune_empty(path: &Path, tick: &mut impl FnMut() -> Result<()>) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.is_dir()
        && children(path)?.is_empty()
    {
        fs::remove_dir(path)?;
        tick()?;
    }
    Ok(())
}

pub(crate) fn run(main: &Path, gitdir: &Path, home: &Path, data: &Path) -> Result<()> {
    // Inside the worker sandbox legacy homes stay where they are: migration commits hidden
    // material to the vault, and the sandbox must not touch or name it (the lead moves homes).
    if vault::sandboxed() {
        return Ok(());
    }
    migrate(main, gitdir, home, data, &mut || Ok(()))
}

// The checkpoint callback is deliberately internal: unit tests interrupt at every
// durable boundary without adding a kill switch to the production CLI.
fn migrate(
    main: &Path,
    gitdir: &Path,
    home: &Path,
    data: &Path,
    tick: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let source = main.with_file_name(format!(
        "{}.delegate",
        main.file_name().unwrap_or_default().to_string_lossy()
    ));
    let saved = match fs::read(home.join(JOURNAL)) {
        Ok(bytes) => Some(serde_json::from_slice::<Journal>(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if saved.as_ref().is_some_and(|j| j.done) || (saved.is_none() && !source.exists()) {
        return Ok(());
    }
    // Hidden material is the only reason to touch the vault, and nothing here waits for it:
    // a busy vault delays the hidden set to the next command (see below).
    let hidden_ahead = match &saved {
        Some(journal) => journal.entries.iter().any(|e| e.hidden),
        None => {
            fs::symlink_metadata(&source).is_ok_and(|m| m.is_dir())
                && discover(&source)?.iter().any(|e| e.hidden)
        }
    };
    let mut vault = Vault::at(data);
    if hidden_ahead {
        vault.ensure_no_conflict()?;
    }
    let identity = File::open(gitdir)?;
    identity.lock()?;
    let ctx = Ctx {
        root: main.into(),
        gitdir: gitdir.into(),
        dhome: home.into(),
    };
    let mut journal = if let Some(saved) = saved {
        saved
    } else {
        if !fs::symlink_metadata(&source)?.is_dir() {
            bail!(
                "{} is not a directory; restore the old home and retry",
                source.display()
            );
        }
        let entries = discover(&source)?;
        let journal = Journal {
            source,
            project: if entries.is_empty() {
                String::new()
            } else {
                vault::read_project_id(&ctx)?.unwrap_or_else(|| Uuid::new_v4().to_string())
            },
            entries,
            done: false,
        };
        save(home, &journal, tick)?;
        journal
    };
    if let Some(id) = vault::read_project_id(&ctx)?
        && !journal.entries.is_empty()
        && id != journal.project
    {
        bail!(
            "project id changed during migration; restore project = {:?} in .delegate-kit.toml and retry",
            journal.project
        );
    }
    let scratch = home.join(".legacy-copy.tmp");
    for index in 0..journal.entries.len() {
        let entry = &journal.entries[index];
        if entry.hidden {
            continue;
        }
        let from = journal.source.join(&entry.path);
        let to = home.join(&entry.path);
        if !entry.copied {
            copy_tree(&from, &to, &scratch, tick)?;
            journal.entries[index].copied = true;
            save(home, &journal, tick)?;
        }
        remove_copied(&from, &to, tick)?;
    }
    repair(main, home)?;
    tick()?;
    let has_hidden = journal.entries.iter().any(|e| e.hidden);
    let mut hidden_dest = None;
    if has_hidden {
        // Another process owns the vault: the task state is moved, the journal stays open and
        // the next command moves the hidden set. Reserve the project id so it can't change.
        if !vault.try_lock()? {
            if vault::read_project_id(&ctx)?.is_none() {
                vault::save_project_id(&ctx, &journal.project)?;
            }
            return Ok(());
        }
        vault.init()?;
        tick()?;
        let project = vault.project(&ctx, &journal.project)?.unwrap_or_else(|| {
            vault.root.join(format!(
                "{}-{}",
                journal.project,
                main.file_name().unwrap_or_default().to_string_lossy()
            ))
        });
        let to = project.join("hidden/scenarios");
        hidden_dest = Some(to.clone());
        for index in 0..journal.entries.len() {
            if journal.entries[index].hidden && !journal.entries[index].copied {
                copy_tree(
                    &journal.source.join(&journal.entries[index].path),
                    &to,
                    &scratch,
                    tick,
                )?;
                journal.entries[index].copied = true;
                save(home, &journal, tick)?;
            }
        }
        // Include any folder rename for this id, but never unrelated staged edits.
        let staged = git(&vault.root, &["diff", "--cached", "--name-only", "-z"])?;
        let prefix = format!("{}-", journal.project);
        if staged
            .split('\0')
            .filter(|p| !p.is_empty())
            .any(|p| !p.starts_with(&prefix))
        {
            bail!(
                "vault has unrelated staged edits; commit them in {} and retry migration (sources kept)",
                vault.root.display()
            );
        }
        let paths = format!(":(glob){}-*/**", journal.project);
        // Git's recursive glob rejects an entirely empty hidden tree. There is
        // nothing to stage then, unless a tracked project-folder rename remains.
        if has_files(&project)? || !git(&vault.root, &["ls-files", "--", &paths])?.is_empty() {
            git(&vault.root, &["add", "--all", "--force", "--", &paths])?;
        }
        tick()?;
        if !git(&vault.root, &["diff", "--cached", "--name-only"])?.is_empty() {
            git(
                &vault.root,
                &[
                    "-c",
                    "user.name=Delegate",
                    "-c",
                    "user.email=delegate@localhost",
                    "commit",
                    "-m",
                    "Migrate legacy delegate hidden scenarios",
                ],
            )?;
        }
        tick()?;
        // Local only: the push waits for the next sync point (decision 011).
        tick()?;
        for entry in journal.entries.iter().filter(|e| e.hidden) {
            remove_copied(&journal.source.join(&entry.path), &to, tick)?;
        }
    }
    if !journal.entries.is_empty() && vault::read_project_id(&ctx)?.is_none() {
        vault::save_project_id(&ctx, &journal.project)?;
        tick()?;
    }
    // Remove only known empty containers. Unknown files, including inside these
    // containers, are left in place and named once in the completion block.
    for path in ["_evals/hidden", "_evals", ""] {
        prune_empty(&journal.source.join(path), tick)?;
    }
    let mut message = String::new();
    if !journal.entries.is_empty() {
        message.push_str("Migrated legacy delegate home:\n");
        for entry in &journal.entries {
            let dest = if entry.hidden {
                hidden_dest
                    .as_deref()
                    .context("missing hidden destination")?
                    .to_path_buf()
            } else {
                home.join(&entry.path)
            };
            writeln!(message, "  {} -> {}", entry.path.display(), dest.display())?;
        }
        message.push_str("  Commit .delegate-kit.toml (project id).\n");
        if !vault.root.join(".git").is_dir() || vault.remote()?.is_none() {
            message.push_str("  No vault remote; connect it with delegate setup --vault <url>\n");
        }
    }
    if journal.source.exists() {
        writeln!(
            message,
            "  Unknown files left in {}",
            journal.source.display()
        )?;
    }
    eprint!("{message}");
    journal.done = true;
    save(home, &journal, tick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        base: PathBuf,
        main: PathBuf,
        home: PathBuf,
        data: PathBuf,
        old: PathBuf,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let base = std::env::temp_dir().join(format!("delegate-migrate-{}", Uuid::new_v4()));
            let main = base.join("shop");
            let data = base.join("delegate");
            let old = base.join("shop.delegate");
            fs::create_dir_all(&main)?;
            crate::task::git(&main, &["init", "-q"])?;
            fs::write(
                main.join(".delegate-kit.toml"),
                "# preserve this\nkit = \"old\"\n[files]\n",
            )?;
            crate::task::git(&main, &["add", "."])?;
            crate::task::git(
                &main,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qm",
                    "initial",
                ],
            )?;
            let home = crate::checkout::resolve(&main.join(".git"), &main, &data)?;
            let f = Self {
                base,
                main,
                home,
                data,
                old,
            };
            for (path, text) in [
                ("t001/SPEC.md", "# Task t001\n"),
                ("t001/STATE.json", "{\"rounds\":[]}"),
                ("t002/SPEC.md", "# Task t002\n"),
                ("_plan-review/report.md", "plan review"),
                ("_evals/results/fast.txt", "pass"),
                ("_evals/hidden/scenarios/S901/check", "#!/bin/sh\nexit 0\n"),
                ("_evals/hidden/scenarios/S901/scenario.md", "# hidden one"),
                ("_evals/hidden/scenarios/S902/scenario.md", "# hidden two"),
            ] {
                let path = f.old.join(path);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(path, text)?;
            }
            fs::set_permissions(
                f.old.join("_evals/hidden/scenarios/S901/check"),
                fs::Permissions::from_mode(0o755),
            )?;
            crate::task::git(
                &f.main,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "--detach",
                    &f.old.join("t001/worktree").to_string_lossy(),
                ],
            )?;
            fs::write(f.old.join("t001/worktree/uncommitted"), "keep my work")?;
            symlink("uncommitted", f.old.join("t001/worktree/link"))?;
            Ok(f)
        }

        fn run(&self, tick: &mut impl FnMut() -> Result<()>) -> Result<()> {
            migrate(
                &self.main,
                &self.main.join(".git"),
                &self.home,
                &self.data,
                tick,
            )
        }

        fn hidden(&self) -> Result<PathBuf> {
            let journal: Journal = serde_json::from_slice(&fs::read(self.home.join(JOURNAL))?)?;
            Ok(self
                .data
                .join(format!("vault/{}-shop/hidden/scenarios", journal.project)))
        }

        fn assert_finished(&self) -> Result<()> {
            assert!(!self.old.exists());
            assert_eq!(
                fs::read_to_string(self.home.join("t001/worktree/uncommitted"))?,
                "keep my work"
            );
            assert_eq!(
                fs::read_link(self.home.join("t001/worktree/link"))?,
                Path::new("uncommitted")
            );
            assert_eq!(
                fs::read_to_string(self.home.join("_evals/results/fast.txt"))?,
                "pass"
            );
            assert!(self.home.join("_plan-review/report.md").is_file());
            assert!(self.home.join("t002/SPEC.md").is_file());
            assert!(
                crate::task::git(&self.main, &["worktree", "list", "--porcelain"])?.contains(
                    &format!("worktree {}", self.home.join("t001/worktree").display())
                )
            );
            assert!(
                crate::task::git(&self.home.join("t001/worktree"), &["status", "--porcelain"])?
                    .contains("uncommitted")
            );
            let hidden = self.hidden()?;
            assert_eq!(
                fs::read_to_string(hidden.join("S901/scenario.md"))?,
                "# hidden one"
            );
            assert_eq!(
                fs::read_to_string(hidden.join("S902/scenario.md"))?,
                "# hidden two"
            );
            assert_eq!(
                fs::metadata(hidden.join("S901/check"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
            assert_eq!(
                git(&self.data.join("vault"), &["status", "--porcelain"])?,
                ""
            );
            assert_eq!(
                git(&self.data.join("vault"), &["rev-list", "--count", "HEAD"])?,
                "2"
            );
            let ctx = Ctx {
                root: self.main.clone(),
                gitdir: self.main.join(".git"),
                dhome: self.home.clone(),
            };
            assert!(vault::read_project_id(&ctx)?.is_some());
            assert!(
                fs::read_to_string(self.main.join(".delegate-kit.toml"))?
                    .ends_with("# preserve this\nkit = \"old\"\n[files]\n")
            );
            Ok(())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn every_interruption_resumes_without_loss_or_duplicate_commits() -> Result<()> {
        let golden = Fixture::new()?;
        let mut count = 0;
        golden.run(&mut || {
            count += 1;
            Ok(())
        })?;
        golden.assert_finished()?;
        for stop in 1..=count {
            let f = Fixture::new()?;
            let mut n = 0;
            let result = f.run(&mut || {
                n += 1;
                if n == stop {
                    bail!("simulated interruption {stop}");
                }
                Ok(())
            });
            assert!(result.is_err(), "checkpoint {stop} was not reached");
            for (relative, text) in [
                ("t001/SPEC.md", "# Task t001\n"),
                ("t001/worktree/uncommitted", "keep my work"),
                ("t002/SPEC.md", "# Task t002\n"),
                ("_plan-review/report.md", "plan review"),
                ("_evals/results/fast.txt", "pass"),
            ] {
                assert!(
                    fs::read_to_string(f.old.join(relative)).is_ok_and(|s| s == text)
                        || fs::read_to_string(f.home.join(relative)).is_ok_and(|s| s == text),
                    "lost {relative} at {stop}"
                );
            }
            for (scenario, text) in [("S901", "# hidden one"), ("S902", "# hidden two")] {
                let old = f
                    .old
                    .join(format!("_evals/hidden/scenarios/{scenario}/scenario.md"));
                assert!(
                    fs::read_to_string(old).is_ok_and(|s| s == text)
                        || f.hidden()?.join(scenario).join("scenario.md").is_file(),
                    "lost {scenario} at {stop}"
                );
            }
            f.run(&mut || Ok(()))?;
            f.run(&mut || Ok(()))?;
            f.assert_finished()
                .with_context(|| format!("after interruption {stop}/{count}"))?;
        }
        Ok(())
    }

    #[test]
    fn empty_hidden_scaffolding_does_not_prevent_migration() -> Result<()> {
        let f = Fixture::new()?;
        for scenario in ["S901", "S902"] {
            fs::remove_dir_all(f.old.join("_evals/hidden/scenarios").join(scenario))?;
        }
        f.run(&mut || Ok(()))?;
        f.run(&mut || Ok(()))?;
        assert!(!f.old.exists());
        assert!(f.home.join("t001/SPEC.md").is_file());
        assert!(f.hidden()?.is_dir());
        assert_eq!(
            git(&f.data.join("vault"), &["rev-list", "--count", "HEAD"])?,
            "1"
        );
        Ok(())
    }

    #[test]
    fn concurrent_first_commands_import_once_under_the_data_lock() -> Result<()> {
        let f = Fixture::new()?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let (main, data, barrier) = (f.main.clone(), f.data.clone(), barrier.clone());
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                crate::checkout::resolve(&main.join(".git"), &main, &data).unwrap()
            }));
        }
        for thread in threads {
            assert_eq!(thread.join().unwrap(), f.home);
        }
        f.assert_finished()
    }

    #[test]
    fn collisions_keep_both_files_and_identical_partial_copies_resume() -> Result<()> {
        let f = Fixture::new()?;
        fs::create_dir(f.home.join("t001"))?;
        fs::write(f.home.join("t001/SPEC.md"), "different")?;
        let error = f.run(&mut || Ok(())).unwrap_err().to_string();
        assert!(error.contains("t001/SPEC.md") && error.contains("differs"));
        assert_eq!(
            fs::read_to_string(f.home.join("t001/SPEC.md"))?,
            "different"
        );
        assert!(f.old.join("t001/SPEC.md").is_file());
        fs::write(f.home.join("t001/SPEC.md"), "# Task t001\n")?;
        fs::write(f.home.join(".legacy-copy.tmp"), "interrupted partial write")?;
        f.run(&mut || Ok(()))?;
        f.assert_finished()
    }

    #[test]
    fn hidden_collision_is_never_overwritten_or_deleted() -> Result<()> {
        let f = Fixture::new()?;
        let mut first = true;
        assert!(
            f.run(&mut || {
                if first {
                    first = false;
                    bail!("after journal");
                }
                Ok(())
            })
            .is_err()
        );
        let hidden = f.hidden()?;
        fs::create_dir_all(hidden.join("S901"))?;
        fs::write(hidden.join("S901/scenario.md"), "different")?;
        assert!(
            f.run(&mut || Ok(()))
                .unwrap_err()
                .to_string()
                .contains("S901/scenario.md")
        );
        assert_eq!(
            fs::read_to_string(hidden.join("S901/scenario.md"))?,
            "different"
        );
        assert!(
            f.old
                .join("_evals/hidden/scenarios/S901/scenario.md")
                .is_file()
        );
        Ok(())
    }

    /// Run the migration on another thread: a bug here must fail the test, not hang it.
    fn run_without_waiting(f: Fixture) -> Fixture {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = f.run(&mut || Ok(())).map_err(|e| format!("{e:#}"));
            let _ = sender.send((f, result));
        });
        let (f, result) = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the migration waited on vault.lock");
        result.unwrap();
        f
    }

    #[test]
    fn a_busy_vault_delays_only_the_hidden_set_to_the_next_command() -> Result<()> {
        let f = Fixture::new()?;
        let held = Vault::lock(&f.data)?;
        let f = run_without_waiting(f);
        // The task state moved at once; the hidden set and the journal wait.
        assert!(f.home.join("t001/SPEC.md").is_file());
        assert!(f.home.join("t002/SPEC.md").is_file());
        assert!(!f.old.join("t001").exists());
        assert!(
            f.old
                .join("_evals/hidden/scenarios/S901/scenario.md")
                .is_file()
        );
        assert!(!f.data.join("vault/.git").exists());
        let journal: Journal = serde_json::from_slice(&fs::read(f.home.join(JOURNAL))?)?;
        assert!(!journal.done);
        let ctx = Ctx {
            root: f.main.clone(),
            gitdir: f.main.join(".git"),
            dhome: f.home.clone(),
        };
        assert_eq!(
            vault::read_project_id(&ctx)?.as_deref(),
            Some(&*journal.project)
        );
        drop(held);
        f.run(&mut || Ok(()))?;
        f.assert_finished()
    }

    #[test]
    fn nothing_hidden_means_no_vault_lock_at_all() -> Result<()> {
        let f = Fixture::new()?;
        fs::remove_dir_all(f.old.join("_evals/hidden"))?;
        let held = Vault::lock(&f.data)?;
        let f = run_without_waiting(f);
        assert!(!f.old.exists());
        assert!(f.home.join("t001/SPEC.md").is_file());
        let journal: Journal = serde_json::from_slice(&fs::read(f.home.join(JOURNAL))?)?;
        assert!(journal.done);
        drop(held);
        Ok(())
    }

    #[test]
    fn an_empty_old_home_needs_no_vault_lock() -> Result<()> {
        let f = Fixture::new()?;
        fs::remove_dir_all(&f.old)?;
        fs::create_dir(&f.old)?;
        let held = Vault::lock(&f.data)?;
        let f = run_without_waiting(f);
        assert!(!f.old.exists());
        drop(held);
        Ok(())
    }

    #[test]
    fn commits_one_migration_commit_locally_and_never_pushes() -> Result<()> {
        let f = Fixture::new()?;
        let remote = f.base.join("remote.git");
        fs::create_dir(&remote)?;
        git(&remote, &["init", "--bare", "--initial-branch=main"])?;
        {
            let v = Vault::lock(&f.data)?;
            v.init()?;
            git(
                &v.root,
                &["remote", "add", "origin", &remote.to_string_lossy()],
            )?;
        }
        f.run(&mut || Ok(()))?;
        f.assert_finished()?;
        let vault = f.data.join("vault");
        assert_eq!(git(&vault, &["rev-list", "--count", "HEAD"])?, "2");
        assert!(git(&vault, &["grep", "hidden two", "HEAD"])?.contains("S902"));
        // The push waits for the next sync point (decision 011).
        assert!(git(&remote, &["rev-parse", "--verify", "main"]).is_err());
        Ok(())
    }

    #[test]
    fn first_command_inside_a_legacy_worktree_follows_its_move_and_main_project_id() -> Result<()> {
        const CHILD: &str = "_DELEGATE_TEST_MIGRATION_WORKTREE";
        if std::env::var_os(CHILD).is_some() {
            let ctx = Ctx::discover()?;
            assert_eq!(ctx.root, ctx.dhome.join("t001/worktree"));
            assert_eq!(std::env::current_dir()?, ctx.root.join("nested"));
            let id = vault::project_id(&ctx)?;
            let journal: Journal = serde_json::from_slice(&fs::read(ctx.dhome.join(JOURNAL))?)?;
            assert_eq!(id, journal.project);
            assert!(
                !fs::read_to_string(ctx.root.join(".delegate-kit.toml"))?.contains("project =")
            );
            vault::where_paths(&ctx)?;
            return Ok(());
        }
        let f = Fixture::new()?;
        fs::create_dir(f.old.join("t001/worktree/nested"))?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "migration::tests::first_command_inside_a_legacy_worktree_follows_its_move_and_main_project_id", "--nocapture"])
            .current_dir(f.old.join("t001/worktree/nested"))
            .env(CHILD, "1").env_remove("DELEGATE_HOME").env("XDG_DATA_HOME", &f.base).output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(&format!(
            "hidden {}",
            f.hidden()?.parent().unwrap().display()
        )));
        f.assert_finished()
    }

    #[test]
    fn override_skips_migration_and_machine_commands_surface_conflicts() -> Result<()> {
        const CHILD: &str = "_DELEGATE_TEST_MIGRATION_DISPATCH";
        if let Some(mode) = std::env::var_os(CHILD) {
            if mode == "override" {
                let ctx = Ctx::discover()?;
                assert_eq!(
                    ctx.dhome,
                    PathBuf::from(std::env::var_os("DELEGATE_HOME").unwrap())
                );
                assert!(
                    ctx.gitdir
                        .parent()
                        .unwrap()
                        .with_file_name("shop.delegate/t001/SPEC.md")
                        .is_file()
                );
            } else {
                for command in [
                    crate::Cmd::Where { pull: false },
                    crate::Cmd::Init { dir: None },
                    crate::Cmd::Setup { vault: None },
                    crate::Cmd::Upgrade { paths: Vec::new() },
                ] {
                    let error = crate::dispatch(command).unwrap_err().to_string();
                    assert!(
                        error.contains("t001/SPEC.md") && error.contains("differs"),
                        "{error}"
                    );
                }
            }
            return Ok(());
        }
        for mode in ["override", "conflict"] {
            let f = Fixture::new()?;
            fs::create_dir(f.home.join("t001"))?;
            fs::write(f.home.join("t001/SPEC.md"), "different")?;
            let mut child = std::process::Command::new(std::env::current_exe()?);
            child.args(["--exact", "migration::tests::override_skips_migration_and_machine_commands_surface_conflicts", "--nocapture"])
                .current_dir(&f.main).env(CHILD, mode).env_remove("DELEGATE_HOME")
                .env("XDG_DATA_HOME", &f.base);
            if mode == "override" {
                child.env("DELEGATE_HOME", f.base.join("custom-home"));
            }
            let output = child.output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(f.old.join("t001/SPEC.md").is_file());
            if mode == "override" {
                assert!(!f.home.join(JOURNAL).exists());
            }
        }
        Ok(())
    }

    #[test]
    fn unknown_files_are_left_and_reported_once() -> Result<()> {
        const CHILD: &str = "_DELEGATE_TEST_MIGRATION_UNKNOWN";
        if std::env::var_os(CHILD).is_some() {
            crate::dispatch(crate::Cmd::List)?;
            crate::dispatch(crate::Cmd::List)?;
            return Ok(());
        }
        for unknown_only in [true, false] {
            let f = Fixture::new()?;
            if unknown_only {
                fs::remove_dir_all(&f.old)?;
            }
            fs::create_dir_all(f.old.join("unknown/nested"))?;
            fs::write(f.old.join("unknown/nested/keep"), "precious")?;
            let output = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "migration::tests::unknown_files_are_left_and_reported_once",
                    "--nocapture",
                ])
                .current_dir(&f.main)
                .env(CHILD, "1")
                .env_remove("DELEGATE_HOME")
                .env("XDG_DATA_HOME", &f.base)
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8_lossy(&output.stderr);
            assert_eq!(text.matches(&*f.old.to_string_lossy()).count(), 1, "{text}");
            assert_eq!(
                fs::read_to_string(f.old.join("unknown/nested/keep"))?,
                "precious"
            );
        }
        Ok(())
    }
}
