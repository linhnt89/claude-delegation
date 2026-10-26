//! Machine-local checkout identity and task homes. Never infer identity from a path name.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::task::git;

const ID: &str = "delegate-checkout-id";
const RECORD: &str = "checkout.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct Checkout {
    id: String,
    path: PathBuf,
}

fn lock(data: &Path) -> Result<File> {
    fs::create_dir_all(data)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data.join("checkouts.lock"))?;
    file.lock()
        .context("locking checkout homes; retry delegate")?;
    Ok(file)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let pending = path.with_file_name(format!(".checkout-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        fs::write(&pending, bytes)?;
        fs::rename(&pending, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}

fn read_id(gitdir: &Path) -> Result<Option<String>> {
    match fs::read_to_string(gitdir.join(ID)) {
        Ok(text) => {
            let id = Uuid::parse_str(text.trim())
                .context("invalid checkout id in git common dir; restore it and retry")?;
            Ok(Some(id.to_string()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading checkout id; check git common dir permissions"),
    }
}

fn holds_id(path: &Path, id: &str) -> bool {
    // Do not run git at an unrelated parent repo if this path was replaced.
    if !path.join(".git").exists() {
        return false;
    }
    git(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()
    .and_then(|dir| read_id(Path::new(&dir)).ok().flatten())
    .is_some_and(|found| found == id)
}

fn read_record(folder: &Path) -> Result<Checkout> {
    serde_json::from_slice(&fs::read(folder.join(RECORD))?).with_context(|| {
        format!(
            "reading {}; restore its checkout record and retry",
            folder.display()
        )
    })
}

fn save_record(folder: &Path, record: &Checkout) -> Result<()> {
    atomic_write(&folder.join(RECORD), &serde_json::to_vec(record)?)
}

fn find_home(checkouts: &Path, id: &str) -> Result<Option<PathBuf>> {
    let mut found = None;
    let prefix = format!("{id}-");
    for entry in fs::read_dir(checkouts)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with(&prefix) {
            if found.is_some() {
                bail!(
                    "multiple checkout homes for {id}; reconcile {} and retry",
                    checkouts.display()
                );
            }
            found = Some(entry.path());
        }
    }
    Ok(found)
}

/// Resolve before sweeping: a moved checkout with only results must not be swept first.
/// The machine-wide lock serializes folder renames, copying decisions and cleanup.
/// The git-directory lock also serializes identity creation across different data roots.
pub(crate) fn resolve(gitdir: &Path, main: &Path, data: &Path) -> Result<PathBuf> {
    let _homes = lock(data)?;
    let identity = File::open(gitdir)?;
    identity
        .lock()
        .context("locking checkout identity; retry delegate")?;
    let checkouts = data.join("checkouts");
    fs::create_dir_all(&checkouts)?;
    let mut id = if let Some(id) = read_id(gitdir)? {
        id
    } else {
        let id = Uuid::new_v4().to_string();
        atomic_write(&gitdir.join(ID), id.as_bytes())?;
        id
    };
    let mut folder = find_home(&checkouts, &id)?;
    if let Some(existing) = &folder {
        let record = read_record(existing)?;
        if record.path != main && holds_id(&record.path, &id) {
            // cp -a copied only the main checkout. Its linked worktree records
            // still refer to the original's worktrees; never repair/remove those.
            let records = gitdir.join("worktrees");
            if records.exists() {
                fs::remove_dir_all(records)?;
            }
            git(main, &["worktree", "prune"])?;
            id = Uuid::new_v4().to_string();
            atomic_write(&gitdir.join(ID), id.as_bytes())?;
            folder = None;
        }
    }
    let name = main.file_name().unwrap_or_default().to_string_lossy();
    let desired = checkouts.join(format!("{id}-{name}"));
    let previous = folder.as_ref().map(|path| read_record(path)).transpose()?;
    let renamed = folder.as_ref().is_some_and(|path| path != &desired);
    if let Some(path) = &folder {
        if renamed {
            fs::rename(path, &desired)
                .context("renaming checkout home; check permissions and retry")?;
        }
    } else {
        fs::create_dir(&desired)?;
    }
    // Always repair before updating the record. On failure the next command
    // retries, including after a successful folder rename but failed repair.
    if renamed || previous.as_ref().is_some_and(|record| record.path != main) {
        repair(main, &desired)?;
    }
    save_record(
        &desired,
        &Checkout {
            id,
            path: main.into(),
        },
    )?;
    sweep(&checkouts)?;
    Ok(desired)
}

fn repair(main: &Path, folder: &Path) -> Result<()> {
    git(main, &["worktree", "repair"])?;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let wt = entry.path().join("worktree");
        if entry.file_type()?.is_dir() && wt.join(".git").is_file() {
            git(main, &["worktree", "repair", &wt.to_string_lossy()])
                .context("repairing moved task worktree; retry delegate from the main checkout")?;
        }
    }
    Ok(())
}

fn has_tasks(folder: &Path) -> Result<bool> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        // A task is open until merge/discard deletes it, even if it finished or
        // has incomplete/corrupt metadata. Never guess that it can be deleted.
        if !entry.file_name().to_string_lossy().starts_with(['_', '.'])
            && (entry.file_type()?.is_dir() || entry.path().join("SPEC.md").is_file())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sweep(checkouts: &Path) -> Result<Vec<PathBuf>> {
    let mut orphans = Vec::new();
    if !checkouts.is_dir() {
        return Ok(orphans);
    }
    for entry in fs::read_dir(checkouts)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.path();
        // Unknown/malformed records are not permission to destroy task state.
        let Ok(record) = read_record(&folder) else {
            continue;
        };
        if holds_id(&record.path, &record.id) {
            continue;
        }
        if has_tasks(&folder)? {
            orphans.push(folder);
        } else {
            fs::remove_dir_all(&folder).with_context(|| {
                format!("cleaning {}; check permissions and retry", folder.display())
            })?;
        }
    }
    orphans.sort();
    Ok(orphans)
}

pub(crate) fn cleanup() -> Result<Vec<PathBuf>> {
    let data = crate::vault::data_dir();
    if !data.join("checkouts").is_dir() {
        return Ok(vec![]);
    }
    let _lock = lock(&data)?;
    sweep(&data.join("checkouts"))
}

pub(crate) fn list_orphans() -> Result<()> {
    for folder in cleanup()? {
        println!("open tasks from a missing checkout: {}", folder.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Result<Self> {
            let f =
                Self(std::env::temp_dir().join(format!("delegate-checkout-{}", Uuid::new_v4())));
            fs::create_dir_all(f.main())?;
            git(&f.main(), &["init", "-q"])?;
            git(
                &f.main(),
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
                    "--allow-empty",
                ],
            )?;
            Ok(f)
        }

        fn main(&self) -> PathBuf {
            self.0.join("shop")
        }
        fn data(&self) -> PathBuf {
            self.0.join("delegate")
        }
        fn home(&self) -> Result<PathBuf> {
            resolve(&self.main().join(".git"), &self.main(), &self.data())
        }
        fn task(&self, home: &Path) -> Result<PathBuf> {
            let wt = home.join("t001/worktree");
            fs::create_dir(home.join("t001"))?;
            fs::write(home.join("t001/SPEC.md"), "# Task t001\n")?;
            git(
                &self.main(),
                &["worktree", "add", "-q", "--detach", &wt.to_string_lossy()],
            )?;
            Ok(wt)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn concurrent_first_use_has_one_id_and_one_home() -> Result<()> {
        let f = Fixture::new()?;
        let barrier = Arc::new(Barrier::new(12));
        let mut threads = Vec::new();
        for _ in 0..12 {
            let (main, data, barrier) = (f.main(), f.data(), barrier.clone());
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                resolve(&main.join(".git"), &main, &data).unwrap()
            }));
        }
        let homes: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(homes.iter().all(|h| h == &homes[0]));
        let id = read_id(&f.main().join(".git"))?.unwrap();
        assert!(
            homes[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&id)
        );
        assert_eq!(fs::read_dir(f.data().join("checkouts"))?.count(), 1);
        assert_eq!(
            fs::read_dir(f.main().join(".git"))?
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with(".checkout-"))
                .count(),
            0
        );
        Ok(())
    }

    #[test]
    fn move_and_rename_repair_both_worktree_links() -> Result<()> {
        let f = Fixture::new()?;
        let old = f.home()?;
        let wt = f.task(&old)?;
        fs::write(wt.join("uncommitted"), "keep this work")?;
        let moved = f.0.join("new-parent/shop2");
        fs::create_dir(moved.parent().unwrap())?;
        fs::rename(f.main(), &moved)?;
        let home = resolve(&moved.join(".git"), &moved, &f.data())?;
        assert!(!old.exists());
        assert!(
            home.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-shop2")
        );
        assert_eq!(
            fs::read_to_string(home.join("t001/worktree/uncommitted"))?,
            "keep this work"
        );
        assert_eq!(
            git(
                &home.join("t001/worktree"),
                &["rev-parse", "--git-common-dir"]
            )?,
            moved.join(".git").to_string_lossy()
        );
        assert!(
            git(&moved, &["worktree", "list", "--porcelain"])?.contains(&format!(
                "worktree {}",
                home.join("t001/worktree").display()
            ))
        );
        assert_eq!(resolve(&moved.join(".git"), &moved, &f.data())?, home);
        Ok(())
    }

    #[test]
    fn move_without_tasks_preserves_results_before_sweeping() -> Result<()> {
        let f = Fixture::new()?;
        let old = f.home()?;
        fs::create_dir_all(old.join("_evals/results"))?;
        fs::write(old.join("_evals/results/fast.txt"), "pass")?;
        let moved = f.0.join("renamed");
        fs::rename(f.main(), &moved)?;
        let home = resolve(&moved.join(".git"), &moved, &f.data())?;
        assert_eq!(
            fs::read_to_string(home.join("_evals/results/fast.txt"))?,
            "pass"
        );
        Ok(())
    }

    #[test]
    fn copy_gets_new_identity_and_cannot_touch_original_worktrees() -> Result<()> {
        let f = Fixture::new()?;
        let original = f.home()?;
        let wt = f.task(&original)?;
        let id = read_id(&f.main().join(".git"))?;
        let backlink = fs::read(wt.join(".git"))?;
        let admin = git(&wt, &["rev-parse", "--absolute-git-dir"])?;
        let record = fs::read(Path::new(&admin).join("gitdir"))?;
        let copy = f.0.join("experiment");
        assert!(
            std::process::Command::new("cp")
                .arg("-a")
                .arg(f.main())
                .arg(&copy)
                .status()?
                .success()
        );
        let home = resolve(&copy.join(".git"), &copy, &f.data())?;
        assert_ne!(home, original);
        assert_ne!(read_id(&copy.join(".git"))?, id);
        assert_eq!(read_id(&f.main().join(".git"))?, id);
        assert!(!copy.join(".git/worktrees").exists());
        assert!(!has_tasks(&home)?);
        assert_eq!(fs::read(wt.join(".git"))?, backlink);
        assert_eq!(fs::read(Path::new(&admin).join("gitdir"))?, record);
        assert_eq!(git(&wt, &["status", "--porcelain"])?, "");
        assert_eq!(resolve(&copy.join(".git"), &copy, &f.data())?, home);
        Ok(())
    }

    #[test]
    fn cleanup_keeps_every_open_task_and_reports_each_home_once() -> Result<()> {
        let f = Fixture::new()?;
        let home = f.home()?;
        f.task(&home)?;
        fs::create_dir(home.join("t002"))?; // Incomplete task creation is still open.
        fs::write(home.join("t001/STATE.json"), "corrupt")?;
        fs::remove_dir_all(f.main())?;
        let checkouts = f.data().join("checkouts");
        assert_eq!(sweep(&checkouts)?, vec![home.clone()]);
        assert_eq!(sweep(&checkouts)?, vec![home.clone()]);
        fs::remove_dir_all(home.join("t001"))?;
        assert_eq!(sweep(&checkouts)?, vec![home.clone()]);
        fs::remove_dir_all(home.join("t002"))?;
        fs::create_dir_all(home.join("_evals/results"))?;
        assert_eq!(sweep(&checkouts)?, Vec::<PathBuf>::new());
        assert!(!home.exists());
        Ok(())
    }

    #[test]
    fn list_outside_a_repo_mentions_retained_home_once() -> Result<()> {
        const CHILD: &str = "_DELEGATE_TEST_ORPHAN_LIST";
        if std::env::var_os(CHILD).is_some() {
            assert_eq!(
                crate::dispatch(crate::Cmd::List)?,
                std::process::ExitCode::SUCCESS
            );
            return Ok(());
        }
        let f = Fixture::new()?;
        let home = f.home()?;
        f.task(&home)?;
        fs::remove_dir_all(f.main())?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "checkout::tests::list_outside_a_repo_mentions_retained_home_once",
                "--nocapture",
            ])
            .current_dir(&f.0)
            .env(CHILD, "1")
            .env_remove("DELEGATE_HOME")
            .env("XDG_DATA_HOME", &f.0)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(text.matches(&*home.to_string_lossy()).count(), 1, "{text}");
        assert!(home.join("t001/SPEC.md").is_file());
        Ok(())
    }

    #[test]
    fn cleanup_detects_replaced_checkout_and_preserves_unknown_folders() -> Result<()> {
        let f = Fixture::new()?;
        let home = f.home()?;
        atomic_write(
            &f.main().join(".git").join(ID),
            Uuid::new_v4().to_string().as_bytes(),
        )?;
        let unknown = f.data().join("checkouts/unknown");
        fs::create_dir(&unknown)?;
        assert_eq!(sweep(&f.data().join("checkouts"))?, Vec::<PathBuf>::new());
        assert!(!home.exists());
        assert!(unknown.exists());
        Ok(())
    }
}
