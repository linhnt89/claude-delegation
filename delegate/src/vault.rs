//! Project identity and the machine-wide, locked git vault.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::config::home;
use crate::task::Ctx;
use crate::upgrade::{RECORD, Record};

pub(crate) fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map_or_else(|| home().join(".local/share"), PathBuf::from)
        .join("delegate")
}

/// Serialize first-use identity creation, including commands in linked worktrees.
/// Lock the existing git directory, so a read-only `where` creates no lock file.
pub(crate) fn project_id(ctx: &Ctx) -> Result<String> {
    let lock = File::open(&ctx.gitdir)?;
    lock.lock()
        .context("locking project identity; retry delegate")?;
    if let Some(id) = read_project_id(ctx)? {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    save_project_id(ctx, &id)?;
    eprintln!("added project id; commit .delegate-kit.toml");
    Ok(id)
}

// Caller holds the git-directory identity lock, including when migration reserves an id.
pub(crate) fn read_project_id(ctx: &Ctx) -> Result<Option<String>> {
    let path = ctx.gitdir.parent().unwrap_or(&ctx.root).join(RECORD);
    let text = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if let Some(text) = &text {
        let record: toml::Value =
            toml::from_str(text).context("invalid .delegate-kit.toml; repair it and retry")?;
        if let Some(id) = record.get("project") {
            let id = id
                .as_str()
                .context("project in .delegate-kit.toml must be a UUID v4 string")?;
            let uuid =
                Uuid::parse_str(id).context("project in .delegate-kit.toml must be a UUID v4")?;
            if uuid.get_version_num() != 4 || uuid.get_variant() != uuid::Variant::RFC4122 {
                bail!("project in .delegate-kit.toml must be a UUID v4; repair it and retry");
            }
            return Ok(Some(uuid.to_string()));
        }
    }
    Ok(None)
}

pub(crate) fn save_project_id(ctx: &Ctx, id: &str) -> Result<()> {
    let path = ctx.gitdir.parent().unwrap_or(&ctx.root).join(RECORD);
    let text = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let body = if let Some(text) = text {
        format!("project = \"{id}\"\n{text}")
    } else {
        let mut record = Record::new();
        record.project = Some(id.to_owned());
        toml::to_string(&record)?
    };
    crate::checkout::atomic_write(&path, body.as_bytes())
        .context("writing project id; check .delegate-kit.toml permissions")
}

/// Git used for vault operations must never ask for credentials or launch an editor.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String> {
    git_input(dir, args, None, "")
}

/// A git call that talks to the remote (`fetch`, `push`, `ls-remote`). Whatever it started is
/// killed when it returns, whether git exited or the limit ran out.
fn git_net(dir: &Path, args: &[&str]) -> Result<String> {
    let bytes = git_bounded(dir, args, None, "", GIT_LIMIT, true)?;
    Ok(String::from_utf8_lossy(&bytes).trim_end().to_owned())
}

fn git_input(dir: &Path, args: &[&str], index: Option<&Path>, input: &str) -> Result<String> {
    let bytes = git_bytes(dir, args, index, input)?;
    Ok(String::from_utf8_lossy(&bytes).trim_end().to_owned())
}

/// How long one git call may run. Transports and credential helpers are bounded by it too.
const GIT_LIMIT: Duration = Duration::from_secs(10);

fn git_bytes(dir: &Path, args: &[&str], index: Option<&Path>, input: &str) -> Result<Vec<u8>> {
    git_bounded(dir, args, index, input, GIT_LIMIT, false)
}

/// An anonymous scratch file: it has no name once created, so nothing is left behind.
fn scratch() -> Result<File> {
    let path = std::env::temp_dir().join(format!("delegate-git-{}", Uuid::new_v4()));
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .context("creating a scratch file for vault git; check the temp folder")?;
    fs::remove_file(&path)?;
    Ok(file)
}

/// SIGKILL a whole process group; `kill` signals every member, and an empty group is no error.
fn kill_group(leader: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{leader}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Run git in its own process group with a hard limit. Output goes to scratch files, not
/// pipes, so a transport that outlives git (or ignores SIGTERM) can neither block the wait
/// nor survive it: the whole group gets SIGKILL when the limit runs out. A `network` call
/// also kills the group when git exits first, so nothing its transport started outlives it.
fn git_bounded(
    dir: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: &str,
    limit: Duration,
    network: bool,
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    // A default for transports; GIT_SSH_COMMAND in the environment still wins over it.
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.args([
            "-c",
            "core.sshCommand=ssh -oBatchMode=yes -oConnectTimeout=10",
        ]);
    }
    command
        .current_dir(dir)
        .process_group(0)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let mut stdin = Stdio::null();
    if !input.is_empty() {
        let mut file = scratch()?;
        file.write_all(input.as_bytes())?;
        file.seek(SeekFrom::Start(0))?;
        stdin = Stdio::from(file);
    }
    let (mut out, mut err) = (scratch()?, scratch()?);
    let mut child = command
        .args(args)
        .stdin(stdin)
        .stdout(out.try_clone()?)
        .stderr(err.try_clone()?)
        .spawn()
        .context("running git for the vault")?;
    let started = Instant::now();
    let mut pause = Duration::from_millis(1);
    let status = loop {
        if let Some(status) = child.try_wait().context("waiting for vault git")? {
            break Some(status);
        }
        if started.elapsed() >= limit {
            break None;
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(20));
    };
    let Some(status) = status else {
        // A SIGTERM-proof transport still dies to SIGKILL.
        kill_group(child.id());
        let _ = child.kill();
        let _ = child.wait();
        bail!(
            "git {} timed out after {}s; check the vault remote and credentials",
            args.join(" "),
            limit.as_secs().max(1)
        );
    };
    if network {
        kill_group(child.id());
    }
    let read = |file: &mut File| -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    if !status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&read(&mut err)?).trim()
        );
    }
    read(&mut out)
}

/// The scenario ids ("S901") that a list of vault paths touches, sorted, comma-separated.
fn scenario_ids<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let ids: std::collections::BTreeSet<_> = paths
        .filter_map(|path| path.split_once("/hidden/scenarios/"))
        .filter_map(|(_, path)| path.split('/').next())
        .map(|name| name.split('-').next().unwrap_or(name))
        .collect();
    ids.into_iter().collect::<Vec<_>>().join(", ")
}

fn project_prefix(name: &str) -> Option<&str> {
    let (id, suffix) = name.split_at_checked(36)?;
    (suffix.starts_with('-') && Uuid::parse_str(id).is_ok()).then_some(id)
}

/// Held by a sync point (`setup --vault`, `hidden add`, `where --pull`) for its whole sync,
/// network included, so two syncs never interleave (one switching the remote under the other).
/// Take it before `vault.lock`; ordinary commands take neither.
#[derive(Debug)]
pub(crate) struct SyncLock {
    _held: File,
}

impl SyncLock {
    /// Wait up to one sync's network limit for the sync in flight; `None` if it's still going.
    pub(crate) fn take(data: &Path) -> Result<Option<Self>> {
        Self::take_within(data, GIT_LIMIT)
    }

    fn take_within(data: &Path, wait: Duration) -> Result<Option<Self>> {
        fs::create_dir_all(data).context("creating delegate data folder; check its permissions")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data.join("vault-sync.lock"))?;
        let deadline = Instant::now() + wait;
        loop {
            match lock.try_lock() {
                Ok(()) => return Ok(Some(Self { _held: lock })),
                Err(fs::TryLockError::WouldBlock) => {}
                Err(fs::TryLockError::Error(e)) => {
                    return Err(e).context("locking the vault sync; retry the command");
                }
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Said when a sync point finds another sync still running after one sync's network limit.
const SYNC_BUSY: &str = "another vault sync is still running";

#[derive(Debug)]
pub(crate) struct Vault {
    pub(crate) root: PathBuf,
    data: PathBuf,
    lock: Option<File>,
}

impl Vault {
    /// Take `vault.lock` for local steps. Release it (`unlocked`) around every network call.
    pub(crate) fn lock(data: &Path) -> Result<Self> {
        let mut vault = Self::at(data);
        vault.relock()?;
        Ok(vault)
    }

    /// The vault without its lock, for read-only looks that must never wait.
    pub(crate) fn at(data: &Path) -> Self {
        Self {
            root: data.join("vault"),
            data: data.to_owned(),
            lock: None,
        }
    }

    /// Take the lock only if nobody holds it; `false` means another process does.
    pub(crate) fn try_lock(&mut self) -> Result<bool> {
        if self.lock.is_some() {
            return Ok(true);
        }
        fs::create_dir_all(&self.data)
            .context("creating delegate data folder; check its permissions")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data.join("vault.lock"))?;
        match lock.try_lock() {
            Ok(()) => {
                self.lock = Some(lock);
                Ok(true)
            }
            Err(fs::TryLockError::WouldBlock) => Ok(false),
            Err(fs::TryLockError::Error(e)) => Err(e).context("locking delegate vault"),
        }
    }

    fn relock(&mut self) -> Result<()> {
        if self.lock.is_some() {
            return Ok(());
        }
        fs::create_dir_all(&self.data)
            .context("creating delegate data folder; check its permissions")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data.join("vault.lock"))?;
        lock.lock()
            .context("locking delegate vault; retry the command")?;
        self.lock = Some(lock);
        Ok(())
    }

    /// Run a network step without holding the lock, then take it back for the local steps.
    fn unlocked<T>(&mut self, step: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.lock = None;
        let result = step(self);
        self.relock()?;
        result
    }

    pub(crate) fn init(&self) -> Result<()> {
        if !self.root.join(".git").is_dir() {
            fs::create_dir_all(&self.root)?;
            git(&self.root, &["init", "--initial-branch=main"])?;
            self.commit("Initialize delegate vault", true)?;
        }
        Ok(())
    }

    fn commit(&self, message: &str, empty: bool) -> Result<()> {
        if self.pending_merge()? {
            bail!("vault has a pending conflict; resolve it before committing");
        }
        let mut args = vec![
            "-c",
            "user.name=Delegate",
            "-c",
            "user.email=delegate@localhost",
            "commit",
            "-m",
            message,
        ];
        if empty {
            args.push("--allow-empty");
        }
        git(&self.root, &args)?;
        Ok(())
    }

    pub(crate) fn project(&self, _ctx: &Ctx, id: &str) -> Result<Option<PathBuf>> {
        self.find_project(id, true)
    }

    /// The project's folder; with `repair`, legacy duplicates are merged first (needs the lock).
    fn find_project(&self, id: &str, repair: bool) -> Result<Option<PathBuf>> {
        let mut matches = self.project_folders(id)?;
        if repair && matches.len() > 1 && self.conflict()?.is_none() {
            self.reconcile_project(&mut matches)?;
            self.commit_edits()?;
        }
        // The label belongs to the vault, not this machine's checkout. In
        // particular, a read-only lookup must never rename a shared folder.
        if let Some(folder) = matches.into_iter().next() {
            return Ok(Some(folder));
        }
        // Git does not track empty directories. After the last scenario was
        // deleted, a fresh clone still reuses the project's original label.
        Ok(self.project_history()?.lines().find_map(|path| {
            let (folder, _) = path.split_once('/')?;
            (project_prefix(folder) == Some(id)).then(|| self.root.join(folder))
        }))
    }

    fn project_folders(&self, id: &str) -> Result<Vec<PathBuf>> {
        let prefix = format!("{id}-");
        let mut matches = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry.file_type()?.is_dir()
            {
                matches.push(entry.path());
            }
        }
        if matches.len() < 2 {
            return Ok(matches);
        }
        // Git history survives clones; filesystem creation times do not. Walk
        // oldest ancestors first, even when several commits have the same date.
        // Labels break ties for folders first committed together or untracked.
        let history = self.project_history()?;
        matches.sort_by_cached_key(|folder| {
            let prefix = format!(
                "{}/",
                folder.file_name().unwrap_or_default().to_string_lossy()
            );
            let first = history.lines().position(|path| path.starts_with(&prefix));
            (first.unwrap_or(usize::MAX), folder.clone())
        });
        Ok(matches)
    }

    fn project_history(&self) -> Result<String> {
        git(
            &self.root,
            &[
                "-c",
                "core.quotepath=false",
                "log",
                "--reverse",
                "--topo-order",
                "--format=",
                "--name-only",
                "--diff-filter=A",
                "--no-renames",
            ],
        )
    }

    fn reconcile_project(&self, folders: &mut Vec<PathBuf>) -> Result<()> {
        let Some(dest) = folders.first() else {
            return Ok(());
        };
        if folders.len() < 2 {
            return Ok(());
        }
        // Preflight the entire union before moving anything. Identical copies
        // coalesce; differing copies are never overwritten or silently chosen.
        let mut files = std::collections::BTreeMap::new();
        let mut directories = std::collections::BTreeSet::new();
        for folder in folders.iter() {
            collect_project_files(folder, folder, &mut files, &mut directories)?;
        }
        for directory in &directories {
            if files.contains_key(directory) {
                bail!(
                    "vault: conflicting duplicate project path {}; reconcile the copies in {} and retry (all copies kept)",
                    directory.display(),
                    self.root.display()
                );
            }
        }
        for directory in directories {
            fs::create_dir_all(dest.join(directory))?;
        }
        for (relative, sources) in files {
            let target = dest.join(relative);
            // Move a retained copy into the oldest folder before removing any
            // identical duplicates. A failed move always leaves the source.
            for source in sources {
                if source != target {
                    if target.exists() {
                        fs::remove_file(source)?;
                    } else {
                        fs::rename(source, &target)?;
                    }
                }
            }
        }
        for folder in folders.iter().skip(1) {
            remove_empty_directories(folder)?;
        }
        eprintln!(
            "vault: merged duplicate project folders into {} (all scenarios kept)",
            dest.display()
        );
        folders.truncate(1);
        Ok(())
    }

    fn reconcile_projects(&self) -> Result<()> {
        if self.conflict()?.is_some() {
            return Ok(());
        }
        let mut ids = std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(id) = project_prefix(&entry.file_name().to_string_lossy())
            {
                ids.insert(id.to_owned());
            }
        }
        for id in ids {
            self.reconcile_project(&mut self.project_folders(&id)?)?;
        }
        self.commit_edits()
    }

    pub(crate) fn remote(&self) -> Result<Option<String>> {
        let remotes = git(&self.root, &["remote"])?;
        Ok(remotes
            .lines()
            .find(|r| *r == "origin")
            .or_else(|| remotes.lines().next())
            .map(str::to_owned))
    }

    pub(crate) fn ensure_no_conflict(&self) -> Result<()> {
        if self.root.join(".git").is_dir()
            && let Some(files) = self.conflict()?
        {
            bail!("{}", self.conflict_hint(&files));
        }
        Ok(())
    }

    fn pending_merge(&self) -> Result<bool> {
        Ok(self.root.join(".git/MERGE_HEAD").exists()
            || !git(&self.root, &["diff", "--name-only", "--diff-filter=U"])?.is_empty())
    }

    fn conflict(&self) -> Result<Option<String>> {
        if self.pending_merge()? {
            return Ok(Some(git(
                &self.root,
                &["diff", "--name-only", "--diff-filter=U"],
            )?));
        }
        let record = self.root.join(".git/delegate-conflict");
        if record.is_file()
            && git(
                &self.root,
                &[
                    "merge-base",
                    "--is-ancestor",
                    "refs/delegate/theirs",
                    "HEAD",
                ],
            )
            .is_err()
        {
            return Ok(Some(fs::read_to_string(record)?));
        }
        Ok(None)
    }

    fn conflict_hint(&self, files: &str) -> String {
        let path = self.root.display().to_string().replace('\'', "'\\''");
        format!(
            "vault: conflict in {}; resolve in '{}': git -C '{}' merge --no-ff refs/delegate/theirs; edit the conflicted files, then git -C '{}' add --all && git -C '{}' commit; rerun delegate list",
            files.replace(['\n', '\r'], ", "),
            path,
            path,
            path,
            path
        )
    }

    fn current_branch(&self) -> Result<String> {
        git(&self.root, &["symbolic-ref", "--short", "HEAD"])
    }

    /// The remote branch this one tracks; the local name when nothing is configured.
    fn branch(&self) -> Result<String> {
        let current = self.current_branch()?;
        if let Ok(merge) = git(
            &self.root,
            &["config", "--get", &format!("branch.{current}.merge")],
        ) && let Some(branch) = merge.strip_prefix("refs/heads/")
        {
            return Ok(branch.to_owned());
        }
        Ok(current)
    }

    fn unpushed(&self) -> Result<usize> {
        let base = self
            .remote()?
            .map(|remote| {
                self.branch()
                    .map(|branch| format!("refs/remotes/{remote}/{branch}"))
            })
            .transpose()?;
        let range = base
            .filter(|base| git(&self.root, &["rev-parse", "--verify", base]).is_ok())
            .map_or_else(|| "HEAD".to_owned(), |base| format!("{base}..HEAD"));
        Ok(git(&self.root, &["rev-list", "--count", &range])?.parse()?)
    }

    fn commit_edits(&self) -> Result<()> {
        if self.conflict()?.is_some() {
            return Ok(());
        }
        let changed = git(
            &self.root,
            &["diff", "--name-only", "--no-renames", "-z", "HEAD"],
        )?;
        let staged = git(
            &self.root,
            &["diff", "--cached", "--name-only", "--no-renames", "-z"],
        )?;
        let new = git(
            &self.root,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )?;
        if changed.is_empty() && staged.is_empty() && new.is_empty() {
            return Ok(());
        }
        let names = scenario_ids(
            changed
                .split('\0')
                .chain(staged.split('\0'))
                .chain(new.split('\0')),
        );
        git(&self.root, &["add", "--all", "--", "."])?;
        if git(&self.root, &["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(());
        }
        self.commit(
            &format!(
                "Update hidden scenarios: {}",
                if names.is_empty() {
                    "vault files"
                } else {
                    &names
                }
            ),
            false,
        )
    }

    /// Fetch the remote branch into a private ref when it has something we lack. Network only;
    /// the caller must not hold `vault.lock`. `remote` is the name `try_sync` read once.
    fn fetch_remote(&self, remote: &str, branch: &str) -> Result<Option<String>> {
        // Probe without writing FETCH_HEAD or tracking metadata. A health check
        // against an unchanged remote must not change any machine files.
        let tip = git_net(
            &self.root,
            &["ls-remote", remote, &format!("refs/heads/{branch}")],
        )?;
        // An empty remote (or a branch not published yet) still needs our push.
        if tip.is_empty() {
            return Ok(None);
        }
        if let Some((hash, _)) = tip.split_once('\t')
            && git(&self.root, &["merge-base", "--is-ancestor", hash, "HEAD"]).is_ok()
        {
            return Ok(None);
        }
        let fetched = format!("refs/delegate/fetched-{}", Uuid::new_v4());
        git_net(
            &self.root,
            &[
                "fetch",
                "--no-write-fetch-head",
                "--refmap=",
                remote,
                &format!("+refs/heads/{branch}:{fetched}"),
            ],
        )?;
        Ok(Some(fetched))
    }

    /// Move the remote-tracking ref to what `fetch_remote` brought in. The caller holds the lock.
    fn track_fetched(&self, remote: &str, branch: &str, fetched: &str) -> Result<()> {
        let tip = git(&self.root, &["rev-parse", fetched])?;
        git(
            &self.root,
            &[
                "update-ref",
                &format!("refs/remotes/{remote}/{branch}"),
                &tip,
            ],
        )?;
        Ok(())
    }

    /// Merge what `fetch_remote` brought in. Local steps only; the caller holds the lock.
    fn merge_fetched(&self, fetched: &str) -> Result<()> {
        let result = self.merge_ref(fetched);
        let _ = git(&self.root, &["update-ref", "-d", fetched]);
        result
    }

    fn merge_ref(&self, fetched: &str) -> Result<()> {
        if let Some(files) = self.conflict()? {
            bail!("{}", self.conflict_hint(&files));
        }
        // Do not create a merge commit for history already present locally.
        if git(
            &self.root,
            &["merge-base", "--is-ancestor", fetched, "HEAD"],
        )
        .is_ok()
        {
            return Ok(());
        }
        git(&self.root, &["update-ref", "refs/delegate/theirs", fetched])?;
        let theirs = "refs/delegate/theirs";
        // A machine that is only behind should catch up without making a new
        // merge commit that the other machines would then need to merge again.
        if git(&self.root, &["merge-base", "--is-ancestor", "HEAD", theirs]).is_ok() {
            git(&self.root, &["merge", "--ff-only", "refs/delegate/theirs"])?;
            return Ok(());
        }
        let merge = git(
            &self.root,
            &[
                "-c",
                "user.name=Delegate",
                "-c",
                "user.email=delegate@localhost",
                "-c",
                "merge.directoryRenames=true",
                "merge",
                "--no-edit",
                "--no-commit",
                "--no-ff",
                "--allow-unrelated-histories",
                "refs/delegate/theirs",
            ],
        );
        if let Err(error) = merge {
            let files = git(&self.root, &["diff", "--name-only", "--diff-filter=U"])?;
            if !files.is_empty() {
                fs::write(self.root.join(".git/delegate-conflict"), &files)?;
            }
            if self.root.join(".git/MERGE_HEAD").exists() {
                git(&self.root, &["merge", "--abort"])?;
            }
            if !files.is_empty() {
                bail!("{}", self.conflict_hint(&files));
            }
            return Err(error);
        }
        if self.root.join(".git/MERGE_HEAD").exists() {
            // This is our clean merge, not a user's pending merge.
            git(
                &self.root,
                &[
                    "-c",
                    "user.name=Delegate",
                    "-c",
                    "user.email=delegate@localhost",
                    "commit",
                    "-m",
                    "Merge vault remote",
                ],
            )?;
        }
        Ok(())
    }

    /// Fetch and merge the remote branch; the lock is held only for the local steps.
    fn merge_remote(&mut self, remote: &str, branch: &str) -> Result<()> {
        if let Some(files) = self.conflict()? {
            bail!("{}", self.conflict_hint(&files));
        }
        let Some(fetched) = self.unlocked(|vault| vault.fetch_remote(remote, branch))? else {
            return Ok(());
        };
        // Hand edits made while the network answered are committed first, never merged over.
        let committed = self.commit_edits();
        committed
            .and_then(|()| self.track_fetched(remote, branch, &fetched))
            .and_then(|()| self.merge_fetched(&fetched))
            .inspect_err(|_| {
                let _ = git(&self.root, &["update-ref", "-d", &fetched]);
            })
    }

    /// Push the current commit without holding `vault.lock`. Git updates the remote-tracking
    /// ref itself; nothing else in the vault's config changes.
    fn push_to(&mut self, remote: &str, branch: &str) -> Result<()> {
        let pushed = git(&self.root, &["rev-parse", "HEAD"])?;
        self.unlocked(|vault| vault.push_commit(remote, &pushed, branch))
    }

    /// Network only; the caller must not hold the lock.
    fn push_commit(&self, remote: &str, pushed: &str, branch: &str) -> Result<()> {
        git_net(
            &self.root,
            &["push", remote, &format!("{pushed}:refs/heads/{branch}")],
        )
        .map(drop)
    }

    /// Commit hand edits, then (when `pull`) merge the remote, then push. A user-asked sync:
    /// failures are reported on stderr and the local copy carries on. The lock is held on
    /// entry and on exit, and never while the network answers. The `SyncLock` proves no other
    /// sync (and so no remote switch) runs meanwhile.
    pub(crate) fn sync(&mut self, pull: bool, _sync: &SyncLock) {
        if let Err(e) = self.try_sync(pull) {
            let reason = e.to_string().replace(['\n', '\r'], " ");
            let count = self.unpushed().unwrap_or(0);
            if count > 0 {
                eprintln!("vault: {count} commit(s) not pushed: {reason}");
            } else {
                eprintln!("vault: couldn't check it's the latest; using local copy: {reason}");
            }
        }
    }

    fn try_sync(&mut self, pull: bool) -> Result<()> {
        if let Some(files) = self.conflict()? {
            eprintln!("{}", self.conflict_hint(&files));
            return Ok(());
        }
        self.commit_edits()?;
        // Legacy duplicates can be repaired locally even while offline.
        self.reconcile_projects()?;
        // Read once: the sync lock keeps `setup --vault` from changing them mid-sync.
        let Some(remote) = self.remote()? else {
            return Ok(());
        };
        let branch = self.branch()?;
        if pull {
            self.merge_remote(&remote, &branch)?;
            self.reconcile_projects()?;
        }
        if self.unpushed()? == 0 {
            return Ok(());
        }
        if let Err(error) = self.push_to(&remote, &branch) {
            // Offline/auth failures need no second network attempt.
            if !error.to_string().contains("[rejected]") {
                return Err(error);
            }
            self.merge_remote(&remote, &branch)?;
            self.reconcile_projects()?;
            self.push_to(&remote, &branch)?;
        }
        Ok(())
    }

    /// Keep evaluations on committed local content even while the user is in
    /// the middle of resolving a merge. Never expose conflict markers as checks.
    fn local_snapshot(&self) -> Result<PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        let head = git(&self.root, &["rev-parse", "HEAD"])?;
        let snapshot = self.root.join(".git/delegate-local").join(&head);
        let files = git(&self.root, &["ls-tree", "-rz", "HEAD"])?;
        for file in files.split('\0').filter(|s| !s.is_empty()) {
            let (metadata, path) = file.split_once('\t').context("invalid vault tree entry")?;
            let mut fields = metadata.split(' ');
            let mode = fields.next().context("missing vault file mode")?;
            let kind = fields.next().context("missing vault object type")?;
            let hash = fields.next().context("missing vault object id")?;
            if kind != "blob" || !matches!(mode, "100644" | "100755") {
                bail!(
                    "vault contains a non-regular file {path}; resolve it before running hidden scenarios"
                );
            }
            let dest = snapshot.join(path);
            fs::create_dir_all(dest.parent().context("missing vault snapshot parent")?)?;
            fs::write(
                &dest,
                git_bytes(&self.root, &["cat-file", "blob", hash], None, "")?,
            )?;
            fs::set_permissions(
                &dest,
                fs::Permissions::from_mode(if mode == "100755" { 0o755 } else { 0o644 }),
            )?;
        }
        Ok(snapshot)
    }

    /// What isn't on the remote yet, worked out from the vault alone: no network, no lock.
    /// A vault in the middle of a change (another process holds git's index lock) is skipped.
    fn hint(&self) -> Result<Option<String>> {
        if let Some(files) = self.conflict()? {
            return Ok(Some(self.conflict_hint(&files)));
        }
        if self.root.join(".git/index.lock").exists() {
            return Ok(None);
        }
        let mut notes = Vec::new();
        let edited = git_bytes(
            &self.root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            None,
            "",
        )?;
        let edited = String::from_utf8_lossy(&edited);
        // Entries are "XY path"; a rename carries a second, bare path.
        let paths = edited
            .split('\0')
            .filter(|e| !e.is_empty())
            .map(|e| e.get(3..).unwrap_or(e));
        let mut paths = paths.peekable();
        if paths.peek().is_some() {
            let ids = scenario_ids(paths);
            let what = if ids.is_empty() {
                "vault files".to_owned()
            } else {
                format!("hidden scenario(s) {ids}")
            };
            notes.push(format!(
                "vault: hand edits to {what} not pushed (not committed yet); the next evals/run pushes them"
            ));
        }
        let count = self.unpushed()?;
        if count > 0 {
            notes.push(format!("vault: {count} commit(s) not pushed"));
        }
        Ok((!notes.is_empty()).then(|| notes.join("\n")))
    }

    /// Report the remote's state. Reads and one bounded `ls-remote`; call without the lock.
    pub(crate) fn health(&self) -> Result<()> {
        self.report()
    }

    fn report(&self) -> Result<()> {
        println!("vault {}", self.root.display());
        match self.remote()? {
            Some(remote) => {
                let url = git(&self.root, &["remote", "get-url", &remote])?;
                println!("vault remote {url}");
                match git_net(&self.root, &["ls-remote", &remote]) {
                    Ok(_) => println!("vault remote reachable"),
                    Err(e) => println!(
                        "vault remote unreachable: {}; check the URL and credentials",
                        e.to_string().replace(['\n', '\r'], " ")
                    ),
                }
            }
            None => println!("vault remote: none; connect with delegate setup --vault <url>"),
        }
        if let Some(hint) = self.hint()? {
            println!("{hint}");
        }
        if !git(&self.root, &["status", "--porcelain"])?.is_empty() {
            println!("vault: local edits present (kept)");
        }
        Ok(())
    }
}

fn where_at(ctx: &Ctx, data: &Path) -> Result<()> {
    where_mode(ctx, data, false)
}

fn where_mode(ctx: &Ctx, data: &Path, pull: bool) -> Result<()> {
    let id = project_id(ctx)?;
    println!("tasks {}", ctx.dhome.display());
    if !data.join("vault/.git").is_dir() {
        println!(
            "hidden none: no vault on this machine; use delegate hidden add or delegate setup --vault <url>"
        );
        return Ok(());
    }
    // Only a sync point waits for a lock. Plain `where` repairs duplicates when the lock is
    // free and otherwise just reads.
    let mut vault = Vault::at(data);
    // Lock order: the sync lock first, then `vault.lock`.
    let sync = if pull { SyncLock::take(data)? } else { None };
    if pull && sync.is_none() {
        eprintln!("vault: couldn't check it's the latest; using local copy: {SYNC_BUSY}");
    }
    let locked = if sync.is_some() {
        vault.relock()?;
        true
    } else {
        vault.try_lock()?
    };
    if let Some(sync) = &sync {
        vault.sync(true, sync);
    } else if let Some(files) = vault.conflict()? {
        eprintln!("{}", vault.conflict_hint(&files));
    }
    if let Some(project) = vault.find_project(&id, locked)? {
        let project = if pull && vault.conflict()?.is_some() {
            vault
                .local_snapshot()?
                .join(project.strip_prefix(&vault.root)?)
        } else {
            project
        };
        let hidden = project.join("hidden");
        if hidden.is_dir() {
            println!("hidden {}", hidden.display());
            return Ok(());
        }
    }
    println!(
        "hidden none: project {id} is not in this vault; connect its remote or use delegate hidden add"
    );
    Ok(())
}

/// `where` inside the sandbox: the tasks folder only, nothing of the vault.
pub(crate) fn where_sandboxed(ctx: &Ctx) {
    println!("tasks {}", ctx.dhome.display());
    println!("hidden none: sandboxed");
}

pub(crate) fn where_paths(ctx: &Ctx) -> Result<()> {
    where_at(ctx, &data_dir())
}

/// A sync point (`where --pull`, what `evals/run` calls).
pub(crate) fn where_pulled(ctx: &Ctx) -> Result<()> {
    notices(ctx)?;
    where_mode(ctx, &data_dir(), true)
}

/// Inside a worker's sandbox delegate says nothing about the vault, and syncs nothing: the
/// vault is hidden from workers and its notices name hidden scenarios. Checked once, where
/// each command starts (`main::dispatch`).
pub(crate) fn sandboxed() -> bool {
    // Unit tests may themselves run inside a task's sandbox; only the command bridge (a child
    // test process started for one command) honours the variable.
    if cfg!(test) && std::env::var_os("DELEGATE_VAULT_TEST_COMMAND").is_none() {
        return false;
    }
    std::env::var_os("DELEGATE_SANDBOXED").is_some_and(|v| v == "1")
}

/// What isn't on the vault's remote yet. Local reads only: never the network, never the lock.
pub(crate) fn hint() -> Result<Option<String>> {
    let data = data_dir();
    if !data.join("vault/.git").is_dir() {
        return Ok(None);
    }
    Vault::at(&data).hint()
}

/// Say so when the vault holds an unresolved conflict (decision 010). Local reads only: no
/// lock, no network. Commands that report the vault themselves don't call it.
pub(crate) fn conflict_notice() -> Result<()> {
    if let Some(note) = conflict_note(&data_dir())? {
        eprintln!("{note}");
    }
    Ok(())
}

fn conflict_note(data: &Path) -> Result<Option<String>> {
    if !data.join("vault/.git").is_dir() {
        return Ok(None);
    }
    let vault = Vault::at(data);
    Ok(vault.conflict()?.map(|files| vault.conflict_hint(&files)))
}

pub(crate) fn machine_not_setup() -> Result<bool> {
    if crate::config::user_config_path().is_file() {
        return Ok(false);
    }
    let data = data_dir();
    Ok(!data.join("vault/.git").is_dir() || Vault::at(&data).remote()?.is_none())
}

pub(crate) fn notices(ctx: &Ctx) -> Result<()> {
    if machine_not_setup()? {
        eprintln!("this machine isn't set up: run delegate setup --vault <url>");
    }
    if let Some(id) = read_project_id(ctx)? {
        let data = data_dir();
        let known = if data.join("vault/.git").is_dir() {
            let prefix = format!("{id}-");
            fs::read_dir(data.join("vault"))?
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        } else {
            false
        };
        if !known {
            eprintln!("hidden scenarios for this project aren't on this machine");
        }
    }
    Ok(())
}

type ProjectFiles = std::collections::BTreeMap<PathBuf, Vec<PathBuf>>;

/// Build a lossless union of legacy duplicate folders before changing the tree.
fn collect_project_files(
    root: &Path,
    folder: &Path,
    files: &mut ProjectFiles,
    directories: &mut std::collections::BTreeSet<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?.to_owned();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            directories.insert(relative);
            collect_project_files(root, &path, files, directories)?;
        } else if kind.is_file() {
            let sources = files.entry(relative.clone()).or_default();
            if let Some(previous) = sources.first() {
                use std::os::unix::fs::PermissionsExt;
                if fs::read(previous)? != fs::read(&path)?
                    || fs::metadata(previous)?.permissions().mode()
                        != entry.metadata()?.permissions().mode()
                {
                    bail!(
                        "vault: conflicting duplicate project file {}; reconcile '{}' and '{}' and retry (both copies kept)",
                        relative.display(),
                        previous.display(),
                        path.display()
                    );
                }
            }
            sources.push(path);
        } else {
            bail!(
                "{} is not a regular file or directory; remove the link and retry",
                path.display()
            );
        }
    }
    Ok(())
}

fn remove_empty_directories(folder: &Path) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        remove_empty_directories(&entry?.path())?;
    }
    // Deliberately not remove_dir_all: never delete remaining scenario files.
    fs::remove_dir(folder)?;
    Ok(())
}

fn copy_folder(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if kind.is_dir() {
            copy_folder(&entry.path(), &to)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to)?;
        } else {
            bail!(
                "{} is not a regular file or directory; remove the link and retry",
                entry.path().display()
            );
        }
    }
    fs::set_permissions(dest, fs::metadata(src)?.permissions())?;
    Ok(())
}

fn add_at(ctx: &Ctx, folder: &Path, data: &Path, push: bool) -> Result<()> {
    let source = ctx
        .root
        .join(folder)
        .canonicalize()
        .context("scenario folder not found; pass a folder inside this project")?;
    let root = ctx.root.canonicalize()?;
    if !source.starts_with(&root)
        || source == root
        || source.starts_with(root.join(".git"))
        || !source.join("scenario.md").is_file()
    {
        bail!("pass a scenario folder inside this project containing scenario.md");
    }
    let sync = if push { SyncLock::take(data)? } else { None };
    let mut vault = Vault::lock(data)?;
    let id = project_id(ctx)?;
    vault.init()?;
    if let Some(files) = vault.conflict()? {
        bail!("{}", vault.conflict_hint(&files));
    }
    let project = vault.project(ctx, &id)?.unwrap_or_else(|| {
        let main = ctx.gitdir.parent().unwrap_or(&ctx.root);
        vault.root.join(format!(
            "{id}-{}",
            main.file_name().unwrap_or_default().to_string_lossy()
        ))
    });
    let scenarios = project.join("hidden/scenarios");
    fs::create_dir_all(&scenarios)?;
    let dest = scenarios.join(source.file_name().context("scenario folder needs a name")?);
    if dest.exists() {
        bail!(
            "{} already exists; no scenario was replaced",
            dest.display()
        );
    }
    // Copy first: even a failed commit or a cross-filesystem move must not lose the source.
    copy_folder(&source, &dest).context("copying scenario to the vault; source was kept")?;
    git(&vault.root, &["add", "--all", "--force", "--", "."])?;
    vault.commit(
        &format!(
            "Hide {} for {id}",
            source.file_name().unwrap_or_default().to_string_lossy()
        ),
        false,
    )?;
    fs::remove_dir_all(&source).context("scenario committed in the vault, but source could not be removed; remove the source copy manually")?;
    println!("hidden {}", dest.display());
    if let Some(sync) = &sync {
        vault.sync(false, sync);
    } else if push {
        let count = vault.unpushed().unwrap_or(0);
        eprintln!(
            "vault: {count} commit(s) not pushed: {SYNC_BUSY}; the next evals/run pushes them"
        );
    }
    Ok(())
}

/// A sync point: commit and push. Inside the sandbox it only commits.
pub(crate) fn add(ctx: &Ctx, folder: &Path) -> Result<()> {
    add_at(ctx, folder, &data_dir(), !sandboxed())
}

/// Whether git failed because it needed credentials and could not prompt for them.
fn needs_credentials(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "terminal prompts disabled",
        "could not read username",
        "could not read password",
    ]
    .iter()
    .any(|phrase| stderr.contains(phrase))
}

/// What to do when an HTTPS vault needs credentials git could not ask for.
fn credentials_hint(url: &str) -> String {
    format!(
        "git has no saved credentials for {url}; use the SSH URL instead, or save credentials with a \
         credential helper (`git config --global credential.helper <helper>`) and run \
         `git ls-remote {url}` once in a terminal; then rerun delegate setup --vault {url}"
    )
}

/// Probe before changing local state, so an unreachable URL is never connected.
fn connect_at(data: &Path, url: &str) -> Result<()> {
    let Some(sync) = SyncLock::take(data)? else {
        bail!("{SYNC_BUSY}; retry delegate setup --vault <url> in a minute");
    };
    let refs = match git_net(&std::env::current_dir()?, &["ls-remote", "--symref", url]) {
        Ok(refs) => refs,
        Err(e) if needs_credentials(&format!("{e:#}")) => bail!("{}", credentials_hint(url)),
        Err(e) => {
            return Err(e).context(
                "cannot reach vault URL; check the URL and credentials, then rerun delegate setup --vault <url>",
            )
        }
    };
    let branch = refs
        .lines()
        .find_map(|l| {
            l.strip_prefix("ref: refs/heads/")
                .and_then(|r| r.strip_suffix("\tHEAD"))
        })
        .or_else(|| {
            refs.lines()
                .find_map(|l| l.split_once("\trefs/heads/").map(|(_, b)| b))
        })
        .unwrap_or("main");
    let mut vault = Vault::lock(data)?;
    vault.init()?;
    if let Some(files) = vault.conflict()? {
        bail!("{}", vault.conflict_hint(&files));
    }
    vault.commit_edits()?;
    vault.track(url, branch)?;
    vault.sync(true, &sync);
    // A merge that stopped on a conflict is the user's to resolve; say so as an error.
    if let Some(files) = vault.conflict()? {
        bail!("{}", vault.conflict_hint(&files));
    }
    vault.lock = None;
    vault.health()
}

impl Vault {
    /// Make `origin` the vault's remote and the local branch track the remote's `branch`
    /// (what `push --set-upstream` would do). Another remote's tracking refs are dropped.
    fn track(&self, url: &str, branch: &str) -> Result<()> {
        let known = git(&self.root, &["remote"])?.lines().any(|r| r == "origin");
        if known {
            if git(&self.root, &["remote", "get-url", "origin"])? != url {
                git(&self.root, &["remote", "set-url", "origin", url])?;
                let stale = git(
                    &self.root,
                    &["for-each-ref", "--format=%(refname)", "refs/remotes/origin"],
                )?;
                for name in stale.lines() {
                    git(&self.root, &["update-ref", "-d", name])?;
                }
            }
        } else {
            git(&self.root, &["remote", "add", "origin", url])?;
        }
        let current = self.current_branch()?;
        git(
            &self.root,
            &["config", &format!("branch.{current}.remote"), "origin"],
        )?;
        git(
            &self.root,
            &[
                "config",
                &format!("branch.{current}.merge"),
                &format!("refs/heads/{branch}"),
            ],
        )?;
        Ok(())
    }
}

/// A sync point: connect and pull.
pub(crate) fn connect(url: &str) -> Result<()> {
    connect_at(&data_dir(), url)
}

pub(crate) fn health() -> Result<()> {
    let data = data_dir();
    if !data.join("vault/.git").is_dir() {
        println!("vault: not created; use delegate hidden add or delegate setup --vault <url>");
        return Ok(());
    }
    Vault::at(&data).health()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Barrier};

    /// What every sync point does with the vault: commit hand edits, pull, push.
    fn sync_at(data: &Path) -> Result<()> {
        let sync = SyncLock::take(data)?.unwrap();
        let mut vault = Vault::lock(data)?;
        vault.commit_edits()?;
        vault.sync(true, &sync);
        Ok(())
    }

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("delegate-vault-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn ctx(&self) -> Ctx {
            let root = self.0.join("shop");
            fs::create_dir_all(&root).unwrap();
            git(&root, &["init", "--initial-branch=main"]).unwrap();
            Ctx {
                gitdir: root.join(".git"),
                root,
                dhome: self.0.join("shop.delegate"),
            }
        }

        fn scenario(ctx: &Ctx, name: &str) -> PathBuf {
            let folder = ctx.root.join("evals/scenarios").join(name);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("scenario.md"), format!("# {name}\n")).unwrap();
            fs::write(folder.join("check"), "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(folder.join("check"), fs::Permissions::from_mode(0o755)).unwrap();
            folder
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn where_only_creates_a_stable_project_id_and_preserves_legacy_record() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let path = ctx.root.join(RECORD);
        let legacy = "# keep me\nkit = \"legacy\"\n[files]\n\"evals/run\" = \"abc\"\n";
        fs::write(&path, legacy).unwrap();
        let data = fixture.0.join("data");
        where_at(&ctx, &data).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        assert!(first.ends_with(legacy));
        assert_eq!(
            Uuid::parse_str(&project_id(&ctx).unwrap())
                .unwrap()
                .get_version_num(),
            4
        );
        where_at(&ctx, &data).unwrap();
        assert_eq!(first, fs::read_to_string(&path).unwrap());
        assert!(!data.exists());
        assert!(!ctx.dhome.exists());
        assert!(!ctx.gitdir.join("delegate-project.lock").exists());
    }

    #[test]
    fn concurrent_hidden_adds_both_land_and_use_one_project_id() {
        let fixture = Fixture::new();
        let ctx = Arc::new(fixture.ctx());
        let data = fixture.0.join("data");
        let barrier = Arc::new(Barrier::new(2));
        let mut threads = Vec::new();
        for name in ["S901-one", "S902-two"] {
            let folder = Fixture::scenario(&ctx, name);
            let (ctx, data, barrier) = (ctx.clone(), data.clone(), barrier.clone());
            threads.push(thread::spawn(move || {
                barrier.wait();
                add_at(&ctx, &folder, &data, true).unwrap();
                assert!(!folder.exists());
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        let vault = Vault::lock(&data).unwrap();
        let project = vault
            .project(&ctx, &project_id(&ctx).unwrap())
            .unwrap()
            .unwrap();
        for name in ["S901-one", "S902-two"] {
            let check = project.join("hidden/scenarios").join(name).join("check");
            assert!(check.is_file());
            assert_eq!(
                fs::metadata(check).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(
            git(&vault.root, &["rev-list", "--count", "HEAD"]).unwrap(),
            "3"
        );
    }

    #[test]
    fn project_folder_keeps_its_original_name_and_never_overwrites_a_scenario() {
        let fixture = Fixture::new();
        let mut ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let folder = Fixture::scenario(&ctx, "S901-one");
        add_at(&ctx, &folder, &data, true).unwrap();
        let id = project_id(&ctx).unwrap();
        let renamed = fixture.0.join("new-name");
        fs::rename(&ctx.root, &renamed).unwrap();
        ctx.root = renamed;
        ctx.gitdir = ctx.root.join(".git");
        where_at(&ctx, &data).unwrap();
        let hidden = data.join(format!("vault/{id}-shop/hidden/scenarios/S901-one"));
        assert!(hidden.join("scenario.md").is_file());
        assert!(!data.join(format!("vault/{id}-new-name")).exists());
        let folder = Fixture::scenario(&ctx, "S901-one");
        fs::write(folder.join("scenario.md"), "different").unwrap();
        assert!(add_at(&ctx, &folder, &data, true).is_err());
        assert_eq!(
            fs::read_to_string(hidden.join("scenario.md")).unwrap(),
            "# S901-one\n"
        );
        assert!(folder.is_dir());
    }

    #[test]
    fn different_checkout_names_share_one_folder_for_add_edit_and_delete() {
        let fixture = Fixture::new();
        let a_ctx = fixture.ctx();
        let b_root = fixture.0.join("b-shop");
        fs::create_dir(&b_root).unwrap();
        git(&b_root, &["init", "--initial-branch=main"]).unwrap();
        let b_ctx = Ctx {
            gitdir: b_root.join(".git"),
            root: b_root,
            dhome: fixture.0.join("b-shop.delegate"),
        };
        let id = project_id(&a_ctx).unwrap();
        save_project_id(&b_ctx, &id).unwrap();
        let a = fixture.0.join("a");
        let b = fixture.0.join("b");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&a_ctx, &Fixture::scenario(&a_ctx, "S901-one"), &a, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        for (ctx, data) in [(&b_ctx, &b), (&a_ctx, &a)] {
            where_at(ctx, data).unwrap();
            let vault = Vault::lock(data).unwrap();
            let head = git(&vault.root, &["rev-parse", "HEAD"]).unwrap();
            let project = vault.project(ctx, &id).unwrap().unwrap();
            assert_eq!(project, vault.root.join(format!("{id}-shop")));
            assert_eq!(git(&vault.root, &["rev-parse", "HEAD"]).unwrap(), head);
            assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
        }
        add_at(&b_ctx, &Fixture::scenario(&b_ctx, "S902-two"), &b, true).unwrap();
        sync_at(&a).unwrap();
        let hidden = format!("{id}-shop/hidden/scenarios");
        // Edits and deletions in either direction must use precisely the same
        // paths, not manufacture machine-local normalization commits.
        for (data, other) in [(&b, &a), (&a, &b)] {
            let file = format!("{hidden}/S901-one/scenario.md");
            let content = format!("edited on {}\n", data.display());
            fs::write(data.join("vault").join(&file), &content).unwrap();
            sync_at(data).unwrap();
            sync_at(other).unwrap();
            assert_eq!(
                fs::read_to_string(other.join("vault").join(&file)).unwrap(),
                content
            );
        }
        fs::remove_dir_all(b.join("vault").join(format!("{hidden}/S902-two"))).unwrap();
        sync_at(&b).unwrap();
        sync_at(&a).unwrap();
        for data in [&a, &b] {
            let vault = Vault::lock(data).unwrap();
            assert_eq!(vault.project_folders(&id).unwrap().len(), 1);
            assert!(!vault.root.join(format!("{hidden}/S902-two")).exists());
            assert!(vault.conflict().unwrap().is_none());
            assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
            assert_eq!(
                git(&vault.root, &["rev-parse", "HEAD^{tree}"]).unwrap(),
                git(&remote, &["rev-parse", "main^{tree}"]).unwrap()
            );
        }
    }

    #[test]
    fn empty_project_keeps_its_label_after_a_fresh_vault_clone() {
        let fixture = Fixture::new();
        let mut ctx = fixture.ctx();
        let a = fixture.0.join("a");
        let b = fixture.0.join("b");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        fs::remove_dir_all(a.join(format!("vault/{id}-shop"))).unwrap();
        sync_at(&a).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        let renamed = fixture.0.join("b-shop");
        fs::rename(&ctx.root, &renamed).unwrap();
        ctx.root = renamed;
        ctx.gitdir = ctx.root.join(".git");
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &b, true).unwrap();
        sync_at(&a).unwrap();
        for data in [&a, &b] {
            let vault = Vault::lock(data).unwrap();
            assert_eq!(
                vault.project_folders(&id).unwrap(),
                vec![vault.root.join(format!("{id}-shop"))]
            );
            assert!(
                vault
                    .root
                    .join(format!("{id}-shop/hidden/scenarios/S902-two/check"))
                    .is_file()
            );
            assert!(
                !vault
                    .root
                    .join(format!("{id}-shop/hidden/scenarios/S901-one"))
                    .exists()
            );
            assert_eq!(
                git(&vault.root, &["rev-parse", "HEAD^{tree}"]).unwrap(),
                git(&remote, &["rev-parse", "main^{tree}"]).unwrap()
            );
        }
    }

    #[test]
    fn legacy_duplicate_folders_merge_into_oldest_once_and_sync() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let a = fixture.0.join("a");
        let b = fixture.0.join("b");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        let original = a.join(format!("vault/{id}-shop"));
        let duplicate = a.join(format!("vault/{id}-aaa"));
        // Reproduce an old vault containing multiple labels for this ID. The
        // older folder wins even if its name sorts later and commits share a date.
        copy_folder(&original, &duplicate).unwrap();
        let source = Fixture::scenario(&ctx, "S902-two");
        copy_folder(&source, &duplicate.join("hidden/scenarios/S902-two")).unwrap();
        {
            let vault = Vault::lock(&a).unwrap();
            vault.commit_edits().unwrap();
        }
        sync_at(&a).unwrap();
        sync_at(&b).unwrap();
        for data in [&a, &b] {
            let vault = Vault::lock(data).unwrap();
            let folders = vault.project_folders(&id).unwrap();
            assert_eq!(folders, vec![vault.root.join(format!("{id}-shop"))]);
            for name in ["S901-one", "S902-two"] {
                let scenario = folders[0].join("hidden/scenarios").join(name);
                assert_eq!(
                    fs::read_to_string(scenario.join("scenario.md")).unwrap(),
                    format!("# {name}\n")
                );
                assert_eq!(
                    fs::metadata(scenario.join("check"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o755
                );
            }
            let head = git(&vault.root, &["rev-parse", "HEAD"]).unwrap();
            vault.reconcile_projects().unwrap();
            vault.project(&ctx, &id).unwrap();
            assert_eq!(git(&vault.root, &["rev-parse", "HEAD"]).unwrap(), head);
            assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
            assert_eq!(
                git(&vault.root, &["rev-parse", "HEAD^{tree}"]).unwrap(),
                git(&remote, &["rev-parse", "main^{tree}"]).unwrap()
            );
        }
    }

    #[test]
    fn duplicate_folder_reconciliation_never_overwrites_differing_copies() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, true).unwrap();
        let id = project_id(&ctx).unwrap();
        let vault = Vault::lock(&data).unwrap();
        let original = vault.root.join(format!("{id}-shop"));
        let duplicate = vault.root.join(format!("{id}-other"));
        copy_folder(&original, &duplicate).unwrap();
        let file = "hidden/scenarios/S901-one/scenario.md";
        fs::write(duplicate.join(file), "different\n").unwrap();
        vault.commit_edits().unwrap();
        let head = git(&vault.root, &["rev-parse", "HEAD"]).unwrap();
        for _ in 0..2 {
            let error = vault.reconcile_projects().unwrap_err();
            assert!(error.to_string().contains("both copies kept"), "{error}");
            assert_eq!(
                fs::read_to_string(original.join(file)).unwrap(),
                "# S901-one\n"
            );
            assert_eq!(
                fs::read_to_string(duplicate.join(file)).unwrap(),
                "different\n"
            );
            assert_eq!(git(&vault.root, &["rev-parse", "HEAD"]).unwrap(), head);
            assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
        }
    }

    #[test]
    fn setup_merges_two_local_vaults_and_pushes_to_an_empty_remote() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let a = fixture.0.join("machine-a");
        let b = fixture.0.join("machine-b");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a, true).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &b, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        for name in ["S901-one", "S902-two"] {
            assert!(
                b.join(format!(
                    "vault/{id}-shop/hidden/scenarios/{name}/scenario.md"
                ))
                .is_file()
            );
            assert!(
                git(
                    &remote,
                    &[
                        "show",
                        &format!("main:{id}-shop/hidden/scenarios/{name}/scenario.md")
                    ]
                )
                .is_ok()
            );
        }
        add_at(&ctx, &Fixture::scenario(&ctx, "S903-three"), &b, true).unwrap();
        assert!(
            git(
                &remote,
                &[
                    "show",
                    &format!("main:{id}-shop/hidden/scenarios/S903-three/scenario.md")
                ]
            )
            .is_ok()
        );
    }

    #[test]
    fn only_where_pull_syncs_other_machines_changes_and_converges_after_an_outage() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let a = fixture.0.join("machine-a/delegate");
        let b = fixture.0.join("machine-b/delegate");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();

        // Exercise real command dispatch. `where --pull` is the sync point; `list` is local.
        let run = |data: &Path, command: &str| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "vault::tests::command_bridge",
                    "--nocapture",
                    "--quiet",
                ])
                .current_dir(&ctx.root)
                .env("DELEGATE_VAULT_TEST_COMMAND", command)
                .env_remove("DELEGATE_SANDBOXED")
                .env("HOME", fixture.0.join("home"))
                .env("XDG_CONFIG_HOME", fixture.0.join("config"))
                .env("XDG_DATA_HOME", data.parent().unwrap())
                .env_remove("DELEGATE_HOME")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(
                !String::from_utf8_lossy(&output.stderr).contains("vault:"),
                "{output:?}"
            );
        };
        let list = |data: &Path| run(data, "list");
        let pull = |data: &Path| run(data, "where\u{1f}--pull");
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &a, true).unwrap();
        let id = project_id(&ctx).unwrap();
        let scenario = |data: &Path, name: &str| {
            data.join(format!(
                "vault/{id}-shop/hidden/scenarios/{name}/scenario.md"
            ))
        };
        assert!(!scenario(&b, "S902-two").exists());
        list(&b);
        assert!(!scenario(&b, "S902-two").exists(), "list must not sync");
        pull(&b);
        assert!(scenario(&b, "S902-two").is_file());

        let away = fixture.0.join("remote.away");
        fs::rename(&remote, &away).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S903-three"), &b, true).unwrap();
        fs::rename(&away, &remote).unwrap();
        // A is initially up to date with the remote but B has an offline commit.
        // One command on B publishes it; A's next command must receive it.
        pull(&a);
        pull(&b);
        pull(&a);
        for data in [&a, &b] {
            for name in ["S901-one", "S902-two", "S903-three"] {
                assert!(scenario(data, name).is_file());
            }
            assert_eq!(
                git(&data.join("vault"), &["rev-parse", "HEAD^{tree}"]).unwrap(),
                git(&remote, &["rev-parse", "main^{tree}"]).unwrap()
            );
        }
        // An unchanged remote must not create or push gratuitous merge commits.
        for data in [&a, &b] {
            let head = git(&data.join("vault"), &["rev-parse", "HEAD"]).unwrap();
            pull(data);
            assert_eq!(
                git(&data.join("vault"), &["rev-parse", "HEAD"]).unwrap(),
                head
            );
        }
    }

    #[test]
    fn command_sync_publishes_to_a_connected_empty_remote() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, true).unwrap();
        git(
            &data.join("vault"),
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .unwrap();
        sync_at(&data).unwrap();
        assert_eq!(
            git(&data.join("vault"), &["rev-parse", "HEAD"]).unwrap(),
            git(&remote, &["rev-parse", "main"]).unwrap()
        );
    }

    #[test]
    fn command_sync_propagates_hand_deleted_scenarios() {
        // Cover both a fast-forward and a merge with unrelated local edits.
        for diverged in [false, true] {
            let fixture = Fixture::new();
            let ctx = fixture.ctx();
            let a = fixture.0.join("machine-a");
            let b = fixture.0.join("machine-b");
            let remote = fixture.0.join("remote.git");
            fs::create_dir(&remote).unwrap();
            git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
            for name in ["S901-one", "S902-two"] {
                add_at(&ctx, &Fixture::scenario(&ctx, name), &a, true).unwrap();
            }
            connect_at(&a, remote.to_str().unwrap()).unwrap();
            connect_at(&b, remote.to_str().unwrap()).unwrap();
            let id = project_id(&ctx).unwrap();
            let scenario = format!("{id}-shop/hidden/scenarios/S901-one");
            let retained = format!("{id}-shop/hidden/scenarios/S902-two/scenario.md");
            // The user removes a whole scenario; delegate only records and syncs it.
            fs::remove_dir_all(b.join("vault").join(&scenario)).unwrap();
            sync_at(&b).unwrap();
            let message = git(&b.join("vault"), &["log", "-1", "--format=%s"]).unwrap();
            assert!(message.contains("S901"), "{message}");
            assert!(git(&remote, &["show", &format!("main:{scenario}/scenario.md")]).is_err());
            if diverged {
                fs::write(a.join("vault").join(&retained), "edited locally\n").unwrap();
            }
            sync_at(&a).unwrap();
            sync_at(&b).unwrap();
            for data in [&a, &b] {
                let vault = Vault::lock(data).unwrap();
                assert!(!vault.root.join(&scenario).exists());
                assert!(vault.root.join(&retained).is_file());
                if diverged {
                    assert_eq!(
                        fs::read_to_string(vault.root.join(&retained)).unwrap(),
                        "edited locally\n"
                    );
                }
                assert!(vault.conflict().unwrap().is_none());
                assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
                assert_eq!(
                    git(&vault.root, &["rev-parse", "HEAD^{tree}"]).unwrap(),
                    git(&remote, &["rev-parse", "main^{tree}"]).unwrap()
                );
            }
        }
    }

    #[test]
    fn hand_deletion_conflicts_with_another_machines_edit() {
        // Either the local or remote side may be the one that deleted the file.
        for local_deletes in [false, true] {
            let fixture = Fixture::new();
            let ctx = fixture.ctx();
            let a = fixture.0.join("machine-a");
            let b = fixture.0.join("machine-b");
            let remote = fixture.0.join("remote.git");
            fs::create_dir(&remote).unwrap();
            git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
            add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a, true).unwrap();
            connect_at(&a, remote.to_str().unwrap()).unwrap();
            connect_at(&b, remote.to_str().unwrap()).unwrap();
            let id = project_id(&ctx).unwrap();
            let scenario = format!("{id}-shop/hidden/scenarios/S901-one");
            let file = format!("{scenario}/scenario.md");
            let (deleted, edited) = if local_deletes { (&b, &a) } else { (&a, &b) };
            fs::remove_dir_all(deleted.join("vault").join(&scenario)).unwrap();
            fs::write(edited.join("vault").join(&file), "edited by hand\n").unwrap();
            sync_at(&a).unwrap();
            sync_at(&b).unwrap();
            let head = git(&b.join("vault"), &["rev-parse", "HEAD"]).unwrap();
            for _ in 0..2 {
                sync_at(&b).unwrap();
                let vault = Vault::lock(&b).unwrap();
                assert!(vault.conflict().unwrap().unwrap().contains(&file));
                assert!(
                    vault
                        .hint()
                        .unwrap()
                        .unwrap()
                        .contains("vault: conflict in")
                );
                assert!(!vault.pending_merge().unwrap());
                assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
                assert_eq!(git(&vault.root, &["rev-parse", "HEAD"]).unwrap(), head);
                let (deleted_ref, edited_ref) = if local_deletes {
                    ("HEAD", "refs/delegate/theirs")
                } else {
                    ("refs/delegate/theirs", "HEAD")
                };
                assert!(git(&vault.root, &["show", &format!("{deleted_ref}:{file}")]).is_err());
                assert_eq!(
                    git(&vault.root, &["show", &format!("{edited_ref}:{file}")]).unwrap(),
                    "edited by hand"
                );
            }
        }
    }

    #[test]
    fn concurrent_syncs_make_one_commit_naming_all_changed_scenarios() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        for name in ["S901-one", "S902-two"] {
            add_at(&ctx, &Fixture::scenario(&ctx, name), &data, true).unwrap();
        }
        let id = project_id(&ctx).unwrap();
        let hidden = data.join(format!("vault/{id}-shop/hidden/scenarios"));
        fs::write(hidden.join("S901-one/scenario.md"), "edited by hand\n").unwrap();
        fs::write(hidden.join("S902-two/new.txt"), "new\n").unwrap();
        git(&data.join("vault"), &["add", "--all"]).unwrap();
        fs::write(hidden.join("S902-two/unstaged.txt"), "also new\n").unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let (data, barrier) = (data.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    sync_at(&data).unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let vault = Vault::lock(&data).unwrap();
        assert_eq!(
            git(&vault.root, &["rev-list", "--count", "HEAD"]).unwrap(),
            "4"
        );
        let message = git(&vault.root, &["log", "-1", "--format=%s"]).unwrap();
        assert!(message.contains("S901") && message.contains("S902"));
        assert_eq!(git(&vault.root, &["status", "--porcelain"]).unwrap(), "");
        assert!(
            vault
                .hint()
                .unwrap()
                .unwrap()
                .contains("4 commit(s) not pushed")
        );
    }

    // The wrapper used by the runner test invokes real dispatch in a separate
    // process, so HOME, cwd and clap's help exits cannot affect parallel tests.
    #[test]
    fn command_bridge() {
        use clap::Parser;
        let Ok(command) = std::env::var("DELEGATE_VAULT_TEST_COMMAND") else {
            return;
        };
        let args =
            std::iter::once("delegate").chain(command.split('\u{1f}').filter(|s| !s.is_empty()));
        assert_eq!(
            crate::dispatch(crate::Cli::parse_from(args).cmd).unwrap(),
            std::process::ExitCode::SUCCESS
        );
    }

    #[test]
    fn evals_in_task_worktree_only_skips_vault_when_marked_sandboxed() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        fs::create_dir(ctx.root.join(".task")).unwrap();
        fs::create_dir_all(ctx.root.join("evals/scenarios")).unwrap();
        let bin = fixture.0.join("bin");
        fs::create_dir(&bin).unwrap();
        let hidden = fixture.0.join("vault/project/hidden");
        let scenario = hidden.join("scenarios/S901-private");
        fs::create_dir_all(&scenario).unwrap();
        fs::write(
            scenario.join("scenario.md"),
            "# S901 Private check\n- Tier: fast\n- Status: automated\n",
        )
        .unwrap();
        fs::write(
            scenario.join("check"),
            "#!/bin/sh\ntouch \"$EVALS_SCENARIO/ran\"\n",
        )
        .unwrap();
        fs::set_permissions(scenario.join("check"), fs::Permissions::from_mode(0o755)).unwrap();
        let calls = fixture.0.join("delegate-calls");
        let delegate = bin.join("delegate");
        fs::write(
            &delegate,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>\"$TEST_CALLS\"\ncase \"$*\" in\n  --help) printf '  where  Show folders\\n' ;;\n  'where --help') printf '%s\\n' --pull ;;\n  'where --pull') printf 'tasks %s\\nhidden %s\\n' \"$TEST_TASKS\" \"$TEST_HIDDEN\" ;;\n  *) exit 1 ;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&delegate, fs::Permissions::from_mode(0o755)).unwrap();
        let runner = ctx.root.join("evals/run");
        for source in [
            include_str!("../../evals/run"),
            include_str!("../templates/project/evals/run"),
        ] {
            fs::write(&runner, source).unwrap();
            for sandboxed in [false, true] {
                let mut command = Command::new("bash");
                command
                    .arg(&runner)
                    .arg("S901")
                    .current_dir(&ctx.root)
                    .env(
                        "PATH",
                        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
                    )
                    .env("DELEGATE_HOME", &ctx.dhome)
                    .env("EVALS_RESULTS", fixture.0.join("results"))
                    .env("TEST_CALLS", &calls)
                    .env("TEST_TASKS", &ctx.dhome)
                    .env("TEST_HIDDEN", &hidden)
                    .env_remove("EVALS_HIDDEN")
                    .env_remove("DELEGATE_SANDBOXED");
                if sandboxed {
                    // Even an explicit override must not expose the hidden set.
                    command
                        .env("DELEGATE_SANDBOXED", "1")
                        .env("EVALS_HIDDEN", &hidden);
                }
                let output = command.output().unwrap();
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    output.status.success(),
                    "{stdout}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if sandboxed {
                    assert!(!calls.exists(), "sandbox contacted delegate/vault");
                    assert!(!scenario.join("ran").exists());
                    assert!(stdout.contains("no hidden scenarios found"), "{stdout}");
                } else {
                    assert!(
                        fs::read_to_string(&calls)
                            .unwrap()
                            .contains("where --pull\n")
                    );
                    assert!(stdout.contains("S901, hidden"), "{stdout}");
                    assert!(stdout.contains("1 passed"), "{stdout}");
                    assert!(scenario.join("ran").exists());
                    fs::remove_file(scenario.join("ran")).unwrap();
                    fs::remove_file(&calls).unwrap();
                }
            }
        }
    }

    #[test]
    fn conflicts_survive_commands_and_evals_until_explicitly_resolved() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let a = fixture.0.join("a");
        let home = fixture.0.join("home");
        let b = home.join(".local/share/delegate");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        let source = Fixture::scenario(&ctx, "S901-one");
        fs::write(
            source.join("scenario.md"),
            "# S901 Hidden local\n- Tier: fast\n- Status: automated\n",
        )
        .unwrap();
        add_at(&ctx, &source, &a, true).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        let file = format!("{id}-shop/hidden/scenarios/S901-one/scenario.md");
        let original = fs::read_to_string(a.join("vault").join(&file)).unwrap();
        fs::write(
            a.join("vault").join(&file),
            original.replace("local", "from-A"),
        )
        .unwrap();
        fs::write(
            b.join("vault").join(&file),
            original.replace("local", "from-B"),
        )
        .unwrap();
        sync_at(&a).unwrap();
        sync_at(&b).unwrap();
        let head = git(&b.join("vault"), &["rev-parse", "HEAD"]).unwrap();
        for _ in 0..2 {
            sync_at(&b).unwrap();
            let sync = SyncLock::take(&b).unwrap().unwrap();
            let mut vault = Vault::lock(&b).unwrap();
            vault.sync(true, &sync);
            assert!(
                vault
                    .hint()
                    .unwrap()
                    .unwrap()
                    .contains("vault: conflict in")
            );
            assert!(
                vault
                    .hint()
                    .unwrap()
                    .unwrap()
                    .contains("refs/delegate/theirs")
            );
            assert!(!vault.pending_merge().unwrap());
            assert_eq!(git(&vault.root, &["rev-parse", "HEAD"]).unwrap(), head);
            assert!(
                fs::read_to_string(vault.root.join(&file))
                    .unwrap()
                    .contains("from-B")
            );
            assert!(
                git(
                    &vault.root,
                    &["show", &format!("refs/delegate/theirs:{file}")]
                )
                .unwrap()
                .contains("from-A")
            );
        }
        // Begin a manual resolution, but do not finish it. No command may commit
        // conflict markers, and evals must execute a snapshot of the local HEAD.
        assert!(
            git(
                &b.join("vault"),
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@local",
                    "merge",
                    "refs/delegate/theirs"
                ]
            )
            .is_err()
        );
        sync_at(&b).unwrap();
        assert_eq!(git(&b.join("vault"), &["rev-parse", "HEAD"]).unwrap(), head);
        let bin = fixture.0.join("bin");
        fs::create_dir(&bin).unwrap();
        let wrapper = bin.join("delegate");
        fs::write(&wrapper, format!("#!/usr/bin/env bash\nexport DELEGATE_VAULT_TEST_COMMAND=\"$(printf '%s\\037' \"$@\")\"\nexec '{}' --exact vault::tests::command_bridge --nocapture --quiet\n", std::env::current_exe().unwrap().display())).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
        fs::create_dir_all(home.join(".config/delegate")).unwrap();
        fs::write(home.join(".config/delegate/workers.toml"), "").unwrap();
        fs::create_dir_all(ctx.root.join("evals/scenarios")).unwrap();
        fs::create_dir(ctx.root.join(".task")).unwrap();
        let runner = ctx.root.join("evals/run");
        fs::write(&runner, include_str!("../templates/project/evals/run")).unwrap();
        let output = Command::new("bash")
            .arg(&runner)
            .arg("fast")
            .current_dir(&ctx.root)
            .env("HOME", &home)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("EVALS_RESULTS", fixture.0.join("results"))
            .env_remove("DELEGATE_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("EVALS_HIDDEN")
            .env_remove("DELEGATE_SANDBOXED")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        assert!(
            stdout.contains("Hidden from-B") && stdout.contains("S901, hidden"),
            "{stdout}"
        );
        assert!(stderr.contains("conflict"), "{stderr}");
        assert_eq!(git(&b.join("vault"), &["rev-parse", "HEAD"]).unwrap(), head);
        // The user explicitly resolves and commits; only then does sync resume.
        fs::write(
            b.join("vault").join(&file),
            original.replace("local", "resolved"),
        )
        .unwrap();
        git(&b.join("vault"), &["add", "--all"]).unwrap();
        git(
            &b.join("vault"),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@local",
                "commit",
                "-m",
                "Resolve explicitly",
            ],
        )
        .unwrap();
        sync_at(&b).unwrap();
        let vault = Vault::lock(&b).unwrap();
        assert!(vault.hint().unwrap().is_none());
    }

    /// A transport script that ignores SIGTERM, leaves a child holding the pipes, and records
    /// its pids in `pids`.
    fn stubborn_transport(fixture: &Fixture) -> (PathBuf, PathBuf) {
        let pids = fixture.0.join("transport-pids");
        let script = fixture.0.join("stubborn-ssh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ntrap '' TERM\necho $$ >>'{pids}'\n(trap '' TERM; echo $$ >>'{pids}'; sleep 60) &\nsleep 60\n",
                pids = pids.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        (script, pids)
    }

    fn alive(pid: &str) -> bool {
        // A zombie still has a /proc entry; only a live process counts.
        fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|stat| !stat.rsplit(") ").next().unwrap_or("").starts_with('Z'))
    }

    #[test]
    fn a_transport_that_ignores_sigterm_is_killed_by_the_limit() {
        let fixture = Fixture::new();
        let (script, pids) = stubborn_transport(&fixture);
        let started = Instant::now();
        let error = git_bounded(
            &fixture.0,
            &[
                "-c",
                &format!("core.sshCommand={}", script.display()),
                "ls-remote",
                "ssh://vault.invalid/repo.git",
            ],
            None,
            "",
            Duration::from_secs(1),
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out after 1s"), "{error}");
        // The call returned at the limit, without waiting for the leftover child's pipes.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        let pids = fs::read_to_string(pids).unwrap();
        assert_eq!(pids.lines().count(), 2, "{pids}");
        for _ in 0..50 {
            if !pids.lines().any(alive) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("transport processes survived the limit: {pids}");
    }

    #[test]
    fn a_transport_child_left_behind_after_git_exits_is_killed() {
        let fixture = Fixture::new();
        let pids = fixture.0.join("transport-pids");
        let script = fixture.0.join("leaving-ssh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ntrap '' TERM\nsleep 60 </dev/null >/dev/null 2>&1 &\necho $! >>'{}'\nexit 1\n",
                pids.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        let error = git_net(
            &fixture.0,
            &[
                "-c",
                &format!("core.sshCommand={}", script.display()),
                "ls-remote",
                "ssh://vault.invalid/repo.git",
            ],
        )
        .unwrap_err();
        assert!(!error.to_string().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pids = fs::read_to_string(pids).unwrap();
        assert!(pids.lines().count() >= 1, "{pids}");
        for _ in 0..50 {
            if !pids.lines().any(alive) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("a transport child outlived its network call: {pids}");
    }

    #[test]
    fn list_note_answers_while_another_process_holds_the_vault_lock() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let held = Vault::lock(&data).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = data.clone();
        thread::spawn(move || {
            // Vault::at is what `list`, `machine_not_setup` and the notices use.
            let vault = Vault::at(&reader);
            let _ = sender.send((vault.hint().unwrap(), vault.remote().unwrap()));
        });
        let (note, remote) = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the vault note waited on vault.lock");
        assert!(remote.is_none());
        assert!(note.unwrap().contains("commit(s) not pushed"));
        drop(held);
    }

    #[test]
    fn list_note_names_a_hand_edit_without_touching_the_network() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, true).unwrap();
        connect_at(&data, remote.to_str().unwrap()).unwrap();
        assert_eq!(Vault::at(&data).hint().unwrap(), None);
        let id = project_id(&ctx).unwrap();
        let file = data.join(format!(
            "vault/{id}-shop/hidden/scenarios/S901-one/scenario.md"
        ));
        fs::write(&file, "edited by hand\n").unwrap();
        // A transport that would leave a trace if it were ever started.
        let (script, pids) = stubborn_transport(&fixture);
        git(
            &data.join("vault"),
            &["config", "core.sshCommand", &script.display().to_string()],
        )
        .unwrap();
        git(
            &data.join("vault"),
            &[
                "remote",
                "set-url",
                "origin",
                "ssh://vault.invalid/repo.git",
            ],
        )
        .unwrap();
        let note = Vault::at(&data).hint().unwrap().unwrap();
        assert!(
            note.contains("S901") && note.contains("not pushed"),
            "{note}"
        );
        assert!(!pids.exists(), "the note started a transport");
        // A committed but unpushed edit is counted instead.
        git(&data.join("vault"), &["add", "--all"]).unwrap();
        git(
            &data.join("vault"),
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit",
                "-m",
                "edit",
            ],
        )
        .unwrap();
        let note = Vault::at(&data).hint().unwrap().unwrap();
        assert!(note.contains("1 commit(s) not pushed"), "{note}");
        // While another process is changing the vault, the note is skipped.
        fs::write(data.join("vault/.git/index.lock"), "").unwrap();
        assert_eq!(Vault::at(&data).hint().unwrap(), None);
        assert!(!pids.exists(), "the note started a transport");
    }

    /// Run `step` on another thread and fail, rather than hang, if it waits on the lock.
    fn answers_while_locked<T: Send + 'static>(
        data: &Path,
        step: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let held = Vault::lock(data).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(step());
        });
        let answer = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the command waited on vault.lock");
        drop(held);
        answer
    }

    #[test]
    fn plain_where_answers_while_another_process_holds_the_vault_lock() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let reader = data.clone();
        answers_while_locked(&data, move || where_at(&ctx, &reader).unwrap());
    }

    #[test]
    fn the_conflict_note_is_read_without_the_lock_or_the_network() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let vault = data.join("vault");
        assert_eq!(conflict_note(&data).unwrap(), None);
        // What a merge that stopped on a conflict leaves behind.
        let tree = git(&vault, &["hash-object", "-t", "tree", "/dev/null"]).unwrap();
        let theirs = git(
            &vault,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit-tree",
                "-m",
                "theirs",
                &tree,
            ],
        )
        .unwrap();
        git(&vault, &["update-ref", "refs/delegate/theirs", &theirs]).unwrap();
        fs::write(vault.join(".git/delegate-conflict"), "a/file\n").unwrap();
        let reader = data.clone();
        let note = answers_while_locked(&data, move || conflict_note(&reader).unwrap());
        let note = note.unwrap();
        assert!(note.contains("vault: conflict in a/file"), "{note}");
        assert!(note.contains("refs/delegate/theirs"), "{note}");
    }

    /// Two bare remotes (`old`, `new`) with `new`'s default branch called `new_branch`, and a
    /// vault connected to `old` with one scenario pushed.
    fn two_remotes(fixture: &Fixture, new_branch: &str) -> (PathBuf, PathBuf, PathBuf) {
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let (old, new) = (fixture.0.join("old.git"), fixture.0.join("new.git"));
        for (remote, branch) in [(&old, "main"), (&new, new_branch)] {
            fs::create_dir(remote).unwrap();
            git(
                remote,
                &["init", "--bare", &format!("--initial-branch={branch}")],
            )
            .unwrap();
        }
        let seed = fixture.0.join("seed");
        fs::create_dir(&seed).unwrap();
        git(&seed, &["init", "--initial-branch", new_branch]).unwrap();
        fs::write(seed.join("README"), "new remote\n").unwrap();
        git(&seed, &["add", "--all"]).unwrap();
        git(
            &seed,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit",
                "-qm",
                "seed",
            ],
        )
        .unwrap();
        git(&seed, &["push", "-q", new.to_str().unwrap(), new_branch]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        connect_at(&data, old.to_str().unwrap()).unwrap();
        (data, old, new)
    }

    #[test]
    fn a_sync_point_waits_for_the_sync_in_flight_instead_of_interleaving() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let (data, _old, new) = two_remotes(&fixture, "trunk");
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &data, false).unwrap();
        // A sync is in flight (a transport is waiting on the old remote).
        let sync = SyncLock::take(&data).unwrap().unwrap();
        let switching = {
            let (data, new) = (data.clone(), new.clone());
            thread::spawn(move || connect_at(&data, new.to_str().unwrap()).unwrap())
        };
        thread::sleep(Duration::from_millis(500));
        assert!(!switching.is_finished(), "setup --vault did not wait");
        let vault = data.join("vault");
        assert!(
            git(&vault, &["remote", "get-url", "origin"])
                .unwrap()
                .ends_with("old.git")
        );
        assert_eq!(
            git(&vault, &["config", "branch.main.merge"]).unwrap(),
            "refs/heads/main"
        );
        drop(sync);
        switching.join().unwrap();
        // The switch landed whole: new remote, its default branch, our edits pushed to it.
        assert_eq!(
            git(&vault, &["config", "branch.main.merge"]).unwrap(),
            "refs/heads/trunk"
        );
        assert_eq!(
            git(&new, &["rev-parse", "trunk"]).unwrap(),
            git(&vault, &["rev-parse", "HEAD"]).unwrap()
        );
        assert_eq!(Vault::at(&data).unpushed().unwrap(), 0);
    }

    /// With a real sync paused in its transport on the old remote (a local stand-in that waits
    /// for a release file, then fails), `setup --vault <new>` waits; once the sync ends, setup
    /// finishes and the vault is whole on the new remote.
    #[test]
    fn setup_to_a_new_remote_waits_for_a_sync_paused_in_its_transport() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let (data, old, new) = two_remotes(&fixture, "trunk");
        let vault = data.join("vault");
        let (marker, release) = (
            fixture.0.join("transport-started"),
            fixture.0.join("transport-release"),
        );
        let script = fixture.0.join("paused-transport");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch '{}'\nwhile [ ! -e '{}' ]; do sleep 0.1; done\nexit 1\n",
                marker.display(),
                release.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        git(&vault, &["config", "protocol.ext.allow", "always"]).unwrap();
        git(
            &vault,
            &[
                "remote",
                "set-url",
                "origin",
                &format!("ext::{}", script.display()),
            ],
        )
        .unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &data, false).unwrap();
        let syncing = {
            let data = data.clone();
            thread::spawn(move || sync_at(&data).unwrap())
        };
        let until = Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(Instant::now() < until, "the sync never reached the network");
            thread::sleep(Duration::from_millis(20));
        }
        let switching = {
            let (data, new) = (data.clone(), new.clone());
            thread::spawn(move || connect_at(&data, new.to_str().unwrap()).unwrap())
        };
        thread::sleep(Duration::from_millis(500));
        assert!(!switching.is_finished(), "setup --vault did not wait");
        assert!(
            git(&vault, &["remote", "get-url", "origin"])
                .unwrap()
                .starts_with("ext::")
        );
        fs::write(&release, "").unwrap();
        syncing.join().unwrap();
        switching.join().unwrap();
        assert!(
            git(&vault, &["remote", "get-url", "origin"])
                .unwrap()
                .ends_with("new.git")
        );
        // The new remote's default branch got the edits, and the vault tracks it.
        assert_eq!(
            git(&vault, &["config", "branch.main.merge"]).unwrap(),
            "refs/heads/trunk"
        );
        assert_eq!(
            git(&new, &["rev-parse", "trunk"]).unwrap(),
            git(&vault, &["rev-parse", "HEAD"]).unwrap()
        );
        assert!(
            git(&new, &["ls-tree", "-r", "--name-only", "trunk"])
                .unwrap()
                .contains("S902-two")
        );
        assert!(
            git(&old, &["ls-tree", "-r", "--name-only", "main"]).is_ok_and(|t| !t.contains("S902"))
        );
        assert_eq!(
            git(
                &vault,
                &["for-each-ref", "--format=%(refname)", "refs/remotes"]
            )
            .unwrap(),
            "refs/remotes/origin/trunk"
        );
        assert_eq!(Vault::at(&data).hint().unwrap(), None);
        // Anything not on the new remote is named by `list`.
        let id = project_id(&ctx).unwrap();
        fs::write(
            vault.join(format!("{id}-shop/hidden/scenarios/S901-one/scenario.md")),
            "edited after the switch\n",
        )
        .unwrap();
        let note = Vault::at(&data).hint().unwrap().unwrap();
        assert!(
            note.contains("S901") && note.contains("not pushed"),
            "{note}"
        );
    }

    #[test]
    fn ordinary_commands_take_neither_the_sync_lock_nor_the_vault_lock() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let _sync = SyncLock::take(&data).unwrap().unwrap();
        let reader = data.clone();
        answers_while_locked(&data, move || {
            where_at(&ctx, &reader).unwrap();
            Vault::at(&reader).hint().unwrap();
            conflict_note(&reader).unwrap();
        });
    }

    #[test]
    fn a_push_updates_the_tracking_ref_and_leaves_the_upstream_alone() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let (data, _old, new) = two_remotes(&fixture, "trunk");
        connect_at(&data, new.to_str().unwrap()).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &data, false).unwrap();
        let mut vault = Vault::lock(&data).unwrap();
        assert_eq!(vault.unpushed().unwrap(), 1);
        vault.push_to("origin", "trunk").unwrap();
        assert_eq!(
            git(&vault.root, &["config", "branch.main.merge"]).unwrap(),
            "refs/heads/trunk"
        );
        assert_eq!(vault.unpushed().unwrap(), 0);
        assert_eq!(
            git(&new, &["rev-parse", "trunk"]).unwrap(),
            git(&vault.root, &["rev-parse", "HEAD"]).unwrap()
        );
    }

    #[test]
    fn a_sandboxed_where_pull_prints_no_vault_notice_even_with_a_conflict() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let home = fixture.0.join("home");
        let data = home.join(".local/share/delegate");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let vault = data.join("vault");
        let tree = git(&vault, &["hash-object", "-t", "tree", "/dev/null"]).unwrap();
        let theirs = git(
            &vault,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit-tree",
                "-m",
                "theirs",
                &tree,
            ],
        )
        .unwrap();
        git(&vault, &["update-ref", "refs/delegate/theirs", &theirs]).unwrap();
        fs::write(vault.join(".git/delegate-conflict"), "a/file\n").unwrap();
        let fixture_path = fixture.0.display().to_string();
        let run = |sandboxed: bool, args: &str| {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "vault::tests::command_bridge",
                    "--nocapture",
                    "--quiet",
                ])
                .current_dir(&ctx.root)
                .env("HOME", &home)
                .env("DELEGATE_VAULT_TEST_COMMAND", args)
                .env_remove("DELEGATE_HOME")
                .env_remove("XDG_DATA_HOME")
                .env_remove("DELEGATE_SANDBOXED");
            if sandboxed {
                command.env("DELEGATE_SANDBOXED", "1");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            // The fixture's own folder name says "vault"; only delegate's words count.
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
            .replace(&fixture_path, "")
        };
        // The same command outside the sandbox does report it.
        assert!(
            run(false, "where\u{1f}--pull\u{1f}").contains("vault: conflict"),
            "control"
        );
        // Hand edits, a duplicate folder and a remote whose transport would show up in `pids`.
        let (script, pids) = stubborn_transport(&fixture);
        git(
            &vault,
            &["config", "core.sshCommand", &script.display().to_string()],
        )
        .unwrap();
        git(
            &vault,
            &["remote", "add", "origin", "ssh://vault.invalid/repo.git"],
        )
        .unwrap();
        let id = project_id(&ctx).unwrap();
        let duplicate = vault.join(format!("{id}-shop-duplicate"));
        copy_folder(&vault.join(format!("{id}-shop")), &duplicate).unwrap();
        fs::write(
            duplicate.join("hidden/scenarios/S901-one/scenario.md"),
            "different copy\n",
        )
        .unwrap();
        fs::write(
            vault.join(format!("{id}-shop/hidden/scenarios/S901-one/check")),
            "private hand edit\n",
        )
        .unwrap();
        let before = git(&vault, &["status", "--porcelain"]).unwrap();
        for args in ["where\u{1f}--pull\u{1f}", "where\u{1f}", "list\u{1f}"] {
            let output = run(true, args);
            assert!(!output.contains("vault"), "{args}: {output}");
            assert!(!output.contains("S901"), "{args}: {output}");
            assert!(!output.contains("a/file"), "{args}: {output}");
            if args.starts_with("where") {
                assert!(
                    output.contains("hidden none: sandboxed"),
                    "{args}: {output}"
                );
                assert!(output.contains("tasks "), "{args}: {output}");
            }
        }
        // `setup` skips its vault part entirely: no health report, no edit or conflict notice.
        let output = run(true, "setup\u{1f}");
        for word in ["vault", "a/file", "conflict", "S901", "local edits"] {
            assert!(!output.contains(word), "setup {word}: {output}");
        }
        let output = run(
            true,
            "setup\u{1f}--vault\u{1f}ssh://vault.invalid/other.git\u{1f}",
        );
        assert!(!output.contains("vault"), "setup --vault: {output}");
        // Nothing synced, reconciled or contacted the remote.
        assert!(!pids.exists(), "a sandboxed command started a transport");
        assert!(duplicate.exists(), "a sandboxed command reconciled folders");
        assert_eq!(git(&vault, &["status", "--porcelain"]).unwrap(), before);
        assert_eq!(
            git(&vault, &["remote", "get-url", "origin"]).unwrap(),
            "ssh://vault.invalid/repo.git"
        );
    }

    /// One sandboxed command in a child process, with a legacy home holding a hidden scenario
    /// next to a vault that already has one. Returns (fixture, vault, legacy folder, runner).
    fn sandboxed_legacy_fixture() -> (Fixture, PathBuf, PathBuf, PathBuf) {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let home = fixture.0.join("home");
        let data = home.join(".local/share/delegate");
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, false).unwrap();
        let source = ctx.dhome.join("_evals/hidden/scenarios/S990-private");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("scenario.md"), "private legacy scenario\n").unwrap();
        (fixture, data.join("vault"), source, home)
    }

    fn command_output(fixture: &Fixture, home: &Path, sandboxed: bool, args: &str) -> String {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "vault::tests::command_bridge",
                "--nocapture",
                "--quiet",
            ])
            .current_dir(fixture.0.join("shop"))
            .env("HOME", home)
            .env("DELEGATE_VAULT_TEST_COMMAND", args)
            .env_remove("DELEGATE_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("DELEGATE_SANDBOXED");
        if sandboxed {
            command.env("DELEGATE_SANDBOXED", "1");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        // The fixture's own folder name says "vault"; only delegate's words count.
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .replace(&fixture.0.display().to_string(), "")
    }

    #[test]
    fn a_sandboxed_command_does_not_migrate_a_legacy_home_or_name_the_vault() {
        let (fixture, vault, source, home) = sandboxed_legacy_fixture();
        let before = git(&vault, &["rev-parse", "HEAD"]).unwrap();
        for args in ["where\u{1f}--pull\u{1f}", "where\u{1f}", "list\u{1f}"] {
            let output = command_output(&fixture, &home, true, args);
            assert!(!output.contains("vault"), "{args}: {output}");
            assert!(!output.contains("S990"), "{args}: {output}");
            if args.starts_with("where") {
                assert!(output.contains("hidden none: sandboxed"), "{output}");
            }
            assert_eq!(git(&vault, &["rev-parse", "HEAD"]).unwrap(), before);
            assert!(source.exists(), "{args}: consumed the legacy hidden set");
        }
        // Outside the sandbox the same command is what moves it.
        command_output(&fixture, &home, false, "where\u{1f}");
        assert!(!source.exists());
        assert_ne!(git(&vault, &["rev-parse", "HEAD"]).unwrap(), before);
    }

    #[test]
    fn sandboxed_setup_does_not_reconcile_folders_or_leak_hidden_names() {
        let (fixture, vault, source, home) = sandboxed_legacy_fixture();
        let ctx = fixture.ctx();
        let id = project_id(&ctx).unwrap();
        let project = vault.join(format!("{id}-shop"));
        let duplicate = vault.join(format!("{id}-shop-duplicate"));
        copy_folder(&project, &duplicate).unwrap();
        fs::write(
            duplicate.join("hidden/scenarios/S901-one/scenario.md"),
            "different copy\n",
        )
        .unwrap();
        git(&vault, &["add", "-A"]).unwrap();
        git(
            &vault,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit",
                "-m",
                "dup",
            ],
        )
        .unwrap();
        let before = git(&vault, &["rev-parse", "HEAD"]).unwrap();
        let output = command_output(&fixture, &home, true, "setup\u{1f}");
        for word in ["vault", "S901", "S990", "conflict", "local edits"] {
            assert!(!output.contains(word), "setup {word}: {output}");
        }
        assert!(duplicate.exists(), "sandboxed setup reconciled folders");
        assert!(
            source.exists(),
            "sandboxed setup consumed the legacy hidden set"
        );
        assert_eq!(git(&vault, &["rev-parse", "HEAD"]).unwrap(), before);
    }

    #[test]
    fn a_sync_point_waits_one_sync_for_a_busy_sync_lock_then_goes_on_locally() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let (data, _old, new) = two_remotes(&fixture, "trunk");
        let id = project_id(&ctx).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &data, false).unwrap();
        let vault = data.join("vault");
        let held = SyncLock::take(&data).unwrap().unwrap();
        // The wait itself is bounded by the caller's deadline.
        let started = Instant::now();
        assert!(
            SyncLock::take_within(&data, Duration::from_millis(300))
                .unwrap()
                .is_none()
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        // Three sync points queue behind it; none outlasts one sync's network limit by much.
        let started = Instant::now();
        let again = || Ctx {
            root: ctx.root.clone(),
            dhome: ctx.dhome.clone(),
            gitdir: ctx.gitdir.clone(),
        };
        let pull = {
            let (ctx, data) = (again(), data.clone());
            thread::spawn(move || where_mode(&ctx, &data, true).unwrap())
        };
        let add = {
            let folder = Fixture::scenario(&ctx, "S903-three");
            let (ctx, data) = (again(), data.clone());
            thread::spawn(move || add_at(&ctx, &folder, &data, true).unwrap())
        };
        let connect = {
            let (data, new) = (data.clone(), new.clone());
            thread::spawn(move || connect_at(&data, new.to_str().unwrap()))
        };
        pull.join().unwrap();
        add.join().unwrap();
        let error = connect.join().unwrap().unwrap_err().to_string();
        assert!(started.elapsed() < GIT_LIMIT + Duration::from_secs(5));
        assert!(
            error.contains(SYNC_BUSY) && error.contains("retry"),
            "{error}"
        );
        // The edit is committed locally and named as not pushed; the remote is unchanged.
        let note = Vault::at(&data).hint().unwrap().unwrap();
        assert!(
            note.contains("not pushed") && Vault::at(&data).unpushed().unwrap() == 2,
            "{note}"
        );
        assert!(
            git(&vault, &["remote", "get-url", "origin"])
                .unwrap()
                .ends_with("old.git")
        );
        assert!(
            vault
                .join(format!("{id}-shop/hidden/scenarios/S903-three"))
                .is_dir()
        );
        drop(held);
        // Free again: the next sync point takes it at once.
        assert!(SyncLock::take(&data).unwrap().is_some());
    }

    #[test]
    fn the_lock_is_free_while_a_sync_waits_on_the_network() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, true).unwrap();
        connect_at(&data, remote.to_str().unwrap()).unwrap();
        let marker = fixture.0.join("transport-started");
        let script = fixture.0.join("slow-transport");
        fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\nsleep 3\nexit 1\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let vault = data.join("vault");
        // A local stand-in transport that answers slowly, never a network service.
        git(&vault, &["config", "protocol.ext.allow", "always"]).unwrap();
        git(
            &vault,
            &[
                "remote",
                "set-url",
                "origin",
                &format!("ext::{}", script.display()),
            ],
        )
        .unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &data, false).unwrap();
        let syncing = {
            let data = data.clone();
            thread::spawn(move || sync_at(&data).unwrap())
        };
        let until = Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(Instant::now() < until, "the sync never reached the network");
            thread::sleep(Duration::from_millis(20));
        }
        let probe = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data.join("vault.lock"))
            .unwrap();
        assert!(
            probe.try_lock().is_ok(),
            "the lock was held during a network call"
        );
        drop(probe);
        let sync = OpenOptions::new()
            .write(true)
            .open(data.join("vault-sync.lock"))
            .unwrap();
        assert!(
            sync.try_lock().is_err(),
            "the sync lock was free during a network call"
        );
        drop(sync);
        syncing.join().unwrap();
    }

    #[test]
    fn setup_to_another_remote_tracks_its_default_branch() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let (first, second) = (fixture.0.join("first.git"), fixture.0.join("second.git"));
        for remote in [&first, &second] {
            fs::create_dir(remote).unwrap();
            git(remote, &["init", "--bare", "--initial-branch=trunk"]).unwrap();
        }
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data, true).unwrap();
        // The second remote already has a default branch of its own.
        let other = fixture.0.join("other");
        fs::create_dir(&other).unwrap();
        git(&other, &["init", "--initial-branch=trunk"]).unwrap();
        fs::write(other.join("README"), "other\n").unwrap();
        git(&other, &["add", "README"]).unwrap();
        git(
            &other,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@l",
                "commit",
                "-m",
                "other",
            ],
        )
        .unwrap();
        git(&other, &["push", second.to_str().unwrap(), "trunk"]).unwrap();
        connect_at(&data, first.to_str().unwrap()).unwrap();
        connect_at(&data, second.to_str().unwrap()).unwrap();
        let vault = data.join("vault");
        assert_eq!(
            git(&vault, &["config", "branch.main.merge"]).unwrap(),
            "refs/heads/trunk"
        );
        assert_eq!(
            git(&vault, &["rev-parse", "--abbrev-ref", "@{upstream}"]).unwrap(),
            "origin/trunk"
        );
        assert_eq!(
            git(&second, &["rev-parse", "trunk"]).unwrap(),
            git(&vault, &["rev-parse", "HEAD"]).unwrap()
        );
        assert_eq!(Vault::at(&data).hint().unwrap(), None);
    }

    #[test]
    fn vault_git_has_a_transport_deadline() {
        let fixture = Fixture::new();
        let error = git(&fixture.0, &["-c", "alias.stall=!sleep 30", "stall"]).unwrap_err();
        assert!(error.to_string().contains("timed out after 10s"), "{error}");
    }

    #[test]
    fn credential_failures_are_recognised_from_git_stderr() {
        for stderr in [
            "fatal: could not read Username for 'https://gitlab.example/vault.git': terminal prompts disabled",
            "fatal: could not read Password for 'https://user@gitlab.example/vault.git': terminal prompts disabled",
        ] {
            assert!(needs_credentials(stderr), "{stderr}");
        }
        assert!(!needs_credentials(
            "fatal: repository 'https://gitlab.example/nope.git/' not found"
        ));
        let hint = credentials_hint("https://gitlab.example/vault.git");
        assert!(hint.contains("no saved credentials"), "{hint}");
        assert!(hint.contains("SSH URL"), "{hint}");
        assert!(
            hint.contains("git config --global credential.helper") && hint.contains("git ls-remote"),
            "{hint}"
        );
        assert!(
            hint.ends_with("then rerun delegate setup --vault https://gitlab.example/vault.git"),
            "{hint}"
        );
    }

    #[test]
    fn bad_remote_does_not_create_or_connect_a_vault_and_failed_push_keeps_commit() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let missing = fixture.0.join("missing.git");
        assert!(connect_at(&data, missing.to_str().unwrap()).is_err());
        // The sync lock file may exist; the vault must not.
        assert!(!data.join("vault").exists());
        {
            let vault = Vault::lock(&data).unwrap();
            vault.init().unwrap();
            git(
                &vault.root,
                &["remote", "add", "origin", missing.to_str().unwrap()],
            )
            .unwrap();
        }
        let folder = Fixture::scenario(&ctx, "S901-one");
        add_at(&ctx, &folder, &data, true).unwrap();
        let vault = Vault::lock(&data).unwrap();
        assert!(
            git(&vault.root, &["log", "-1", "--format=%s"])
                .unwrap()
                .starts_with("Hide S901-one")
        );
        assert!(!folder.exists());
    }
}
