//! Project identity and the machine-wide, locked git vault.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

fn git_input(dir: &Path, args: &[&str], index: Option<&Path>, input: &str) -> Result<String> {
    let bytes = git_bytes(dir, args, index, input)?;
    Ok(String::from_utf8_lossy(&bytes).trim_end().to_owned())
}

fn git_bytes(dir: &Path, args: &[&str], index: Option<&Path>, input: &str) -> Result<Vec<u8>> {
    // Bound credential helpers and transports too, not only SSH's connect phase.
    let mut command = Command::new("timeout");
    command.args(["10s", "git"]);
    command
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=10");
    }
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let mut child = command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git for the vault")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let out = child.wait_with_output().context("waiting for vault git")?;
    if matches!(out.status.code(), Some(124 | 137)) {
        bail!(
            "git {} timed out after 10s; check the vault remote and credentials",
            args.join(" ")
        );
    }
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

fn project_prefix(name: &str) -> Option<&str> {
    let (id, suffix) = name.split_at_checked(36)?;
    (suffix.starts_with('-') && Uuid::parse_str(id).is_ok()).then_some(id)
}

#[derive(Debug)]
pub(crate) struct Vault {
    pub(crate) root: PathBuf,
    _lock: File,
}

impl Vault {
    pub(crate) fn lock(data: &Path) -> Result<Self> {
        fs::create_dir_all(data).context("creating delegate data folder; check its permissions")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data.join("vault.lock"))?;
        lock.lock()
            .context("locking delegate vault; retry the command")?;
        Ok(Self {
            root: data.join("vault"),
            _lock: lock,
        })
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
        let mut matches = self.project_folders(id)?;
        if matches.len() > 1 && self.conflict()?.is_none() {
            self.reconcile_project(&mut matches)?;
            self.commit_edits()?;
            self.push();
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

    fn branch(&self) -> Result<String> {
        if let Ok(upstream) = git(&self.root, &["rev-parse", "--abbrev-ref", "@{upstream}"])
            && let Some((_, branch)) = upstream.split_once('/')
        {
            return Ok(branch.to_owned());
        }
        git(&self.root, &["symbolic-ref", "--short", "HEAD"])
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
        let ids: std::collections::BTreeSet<_> = changed
            .split('\0')
            .chain(staged.split('\0'))
            .chain(new.split('\0'))
            .filter_map(|path| path.split_once("/hidden/scenarios/"))
            .filter_map(|(_, path)| path.split('/').next())
            .map(|name| name.split('-').next().unwrap_or(name))
            .collect();
        let names = ids.into_iter().collect::<Vec<_>>().join(", ");
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

    fn merge_remote(&self, source: &str, branch: &str) -> Result<()> {
        if let Some(files) = self.conflict()? {
            bail!("{}", self.conflict_hint(&files));
        }
        // Probe without writing FETCH_HEAD or tracking metadata. A health check
        // against an unchanged remote must not change any machine files.
        let tip = git(
            &self.root,
            &["ls-remote", source, &format!("refs/heads/{branch}")],
        )?;
        // An empty remote (or a branch not published yet) still needs our push.
        if tip.is_empty() {
            return Ok(());
        }
        if let Some((hash, _)) = tip.split_once('\t')
            && git(&self.root, &["merge-base", "--is-ancestor", hash, "HEAD"]).is_ok()
        {
            return Ok(());
        }
        git(&self.root, &["fetch", source, branch])?;
        // Do not create a merge commit for history already present locally.
        if git(
            &self.root,
            &["merge-base", "--is-ancestor", "FETCH_HEAD", "HEAD"],
        )
        .is_ok()
        {
            return Ok(());
        }
        git(
            &self.root,
            &["update-ref", "refs/delegate/theirs", "FETCH_HEAD"],
        )?;
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

    fn push_to(&self, remote: &str, branch: &str) -> Result<()> {
        git(
            &self.root,
            &[
                "push",
                "--set-upstream",
                remote,
                &format!("HEAD:refs/heads/{branch}"),
            ],
        )?;
        Ok(())
    }

    pub(crate) fn push(&self) {
        self.sync_remote(false);
    }

    fn sync_remote(&self, pull: bool) {
        let result = (|| -> Result<()> {
            if let Some(files) = self.conflict()? {
                eprintln!("{}", self.conflict_hint(&files));
                return Ok(());
            }
            // Legacy duplicates can be repaired locally even while offline.
            self.reconcile_projects()?;
            let remote = self.remote()?;
            if pull && let Some(remote) = &remote {
                self.merge_remote(remote, &self.branch()?)?;
            }
            self.reconcile_projects()?;
            if let Some(remote) = remote {
                let branch = self.branch()?;
                if self.unpushed()? == 0 {
                    return Ok(());
                }
                if let Err(error) = self.push_to(&remote, &branch) {
                    // Offline/auth failures need no second network attempt.
                    if error.to_string().contains("[rejected]") {
                        self.merge_remote(&remote, &branch)?;
                        self.reconcile_projects()?;
                        self.push_to(&remote, &branch)?;
                    } else {
                        return Err(error);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            let reason = e.to_string().replace(['\n', '\r'], " ");
            let count = self.unpushed().unwrap_or(0);
            if count > 0 {
                eprintln!("vault: {count} commit(s) not pushed: {reason}");
            } else {
                eprintln!("vault: couldn't check it's the latest; using local copy: {reason}");
            }
        }
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

    fn pull(&self) {
        let result = (|| -> Result<()> {
            if let Some(files) = self.conflict()? {
                bail!("{}", self.conflict_hint(&files));
            }
            if let Some(remote) = self.remote()? {
                // Preserve local edits before merging; never stash or reset them.
                self.commit_edits()?;
                self.merge_remote(&remote, &self.branch()?)?;
            }
            self.reconcile_projects()?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!(
                "vault: couldn't check it's the latest; using local copy: {}",
                error.to_string().replace(['\n', '\r'], " ")
            );
        }
    }

    fn hint(&self) -> Result<Option<String>> {
        if let Some(files) = self.conflict()? {
            return Ok(Some(self.conflict_hint(&files)));
        }
        let count = self.unpushed()?;
        Ok((count > 0).then(|| format!("vault: {count} commit(s) not pushed")))
    }

    pub(crate) fn health(&self) -> Result<()> {
        println!("vault {}", self.root.display());
        match self.remote()? {
            Some(remote) => {
                let url = git(&self.root, &["remote", "get-url", &remote])?;
                println!("vault remote {url}");
                match git(&self.root, &["ls-remote", &remote]) {
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
    let vault = Vault::lock(data)?;
    if pull {
        vault.pull();
    } else if let Some(files) = vault.conflict()? {
        eprintln!("{}", vault.conflict_hint(&files));
    }
    if let Some(project) = vault.project(ctx, &id)? {
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

pub(crate) fn where_paths(ctx: &Ctx) -> Result<()> {
    where_at(ctx, &data_dir())
}

pub(crate) fn where_pulled(ctx: &Ctx) -> Result<()> {
    notices(ctx)?;
    where_mode(ctx, &data_dir(), true)
}

/// Pre-command transaction. An absent vault is never created by a read command.
pub(crate) fn sync() -> Result<()> {
    sync_at(&data_dir())
}

fn sync_at(data: &Path) -> Result<()> {
    if data.join("vault/.git").is_dir() {
        let vault = Vault::lock(data)?;
        vault.commit_edits()?;
        vault.sync_remote(true);
    }
    Ok(())
}

pub(crate) fn hint() -> Result<Option<String>> {
    let data = data_dir();
    if !data.join("vault/.git").is_dir() {
        return Ok(None);
    }
    Vault::lock(&data)?.hint()
}

pub(crate) fn machine_not_setup() -> Result<bool> {
    if crate::config::user_config_path().is_file() {
        return Ok(false);
    }
    let data = data_dir();
    Ok(!data.join("vault/.git").is_dir() || Vault::lock(&data)?.remote()?.is_none())
}

pub(crate) fn notices(ctx: &Ctx) -> Result<()> {
    if machine_not_setup()? {
        eprintln!("this machine isn't set up: run delegate setup --vault <url>");
    }
    if let Some(id) = read_project_id(ctx)? {
        let data = data_dir();
        let known = if data.join("vault/.git").is_dir() {
            let vault = Vault::lock(&data)?;
            let prefix = format!("{id}-");
            fs::read_dir(&vault.root)?
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

fn add_at(ctx: &Ctx, folder: &Path, data: &Path) -> Result<()> {
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
    let vault = Vault::lock(data)?;
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
    vault.push();
    Ok(())
}

pub(crate) fn add(ctx: &Ctx, folder: &Path) -> Result<()> {
    add_at(ctx, folder, &data_dir())
}

/// Probe before changing local state, so an unreachable URL is never connected.
fn connect_at(data: &Path, url: &str) -> Result<()> {
    let refs = git(&std::env::current_dir()?, &["ls-remote", "--symref", url])
        .context("cannot reach vault URL; check the URL and credentials, then rerun delegate setup --vault <url>")?;
    let vault = Vault::lock(data)?;
    vault.init()?;
    if let Some(files) = vault.conflict()? {
        bail!("{}", vault.conflict_hint(&files));
    }
    vault.commit_edits()?;
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
    if refs.lines().any(|l| l.contains("\trefs/heads/")) {
        vault.merge_remote(url, branch)?;
    }
    vault.reconcile_projects()?;
    if git(&vault.root, &["remote"])?
        .lines()
        .any(|r| r == "origin")
    {
        git(&vault.root, &["remote", "set-url", "origin", url])?;
    } else {
        git(&vault.root, &["remote", "add", "origin", url])?;
    }
    if let Err(e) = vault.push_to("origin", branch) {
        eprintln!(
            "vault: {} commit(s) not pushed: {}",
            vault.unpushed()?,
            e.to_string().replace(['\n', '\r'], " ")
        );
    }
    vault.health()
}

pub(crate) fn connect(url: &str) -> Result<()> {
    connect_at(&data_dir(), url)
}

pub(crate) fn health() -> Result<()> {
    let data = data_dir();
    if !data.join("vault/.git").is_dir() {
        println!("vault: not created; use delegate hidden add or delegate setup --vault <url>");
        return Ok(());
    }
    Vault::lock(&data)?.health()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

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
                add_at(&ctx, &folder, &data).unwrap();
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
        add_at(&ctx, &folder, &data).unwrap();
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
        assert!(add_at(&ctx, &folder, &data).is_err());
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
        add_at(&a_ctx, &Fixture::scenario(&a_ctx, "S901-one"), &a).unwrap();
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
        add_at(&b_ctx, &Fixture::scenario(&b_ctx, "S902-two"), &b).unwrap();
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        fs::remove_dir_all(a.join(format!("vault/{id}-shop"))).unwrap();
        sync_at(&a).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();
        let renamed = fixture.0.join("b-shop");
        fs::rename(&ctx.root, &renamed).unwrap();
        ctx.root = renamed;
        ctx.gitdir = ctx.root.join(".git");
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &b).unwrap();
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a).unwrap();
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data).unwrap();
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &b).unwrap();
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S903-three"), &b).unwrap();
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
    fn list_syncs_other_machines_changes_and_converges_after_an_outage() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let a = fixture.0.join("machine-a/delegate");
        let b = fixture.0.join("machine-b/delegate");
        let remote = fixture.0.join("remote.git");
        fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "--initial-branch=main"]).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a).unwrap();
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        connect_at(&b, remote.to_str().unwrap()).unwrap();

        // Exercise real command dispatch, not evals/run or where --pull.
        let list = |data: &Path| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "vault::tests::command_bridge",
                    "--nocapture",
                    "--quiet",
                ])
                .current_dir(&ctx.root)
                .env("DELEGATE_VAULT_TEST_COMMAND", "list")
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S902-two"), &a).unwrap();
        let id = project_id(&ctx).unwrap();
        let scenario = |data: &Path, name: &str| {
            data.join(format!(
                "vault/{id}-shop/hidden/scenarios/{name}/scenario.md"
            ))
        };
        assert!(!scenario(&b, "S902-two").exists());
        list(&b);
        assert!(scenario(&b, "S902-two").is_file());

        let away = fixture.0.join("remote.away");
        fs::rename(&remote, &away).unwrap();
        add_at(&ctx, &Fixture::scenario(&ctx, "S903-three"), &b).unwrap();
        fs::rename(&away, &remote).unwrap();
        // A is initially up to date with the remote but B has an offline commit.
        // One command on B publishes it; A's next command must receive it.
        list(&a);
        list(&b);
        list(&a);
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
            list(data);
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
        add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &data).unwrap();
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
                add_at(&ctx, &Fixture::scenario(&ctx, name), &a).unwrap();
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
            add_at(&ctx, &Fixture::scenario(&ctx, "S901-one"), &a).unwrap();
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
            add_at(&ctx, &Fixture::scenario(&ctx, name), &data).unwrap();
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
        add_at(&ctx, &source, &a).unwrap();
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
            let vault = Vault::lock(&b).unwrap();
            vault.pull();
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

    #[test]
    fn vault_git_has_a_transport_deadline() {
        let fixture = Fixture::new();
        let error = git(&fixture.0, &["-c", "alias.stall=!sleep 30", "stall"]).unwrap_err();
        assert!(error.to_string().contains("timed out after 10s"), "{error}");
    }

    #[test]
    fn bad_remote_does_not_create_or_connect_a_vault_and_failed_push_keeps_commit() {
        let fixture = Fixture::new();
        let ctx = fixture.ctx();
        let data = fixture.0.join("data");
        let missing = fixture.0.join("missing.git");
        assert!(connect_at(&data, missing.to_str().unwrap()).is_err());
        assert!(!data.exists());
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
        add_at(&ctx, &folder, &data).unwrap();
        let vault = Vault::lock(&data).unwrap();
        assert!(
            git(&vault.root, &["log", "-1", "--format=%s"])
                .unwrap()
                .starts_with("Hide S901-one")
        );
        assert!(!folder.exists());
    }
}
