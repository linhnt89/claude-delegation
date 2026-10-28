//! Content-addressed kit upgrades and cleanup of the legacy kit status line.
//! Project-owned templates are advice, never write targets.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::guard::Install;
use crate::init::{CLAUDE_SETTINGS, EXECUTABLE, PROJECT_FILES, STATUS_LINE};
use crate::sha256;
use crate::task::git;

pub(crate) const RECORD: &str = ".delegate-kit.toml";
const OWNED: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "PLAN.md",
    ".delegate.toml",
    "evals/jobs.md",
];
const REBUILD: &str =
    "cd delegate && mise exec -- cargo build --release && cp target/release/delegate ~/.local/bin/";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub kit: String,
    pub files: BTreeMap<String, String>,
}

impl Record {
    pub fn new() -> Self {
        Self {
            project: Some(uuid::Uuid::new_v4().to_string()),
            kit: current().id,
            files: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Kit {
    id: String,
    files: BTreeMap<String, String>,
    templates: BTreeMap<String, String>,
}

fn current() -> Kit {
    // Hash the embedded templates once at runtime, never the catalog (which can
    // intentionally lag a locally modified binary), without rustc const evaluation.
    static CURRENT: OnceLock<Kit> = OnceLock::new();
    CURRENT
        .get_or_init(|| {
            let files: BTreeMap<_, _> = PROJECT_FILES
                .iter()
                .map(|(path, body)| ((*path).to_owned(), sha256::hex(body.as_bytes())))
                .collect();
            let mut lines = String::new();
            for (path, hash) in &files {
                lines.push_str(path);
                lines.push(' ');
                lines.push_str(hash);
                lines.push('\n');
            }
            Kit {
                id: sha256::hex(lines.as_bytes())[..12].to_owned(),
                files,
                templates: PROJECT_FILES
                    .iter()
                    .filter(|(p, _)| OWNED.contains(p))
                    .map(|(p, b)| ((*p).into(), (*b).into()))
                    .collect(),
            }
        })
        .clone()
}

fn history() -> Result<Vec<Kit>> {
    serde_json::from_str(include_str!("../templates/kit-history.txt"))
        .context("invalid embedded kit history; run scripts/kit-history and rebuild delegate")
}

/// Only unchanged, kit-owned files identify a legacy kit. Edited and missing
/// files provide no evidence; every recognizable file must agree with the kit.
fn identify<'a>(kits: &'a [Kit], hashes: &BTreeMap<String, String>) -> Option<&'a Kit> {
    let evidence: Vec<_> = hashes
        .iter()
        .filter(|(p, h)| {
            !OWNED.contains(&p.as_str()) && kits.iter().any(|k| k.files.get(*p) == Some(*h))
        })
        .collect();
    if evidence.is_empty() {
        return None;
    }
    kits.iter()
        .find(|kit| evidence.iter().all(|(p, h)| kit.files.get(*p) == Some(*h)))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e)
            .with_context(|| format!("read {}; check its permissions and retry", path.display())),
    }
}

/// `--all` includes HEAD and every ref, so this also finds committed deletions
/// and paths that only existed on another branch, not just files still in HEAD.
fn git_knows(root: &Path, path: &str) -> Result<bool> {
    let history = git(root, &["log", "--all", "-1", "--format=%H", "--", path])
        .with_context(|| format!("inspect history of {path}; check the repository and retry"))?;
    Ok(!history.is_empty())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create {}; check permissions and retry", parent.display()))?;
    }
    // Rename a new inode rather than following a kit file's symlink/hardlink
    // into a project-owned file (also makes each individual replacement atomic).
    let temp = path.with_file_name(format!(".delegate-upgrade-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.with_context(|| format!("write {}; check permissions and retry", path.display()))
}

fn check_parents(root: &Path, path: &str) -> Result<()> {
    let dest = root.join(path);
    for parent in dest.ancestors().skip(1).take_while(|p| *p != root) {
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!(
                    "cannot upgrade {path} through symlink {}; replace it with a directory and retry",
                    parent.display()
                );
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("inspect {}; check permissions and retry", parent.display())
                });
            }
        }
    }
    Ok(())
}

/// Retain comparison copies outside the project so the printed commands work
/// after exit. A private, unique directory avoids following another user's links.
fn comparison(kit: &str, side: &str, path: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("delegate-kit-{kit}-{}", uuid::Uuid::new_v4()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .context("create comparison directory; check your temporary directory's permissions")?;
    let dest = dir.join(side).join(path);
    write_file(&dest, bytes)?;
    Ok(dest)
}

fn template_diff(kit: &str, path: &str, old: &[u8], new: &[u8]) -> Result<String> {
    let old = comparison(kit, "old-template", path, old)?;
    let new = comparison(kit, "new-template", path, new)?;
    let out = Command::new("git")
        .args(["diff", "--no-index", "--no-ext-diff", "--no-color", "--"])
        .arg(old)
        .arg(new)
        .output()
        .context("run git diff; check that git is installed")?;
    if !matches!(out.status.code(), Some(0 | 1)) {
        bail!(
            "git diff failed: {}; check git and retry",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn kept(kit: &str, path: &str, body: &[u8]) -> Result<String> {
    let new = comparison(kit, "current", path, body)?;
    Ok(format!(
        "kept     {path} (edited; compare with the current kit: git diff --no-index '{path}' '{}'; take it with delegate upgrade {path})",
        new.display()
    ))
}

#[derive(Debug, Default)]
struct Plan {
    writes: BTreeMap<String, Vec<u8>>,
    removals: Vec<String>,
    output: Vec<String>,
}

impl Plan {
    fn full(
        &mut self,
        root: &Path,
        kits: &[Kit],
        now: &Kit,
        record: &mut Record,
        hashes: &BTreeMap<String, String>,
    ) -> Result<()> {
        // Only catalogued paths are candidates: arbitrary record keys and the
        // user's .claude/settings.json must never become write/remove targets.
        let paths: BTreeSet<_> = kits.iter().flat_map(|k| k.files.keys()).collect();
        for path in paths {
            if OWNED.contains(&path.as_str()) {
                continue;
            }
            let installed = record.files.get(path);
            let actual = hashes.get(path);
            let target = now.files.get(path);
            let body = PROJECT_FILES
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, b)| b.as_bytes());
            if actual.is_none() {
                if target.is_some() && (installed.is_some() || git_knows(root, path)?) {
                    self.output.push(format!(
                        "missing  {path}; restore with delegate upgrade {path}"
                    ));
                } else if let Some(body) = body {
                    self.writes.insert(path.clone(), body.to_vec());
                    record.files.insert(path.clone(), sha256::hex(body));
                    self.output.push(format!("added    {path}"));
                } else {
                    record.files.remove(path);
                }
                continue;
            }
            let pristine = installed.map_or_else(
                || kits.iter().any(|k| k.files.get(path) == actual),
                |hash| Some(hash) == actual,
            );
            if !pristine {
                self.output
                    .push(kept(&now.id, path, body.unwrap_or_default())?);
                continue;
            }
            if let Some(body) = body {
                let executable = EXECUTABLE.contains(&path.as_str());
                let mode_wrong = executable
                    && fs::metadata(root.join(path))?.permissions().mode() & 0o111 != 0o111;
                if actual != target || mode_wrong {
                    self.writes.insert(path.clone(), body.to_vec());
                    self.output.push(format!("updated  {path}"));
                }
                record.files.insert(path.clone(), sha256::hex(body));
            } else {
                self.removals.push(path.clone());
                record.files.remove(path);
                self.output.push(format!("removed  {path}"));
            }
        }
        for (path, body) in &now.templates {
            let hash = &now.files[path];
            if record.files.get(path) == Some(hash) {
                continue;
            }
            let old = record
                .files
                .get(path)
                .and_then(|hash| kits.iter().find(|k| k.files.get(path) == Some(hash)))
                .and_then(|k| k.templates.get(path));
            let previous = if let Some(old) = old {
                old.as_bytes().to_vec()
            } else {
                self.output.push(format!("template {path}: couldn't tell what's new; comparing the project file with the current template"));
                read_optional(&root.join(path))?.unwrap_or_default()
            };
            self.output.push(format!(
                "template {path}\n{}",
                template_diff(&now.id, path, &previous, body.as_bytes())?
            ));
            record.files.insert(path.clone(), hash.clone());
        }
        record.kit.clone_from(&now.id);
        Ok(())
    }

    /// Settings cleanup is a migration, not a kit file or record entry.
    fn remove_status_line(&mut self, root: &Path) -> Result<()> {
        let Some(bytes) = read_optional(&root.join(CLAUDE_SETTINGS))? else {
            return Ok(());
        };
        let Ok(mut settings) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            self.output.push(format!(
                "kept     {CLAUDE_SETTINGS} (not valid JSON, couldn't read it; left as is, so the lead guard was not added: fix it and run delegate upgrade again)"
            ));
            return Ok(());
        };
        let Some(object) = settings.as_object_mut() else {
            self.output.push(format!(
                "kept     {CLAUDE_SETTINGS} (not a JSON object, couldn't read it; left as is, so the lead guard was not added)"
            ));
            return Ok(());
        };
        let Some(line) = object.get("statusLine") else {
            return Ok(());
        };
        if line.get("command").and_then(serde_json::Value::as_str) != Some(STATUS_LINE) {
            self.output.push(format!(
                "kept     {CLAUDE_SETTINGS} status line (not the kit's command; left as is)"
            ));
            return Ok(());
        }
        object.remove("statusLine");
        self.writes.insert(
            CLAUDE_SETTINGS.into(),
            format!("{}\n", serde_json::to_string_pretty(&settings)?).into_bytes(),
        );
        self.output.push(format!(
            "removed  status line from {CLAUDE_SETTINGS} (tasks show above the prompt)"
        ));
        Ok(())
    }

    /// Add the lead guard's hooks (decision 015) to the settings as the status line cleanup
    /// left them. Settings it couldn't read were already reported by that cleanup.
    fn add_guard(&mut self, root: &Path) -> Result<()> {
        let existing = match self.writes.get(CLAUDE_SETTINGS) {
            Some(bytes) => Some(bytes.clone()),
            None => read_optional(&root.join(CLAUDE_SETTINGS))?,
        };
        match crate::guard::install(existing.as_deref()) {
            Install::Added(_) if check_parents(root, CLAUDE_SETTINGS).is_err() => {
                self.output.push(format!(
                    "kept     {CLAUDE_SETTINGS} (its folder is a symlink upgrade doesn't write through; the lead guard was not added)"
                ));
            }
            Install::Added(bytes) => {
                self.writes.insert(CLAUDE_SETTINGS.into(), bytes);
                self.output.push(crate::guard::ADDED.into());
            }
            Install::BadHooks => self.output.push(format!(
                "skipped  lead guard in {CLAUDE_SETTINGS} (couldn't read its \"hooks\": not the shape Claude Code reads; the guard was not added)"
            )),
            Install::Present | Install::NotJson | Install::NotObject => {}
        }
        Ok(())
    }

    fn take(&mut self, paths: &[String], now: &Kit, record: &mut Record) {
        for path in paths {
            if path == CLAUDE_SETTINGS {
                continue;
            }
            if let Some((_, body)) = PROJECT_FILES.iter().find(|(p, _)| *p == path) {
                self.writes.insert(path.clone(), body.as_bytes().to_vec());
                record.files.insert(path.clone(), now.files[path].clone());
                self.output.push(format!("updated  {path}"));
            } else {
                self.removals.push(path.clone());
                record.files.remove(path);
                self.output.push(format!("removed  {path}"));
            }
        }
    }
}

pub fn upgrade(dir: &Path, paths: &[String]) -> Result<()> {
    let root = PathBuf::from(
        git(dir, &["rev-parse", "--show-toplevel"])
            .context("not inside a git repository; enter your project and run delegate init")?,
    );
    let now = current();
    let mut kits = history()?;
    // A binary can be built with modified templates without a regenerated catalog.
    // Its own embedded kit is always known, independently of the catalog.
    if !kits.iter().any(|k| k.id == now.id) {
        kits.push(now.clone());
    }
    let existing = read_optional(&root.join(RECORD))?;
    let saved: Option<Record> = existing
        .as_deref()
        .map(|bytes| -> Result<Record> {
            toml::from_str(std::str::from_utf8(bytes)?)
                .context("invalid .delegate-kit.toml; repair the record and retry")
        })
        .transpose()?;
    if let Some(record) = &saved
        && !kits.iter().any(|k| k.id == record.kit)
    {
        bail!(
            "this delegate is older than the project's kit {}; rebuild it with:\n{REBUILD}",
            record.kit
        );
    }
    let all_paths: BTreeSet<_> = kits.iter().flat_map(|k| k.files.keys()).collect();
    let valid: Vec<_> = all_paths
        .iter()
        .filter(|p| !OWNED.contains(&p.as_str()))
        .map(|p| p.as_str())
        .collect();
    for path in paths {
        if path != CLAUDE_SETTINGS && !valid.contains(&path.as_str()) {
            bail!("{path} is not a kit file; kit paths: {}", valid.join(", "));
        }
    }
    let mut hashes = BTreeMap::new();
    for path in all_paths {
        if let Some(bytes) = read_optional(&root.join(path))? {
            hashes.insert(path.clone(), sha256::hex(&bytes));
        }
    }
    if saved.is_none() && hashes.is_empty() {
        bail!("this repository was never set up; run delegate init first");
    }
    // Rerunning init on a legacy project records only newly added files. Those
    // additions must not mask the older kit of the still-unrecorded files.
    let mut unrecorded: BTreeMap<_, _> = hashes
        .iter()
        .filter(|(path, _)| saved.as_ref().is_none_or(|r| !r.files.contains_key(*path)))
        .map(|(path, hash)| (path.clone(), hash.clone()))
        .collect();
    // A missing file can still identify its installed kit through HEAD. This
    // disambiguates partial records when the newly changed template is precisely
    // the one the user deleted; do not infer an older kit from unchanged peers.
    for path in &valid {
        if !hashes.contains_key(*path)
            && saved
                .as_ref()
                .is_none_or(|record| !record.files.contains_key(*path))
        {
            let output = Command::new("git")
                .current_dir(&root)
                .args(["show", &format!("HEAD:{path}")])
                .output()
                .context("reading installed template history")?;
            if output.status.success() {
                unrecorded.insert((*path).to_owned(), sha256::hex(&output.stdout));
            }
        }
    }
    let baseline = identify(&kits, &unrecorded);
    let mut record = saved.unwrap_or_else(|| Record {
        project: None,
        kit: baseline.map_or_else(|| now.id.clone(), |kit| kit.id.clone()),
        files: BTreeMap::new(),
    });
    let mut plan = Plan::default();
    if paths.is_empty() {
        if let Some(kit) = baseline {
            for (path, hash) in &kit.files {
                // Identifying a legacy kit does not prove an absent file was
                // installed: only the project's record or git history can do that.
                if !OWNED.contains(&path.as_str())
                    && !hashes.contains_key(path)
                    && !git_knows(&root, path)?
                {
                    continue;
                }
                record
                    .files
                    .entry(path.clone())
                    .or_insert_with(|| hash.clone());
            }
        }
        plan.full(&root, &kits, &now, &mut record, &hashes)?;
    } else {
        // A targeted upgrade has no side effects on other record entries.
        plan.take(paths, &now, &mut record);
    }
    if paths.is_empty() || paths.iter().any(|path| path == CLAUDE_SETTINGS) {
        plan.remove_status_line(&root)?;
        plan.add_guard(&root)?;
    }
    let serialized = toml::to_string(&record)?;
    let record_changed = existing.as_deref() != Some(serialized.as_bytes());
    let mut changed = record_changed;
    for path in plan.writes.keys().chain(&plan.removals) {
        check_parents(&root, path)?;
    }
    if record_changed {
        check_parents(&root, RECORD)?;
    }
    for (path, bytes) in &plan.writes {
        let dest = root.join(path);
        let same = read_optional(&dest)?.as_deref() == Some(bytes);
        let mode_wrong = EXECUTABLE.contains(&path.as_str())
            && dest.exists()
            && fs::metadata(&dest)?.permissions().mode() & 0o111 != 0o111;
        if !same || mode_wrong {
            write_file(&dest, bytes)?;
            if EXECUTABLE.contains(&path.as_str()) {
                fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
            }
            changed = true;
        }
    }
    for path in &plan.removals {
        let dest = root.join(path);
        if dest.exists() {
            fs::remove_file(&dest).with_context(|| {
                format!("remove {}; check permissions and retry", dest.display())
            })?;
            changed = true;
        }
    }
    if record_changed {
        write_file(&root.join(RECORD), serialized.as_bytes())?;
        plan.output.push(format!("updated  {RECORD}"));
    }
    for line in plan.output {
        println!("{line}");
    }
    if changed {
        println!("Review with git diff, then commit.");
    } else {
        println!("up to date");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Project(PathBuf);

    impl Project {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("delegate-upgrade-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            git(&root, &["init", "-q"]).unwrap();
            Self(root)
        }

        fn install(&self) -> Record {
            for (path, body) in PROJECT_FILES {
                write_file(&self.0.join(path), body.as_bytes()).unwrap();
                if EXECUTABLE.contains(path) {
                    fs::set_permissions(self.0.join(path), fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
            }
            let record = Record {
                project: Some(uuid::Uuid::new_v4().to_string()),
                kit: current().id,
                files: current().files,
            };
            write_file(
                &self.0.join(RECORD),
                toml::to_string(&record).unwrap().as_bytes(),
            )
            .unwrap();
            write_file(&self.0.join(".claude/settings.json"), b"{\"ours\": true}").unwrap();
            record
        }

        fn commit(&self) {
            git(&self.0, &["add", "-A"]).unwrap();
            git(
                &self.0,
                &[
                    "-c",
                    "user.name=Upgrade test",
                    "-c",
                    "user.email=upgrade@example.invalid",
                    "commit",
                    "-qm",
                    "Fixture state",
                ],
            )
            .unwrap();
        }

        fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
            PROJECT_FILES
                .iter()
                .map(|(p, _)| *p)
                .chain([RECORD, ".claude/settings.json"])
                .filter_map(|p| {
                    read_optional(&self.0.join(p))
                        .unwrap()
                        .map(|b| (p.into(), b))
                })
                .collect()
        }
    }

    /// Settings as a full upgrade leaves them: the lead guard's hooks added.
    fn with_guard(settings: &[u8]) -> Vec<u8> {
        match crate::guard::install(Some(settings)) {
            Install::Added(bytes) => bytes,
            other => panic!("guard not added: {other:?}"),
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn targeted_unrelated_upgrade_keeps_stock_status_line_byte_for_byte() {
        let project = Project::new();
        project.install();
        fs::write(
            project.0.join(CLAUDE_SETTINGS),
            b"{ \"statusLine\": {\"command\": \"delegate statusline\"}, \"ours\": true }\n",
        )
        .unwrap();
        let before = project.snapshot();
        upgrade(&project.0, &["evals/run".into()]).unwrap();
        assert_eq!(project.snapshot(), before);
    }

    #[test]
    fn targeted_settings_upgrade_removes_stock_line_without_recording_settings() {
        let project = Project::new();
        project.install();
        fs::write(
            project.0.join(CLAUDE_SETTINGS),
            b"{\"statusLine\": {\"type\": \"command\", \"command\": \"delegate statusline\"}, \"ours\": true}\n",
        )
        .unwrap();
        let mut expected = project.snapshot();
        expected.insert(CLAUDE_SETTINGS.into(), with_guard(b"{\"ours\": true}"));
        upgrade(&project.0, &[CLAUDE_SETTINGS.into()]).unwrap();
        assert_eq!(project.snapshot(), expected);
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert!(!record.files.contains_key(CLAUDE_SETTINGS));
        upgrade(&project.0, &[CLAUDE_SETTINGS.into()]).unwrap();
        assert_eq!(project.snapshot(), expected);
    }

    #[test]
    fn full_upgrade_removes_only_stock_status_line_and_is_idempotent() {
        let project = Project::new();
        project.install();
        let other = serde_json::json!({
            "permissions": {"allow": ["Bash(ls:*)"]},
            "env": {"SHOP_ENV": "dev"},
            "hooks": {"PreToolUse": []},
            "nested": [null, false, 42, {"statusLine": "keep"}]
        });
        let mut settings = other.clone();
        settings["statusLine"] = serde_json::json!({"command": STATUS_LINE, "type": "command"});
        fs::write(
            project.0.join(CLAUDE_SETTINGS),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        let mut expected = project.snapshot();
        expected.insert(
            CLAUDE_SETTINGS.into(),
            with_guard(&serde_json::to_vec(&other).unwrap()),
        );
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), expected);
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), expected);
    }

    #[test]
    fn malformed_hooks_do_not_claim_removed_status_line_was_left_alone() {
        let project = Project::new();
        project.install();
        for hooks in [serde_json::json!([]), serde_json::json!({"Stop": {}})] {
            let before = serde_json::json!({
                "statusLine": {"type": "command", "command": STATUS_LINE},
                "hooks": hooks,
                "ours": true,
            });
            fs::write(
                project.0.join(CLAUDE_SETTINGS),
                serde_json::to_vec(&before).unwrap(),
            )
            .unwrap();
            let mut plan = Plan::default();
            plan.remove_status_line(&project.0).unwrap();
            plan.add_guard(&project.0).unwrap();
            let after: serde_json::Value =
                serde_json::from_slice(&plan.writes[CLAUDE_SETTINGS]).unwrap();
            assert_eq!(after, serde_json::json!({"hooks": hooks, "ours": true}));
            assert_eq!(plan.output.len(), 2);
            assert!(plan.output[0].contains("removed  status line"));
            assert!(plan.output[1].contains(CLAUDE_SETTINGS));
            assert!(plan.output[1].contains("couldn't read its \"hooks\""));
            assert!(plan.output[1].contains("guard was not added"));
            assert!(!plan.output.iter().any(|line| line.contains("left as is")));
            assert!(!plan.output[1].starts_with("kept"));
            for paths in [vec![], vec![CLAUDE_SETTINGS.into()]] {
                upgrade(&project.0, &paths).unwrap();
                let saved: serde_json::Value =
                    serde_json::from_slice(&fs::read(project.0.join(CLAUDE_SETTINGS)).unwrap())
                        .unwrap();
                assert_eq!(saved, after);
                fs::write(
                    project.0.join(CLAUDE_SETTINGS),
                    serde_json::to_vec(&before).unwrap(),
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn status_line_cleanup_preserves_other_settings_byte_for_byte() {
        let project = Project::new();
        project.install();
        for (bytes, message) in [
            ("{ \"ours\": true }\n", None),
            (
                "{\"statusLine\": {\"command\": \"~/bin/my-line; delegate statusline\"}}",
                Some("status line"),
            ),
            (
                "{\"statusLine\": {\"command\": \"delegate statusline \"}}",
                Some("status line"),
            ),
            ("{\"statusLine\": {\"command\": 7}}", Some("status line")),
            ("{\"statusLine\": null}", Some("status line")),
            (
                "{\"statusLine\": \"delegate statusline\"}",
                Some("status line"),
            ),
            ("{invalid JSON\n", Some("settings.json")),
            ("[]\n", Some("settings.json")),
            ("null\n", Some("settings.json")),
        ] {
            fs::write(project.0.join(CLAUDE_SETTINGS), bytes).unwrap();
            let mut plan = Plan::default();
            plan.remove_status_line(&project.0).unwrap();
            assert!(plan.writes.is_empty());
            assert_eq!(plan.removals, Vec::<String>::new());
            if let Some(message) = message {
                assert_eq!(plan.output.len(), 1);
                assert!(plan.output[0].contains(message));
            } else {
                assert_eq!(plan.output, Vec::<String>::new());
            }
            // Readable settings only gain the guard; the rest stays byte-for-byte.
            let mut before = project.snapshot();
            if serde_json::from_str::<serde_json::Value>(bytes).is_ok_and(|v| v.is_object()) {
                before.insert(CLAUDE_SETTINGS.into(), with_guard(bytes.as_bytes()));
                fs::write(project.0.join(CLAUDE_SETTINGS), with_guard(bytes.as_bytes())).unwrap();
                upgrade(&project.0, &[]).unwrap();
                assert_eq!(project.snapshot(), before);
                fs::write(project.0.join(CLAUDE_SETTINGS), bytes).unwrap();
            }
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(project.snapshot(), before);
        }
        fs::remove_file(project.0.join(CLAUDE_SETTINGS)).unwrap();
        let mut before = project.snapshot();
        upgrade(&project.0, &[]).unwrap();
        before.insert(CLAUDE_SETTINGS.into(), with_guard(b"{}"));
        assert_eq!(project.snapshot(), before);
        fs::remove_file(project.0.join(CLAUDE_SETTINGS)).unwrap();
        let mut plan = Plan::default();
        plan.remove_status_line(&project.0).unwrap();
        assert_eq!(plan.output, Vec::<String>::new());
        assert!(plan.writes.is_empty());
    }

    #[test]
    fn targeted_upgrade_only_changes_requested_paths_and_their_record_entries() {
        let project = Project::new();
        let mut record = project.install();
        record.kit = history().unwrap()[0].id.clone();
        let edited = ".claude/skills/delegate/SKILL.md";
        let missing = ".claude/skills/evaluate/SKILL.md";
        record
            .files
            .insert(edited.into(), sha256::hex(b"old skill"));
        record
            .files
            .insert(missing.into(), sha256::hex(b"old evaluate"));
        write_file(
            &project.0.join(RECORD),
            toml::to_string(&record).unwrap().as_bytes(),
        )
        .unwrap();
        fs::write(project.0.join(edited), "our skill").unwrap();
        fs::remove_file(project.0.join(missing)).unwrap();
        let mut before = project.snapshot();
        upgrade(&project.0, &[edited.into(), missing.into()]).unwrap();
        for (path, body) in PROJECT_FILES {
            if [edited, missing].contains(path) {
                before.insert((*path).into(), body.as_bytes().to_vec());
                record
                    .files
                    .insert((*path).into(), sha256::hex(body.as_bytes()));
            }
        }
        before.insert(
            RECORD.into(),
            toml::to_string(&record).unwrap().into_bytes(),
        );
        assert_eq!(project.snapshot(), before);
        let after: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert_eq!(after, record); // kit and all unrelated entries are untouched
    }

    #[test]
    fn partial_init_records_still_infer_legacy_edits_and_deletions() {
        let project = Project::new();
        let mut partial = project.install();
        project.commit();
        partial.files.retain(|path, _| path == "evals/run");
        fs::write(project.0.join(RECORD), toml::to_string(&partial).unwrap()).unwrap();
        let missing = ".claude/skills/evaluate/SKILL.md";
        let edited = ".claude/skills/delegate/SKILL.md";
        fs::remove_file(project.0.join(missing)).unwrap();
        fs::write(project.0.join(edited), b"our skill").unwrap();
        upgrade(&project.0, &[]).unwrap();
        assert!(!project.0.join(missing).exists());
        assert_eq!(fs::read(project.0.join(edited)).unwrap(), b"our skill");
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert_eq!(record.files[missing], current().files[missing]);
        let before = project.snapshot();
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), before);
    }

    fn recordless_deletion_stays_missing(committed: bool) {
        for identifiable in [true, false] {
            let project = Project::new();
            project.install();
            fs::remove_file(project.0.join(RECORD)).unwrap();
            let missing = ".claude/skills/delegate-pane/hooks/hooks.json";
            let new = ".claude/skills/evaluate/SKILL.md";
            // Never installed in this project's history: unlike `missing`, this
            // file should be added, even when the other files identify a kit.
            fs::remove_file(project.0.join(new)).unwrap();
            if !identifiable {
                for (path, _) in PROJECT_FILES {
                    if !OWNED.contains(path) && *path != new {
                        fs::write(project.0.join(path), b"our edited kit file").unwrap();
                    }
                }
            }
            project.commit();
            fs::remove_file(project.0.join(missing)).unwrap();
            if committed {
                project.commit();
            }
            assert_eq!(
                identify(&history().unwrap(), &project_hashes(&project)).is_some(),
                identifiable
            );
            upgrade(&project.0, &[]).unwrap();
            assert!(!project.0.join(missing).exists());
            assert_eq!(
                fs::read(project.0.join(new)).unwrap(),
                PROJECT_FILES
                    .iter()
                    .find(|(p, _)| *p == new)
                    .unwrap()
                    .1
                    .as_bytes()
            );
            let before = project.snapshot();
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(project.snapshot(), before);
        }
    }

    fn project_hashes(project: &Project) -> BTreeMap<String, String> {
        project
            .snapshot()
            .into_iter()
            .map(|(path, bytes)| (path, sha256::hex(&bytes)))
            .collect()
    }

    #[test]
    fn recordless_uncommitted_deletion_stays_missing() {
        recordless_deletion_stays_missing(false);
    }

    #[test]
    fn recordless_committed_deletion_stays_missing() {
        recordless_deletion_stays_missing(true);
    }

    #[test]
    fn validation_errors_do_not_change_any_project_files() {
        let project = Project::new();
        let mut record = project.install();
        fs::write(project.0.join("evals/run"), "our runner").unwrap();
        let before = project.snapshot();
        let error = upgrade(&project.0, &["evals/run".into(), "AGENTS.md".into()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("kit paths:"));
        assert_eq!(project.snapshot(), before);
        record.kit = "unknown-newer-kit".into();
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        let before = project.snapshot();
        for paths in [vec![], vec!["evals/run".into()]] {
            let error = upgrade(&project.0, &paths).unwrap_err().to_string();
            assert!(error.contains(REBUILD));
            assert_eq!(project.snapshot(), before);
        }
        let empty = Project::new();
        assert!(
            upgrade(&empty.0, &[])
                .unwrap_err()
                .to_string()
                .contains("delegate init")
        );
        assert!(empty.snapshot().is_empty());
    }

    #[test]
    fn removed_files_are_deleted_only_if_pristine_and_missing_files_stay_missing() {
        let project = Project::new();
        let now = current();
        let mut old = now.clone();
        old.files.insert("obsolete.txt".into(), sha256::hex(b"old"));
        let mut hashes = old.files.clone();
        let mut record = Record {
            project: None,
            kit: old.id.clone(),
            files: old.files.clone(),
        };
        write_file(&project.0.join("evals/run"), b"unused").unwrap();
        hashes.remove("evals/run");
        let mut plan = Plan::default();
        plan.full(
            &project.0,
            &[old.clone(), now.clone()],
            &now,
            &mut record,
            &hashes,
        )
        .unwrap();
        assert_eq!(plan.removals, ["obsolete.txt"]);
        assert!(!plan.writes.contains_key("evals/run"));
        assert!(
            plan.output
                .iter()
                .any(|line| line.starts_with("missing  evals/run"))
        );
        assert!(!record.files.contains_key("obsolete.txt"));
        record
            .files
            .insert("obsolete.txt".into(), old.files["obsolete.txt"].clone());
        hashes.insert("obsolete.txt".into(), sha256::hex(b"edited"));
        let mut plan = Plan::default();
        plan.full(&project.0, &[old, now.clone()], &now, &mut record, &hashes)
            .unwrap();
        assert_eq!(plan.removals, Vec::<String>::new());
        assert_eq!(record.files["obsolete.txt"], sha256::hex(b"old"));
        assert!(
            plan.output
                .iter()
                .any(|line| line.starts_with("kept     obsolete.txt"))
        );
    }

    #[test]
    fn targeted_upgrade_cannot_overwrite_a_project_owned_link_target() {
        let project = Project::new();
        project.install();
        let own = fs::read(project.0.join("AGENTS.md")).unwrap();
        let skill = ".claude/skills/delegate/SKILL.md";
        fs::remove_file(project.0.join(skill)).unwrap();
        std::os::unix::fs::symlink("../../../AGENTS.md", project.0.join(skill)).unwrap();
        upgrade(&project.0, &[skill.into()]).unwrap();
        assert_eq!(fs::read(project.0.join("AGENTS.md")).unwrap(), own);
        assert!(
            !fs::symlink_metadata(project.0.join(skill))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn current_kit_is_newest() {
        let kits = history().unwrap();
        assert_eq!(
            kits.last().unwrap().id,
            current().id,
            "run scripts/kit-history before building"
        );
        assert_eq!(
            kits.last().unwrap().files,
            current().files,
            "run scripts/kit-history before building"
        );
    }

    #[test]
    fn legacy_matching_ignores_edits_but_requires_consistent_evidence() {
        let kits = history().unwrap();
        for kit in &kits {
            let mut hashes = kit.files.clone();
            hashes.insert(".claude/skills/delegate/SKILL.md".into(), "edited".into());
            let found = identify(&kits, &hashes).unwrap();
            for (path, hash) in &hashes {
                if !OWNED.contains(&path.as_str()) && hash != "edited" {
                    assert_eq!(found.files.get(path), Some(hash));
                }
            }
        }
        assert!(identify(&kits, &BTreeMap::new()).is_none());
    }
}
