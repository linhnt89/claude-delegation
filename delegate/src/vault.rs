//! Project identity and the machine-wide, locked git vault.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let path = ctx.root.join(RECORD);
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
            return Ok(uuid.to_string());
        }
    }
    let id = Uuid::new_v4().to_string();
    let body = if let Some(text) = text {
        format!("project = \"{id}\"\n{text}")
    } else {
        let mut record = Record::new();
        record.project = Some(id.clone());
        toml::to_string(&record)?
    };
    fs::write(&path, body).context("writing project id; check .delegate-kit.toml permissions")?;
    eprintln!("added project id; commit .delegate-kit.toml");
    Ok(id)
}

/// Git used for vault operations must never ask for credentials or launch an editor.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=10");
    }
    let out = command
        .args(args)
        .output()
        .context("running git for the vault")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

#[derive(Debug)]
pub(crate) struct Vault {
    pub(crate) root: PathBuf,
    _lock: File,
}

impl Vault {
    fn lock(data: &Path) -> Result<Self> {
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

    fn init(&self) -> Result<()> {
        if !self.root.join(".git").is_dir() {
            fs::create_dir_all(&self.root)?;
            git(&self.root, &["init", "--initial-branch=main"])?;
            self.commit("Initialize delegate vault", true)?;
        }
        Ok(())
    }

    fn commit(&self, message: &str, empty: bool) -> Result<()> {
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

    fn project(&self, ctx: &Ctx, id: &str) -> Result<Option<PathBuf>> {
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
        if matches.len() > 1 {
            bail!(
                "multiple vault folders for project {id}; reconcile them in {} before retrying",
                self.root.display()
            );
        }
        let main = ctx.gitdir.parent().unwrap_or(&ctx.root);
        let name = main.file_name().unwrap_or_default().to_string_lossy();
        let dest = self.root.join(format!("{id}-{name}"));
        if let Some(found) = matches.first() {
            if *found != dest {
                fs::rename(found, &dest)
                    .context("renaming project folder in the vault; check permissions")?;
            }
            Ok(Some(dest))
        } else {
            Ok(None)
        }
    }

    fn remote(&self) -> Result<Option<String>> {
        let remotes = git(&self.root, &["remote"])?;
        Ok(remotes
            .lines()
            .find(|r| *r == "origin")
            .or_else(|| remotes.lines().next())
            .map(str::to_owned))
    }

    fn push(&self) {
        let result = (|| -> Result<()> {
            if let Some(remote) = self.remote()? {
                let upstream = git(&self.root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok();
                let target = upstream
                    .as_deref()
                    .and_then(|u| u.strip_prefix(&format!("{remote}/")))
                    .map_or_else(|| "HEAD".to_owned(), |b| format!("HEAD:refs/heads/{b}"));
                git(&self.root, &["push", &remote, &target])?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!(
                "vault push failed (commit kept): {}",
                e.to_string().replace(['\n', '\r'], " ")
            );
        }
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
                let upstream = git(&self.root, &["rev-parse", "--abbrev-ref", "@{upstream}"]);
                if let Ok(upstream) = upstream {
                    let count = git(
                        &self.root,
                        &["rev-list", "--count", &format!("{upstream}..HEAD")],
                    )?;
                    if count != "0" {
                        println!(
                            "vault: {count} commits not pushed; run delegate setup --vault <url>"
                        );
                    }
                }
            }
            None => println!("vault remote: none; connect with delegate setup --vault <url>"),
        }
        if !git(&self.root, &["status", "--porcelain"])?.is_empty() {
            println!("vault: local edits present (kept)");
        }
        Ok(())
    }
}

fn where_at(ctx: &Ctx, data: &Path) -> Result<()> {
    let id = project_id(ctx)?;
    println!("tasks {}", ctx.dhome.display());
    if !data.join("vault/.git").is_dir() {
        println!(
            "hidden none: no vault on this machine; use delegate hidden add or delegate setup --vault <url>"
        );
        return Ok(());
    }
    let vault = Vault::lock(data)?;
    if let Some(project) = vault.project(ctx, &id)? {
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
        if !git(&vault.root, &["status", "--porcelain"])?.is_empty() {
            bail!(
                "vault has local edits; commit them in {} before connecting the remote",
                vault.root.display()
            );
        }
        git(&vault.root, &["fetch", url, branch])?;
        if let Err(error) = git(
            &vault.root,
            &[
                "-c",
                "user.name=Delegate",
                "-c",
                "user.email=delegate@localhost",
                "merge",
                "--no-edit",
                "--no-commit",
                "--no-ff",
                "--allow-unrelated-histories",
                "FETCH_HEAD",
            ],
        ) {
            git(&vault.root, &["merge", "--abort"]).ok();
            return Err(error).context("vault merge failed; local scenarios were kept; reconcile both vault histories before reconnecting");
        }
        if vault.root.join(".git/MERGE_HEAD").exists() {
            let removed = git(
                &vault.root,
                &["diff", "--cached", "--name-only", "--diff-filter=D"],
            )?;
            if removed
                .lines()
                .any(|p| p.contains("/hidden/scenarios/") && p.ends_with("/scenario.md"))
            {
                git(&vault.root, &["merge", "--abort"])?;
                bail!(
                    "vault merge would delete a hidden scenario; local scenarios were kept; reconcile the remote before retrying"
                );
            }
            vault.commit("Merge vault remote", false)?;
        }
    }
    if git(&vault.root, &["remote"])?
        .lines()
        .any(|r| r == "origin")
    {
        git(&vault.root, &["remote", "set-url", "origin", url])?;
    } else {
        git(&vault.root, &["remote", "add", "origin", url])?;
    }
    if let Err(e) = git(
        &vault.root,
        &[
            "push",
            "--set-upstream",
            "origin",
            &format!("HEAD:{branch}"),
        ],
    ) {
        eprintln!(
            "vault push failed (commit kept): {}",
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
    fn project_folder_follows_main_checkout_rename_and_never_overwrites_a_scenario() {
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
        let hidden = data.join(format!("vault/{id}-new-name/hidden/scenarios/S901-one"));
        assert!(hidden.join("scenario.md").is_file());
        assert!(!data.join(format!("vault/{id}-shop")).exists());
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
    fn connecting_never_merges_a_hidden_scenario_deletion() {
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
        connect_at(&a, remote.to_str().unwrap()).unwrap();
        let id = project_id(&ctx).unwrap();
        let scenario = format!("{id}-shop/hidden/scenarios/S901-one/scenario.md");
        // Simulate somebody deleting a scenario on the other machine.
        {
            let vault = Vault::lock(&a).unwrap();
            git(&vault.root, &["rm", &scenario]).unwrap();
            vault.commit("Remote deletion", false).unwrap();
            vault.push();
        }
        assert!(connect_at(&b, remote.to_str().unwrap()).is_err());
        assert!(b.join("vault").join(scenario).is_file());
        assert!(!b.join("vault/.git/MERGE_HEAD").exists());
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
