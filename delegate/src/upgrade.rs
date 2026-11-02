//! Content-addressed kit upgrades and cleanup of the legacy kit status line.
//! Project-owned text is advice; only explicitly marked kit blocks are write targets.

#[path = "blocks.rs"]
mod blocks;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, String>,
}

impl Record {
    pub fn new() -> Self {
        Self {
            project: Some(uuid::Uuid::new_v4().to_string()),
            kit: current().id,
            files: BTreeMap::new(),
            blocks: BTreeMap::new(),
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

const BLOCK_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

fn outside_blocks(file: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    if BLOCK_FILES.contains(&file) {
        blocks::outside(file, bytes)
    } else {
        Ok(bytes.to_vec())
    }
}

fn template_blocks(kit: &Kit, file: &str) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut result = BTreeMap::new();
    if BLOCK_FILES.contains(&file)
        && let Some(body) = kit.templates.get(file)
    {
        for block in blocks::parse(file, body.as_bytes())? {
            result.insert(block.name, body.as_bytes()[block.range].to_vec());
        }
    }
    Ok(result)
}

pub(crate) fn installed_blocks() -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for file in BLOCK_FILES {
        for (name, body) in template_blocks(&current(), file)? {
            hashes.insert(format!("{file}#{name}"), sha256::hex(&body));
        }
    }
    Ok(hashes)
}

fn git_had_block(root: &Path, file: &str, name: &str) -> Result<bool> {
    let pattern = format!("^<!-- delegate:begin {name} -->$");
    let history = git(
        root,
        &[
            "log",
            "--all",
            "-1",
            "--format=%H",
            "-G",
            &pattern,
            "--",
            file,
        ],
    )
    .with_context(|| format!("inspect history of {file}#{name}; check git and retry"))?;
    Ok(!history.is_empty())
}

fn old_lead(kit: &Kit) -> Option<&[u8]> {
    lead_section(kit.templates.get("CLAUDE.md")?)
}

fn lead_section(body: &str) -> Option<&[u8]> {
    let heading = "## Claude's role in this project";
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.strip_suffix('\n').unwrap_or(line) == heading {
            return Some(&body.as_bytes()[offset..]);
        }
        offset += line.len();
    }
    None
}

#[derive(Debug, Default)]
struct Plan {
    writes: BTreeMap<String, Vec<u8>>,
    removals: Vec<String>,
    output: Vec<String>,
    adopted_lead: bool,
}

impl Plan {
    fn file_bytes(&self, root: &Path, file: &str) -> Result<Vec<u8>> {
        self.writes.get(file).map_or_else(
            || Ok(read_optional(&root.join(file))?.unwrap_or_default()),
            |body| Ok(body.clone()),
        )
    }

    fn take_block(
        &mut self,
        root: &Path,
        now: &Kit,
        record: &mut Record,
        file: &str,
        name: &str,
    ) -> Result<()> {
        let templates = template_blocks(now, file)?;
        let body = templates
            .get(name)
            .context("unknown block; choose a current kit block")?;
        let mut bytes = self.file_bytes(root, file)?;
        if let Some(block) = blocks::parse(file, &bytes)?
            .into_iter()
            .find(|block| block.name == name)
        {
            if bytes[block.range.clone()] == *body {
                record
                    .blocks
                    .insert(format!("{file}#{name}"), sha256::hex(body));
                return Ok(());
            }
            bytes.splice(block.range, body.iter().copied());
        } else {
            blocks::append(&mut bytes, body);
        }
        self.writes.insert(file.into(), bytes);
        record
            .blocks
            .insert(format!("{file}#{name}"), sha256::hex(body));
        self.output.push(format!("updated  {file}#{name}"));
        Ok(())
    }

    /// Owned files can lag the record. Prefer the fullest matching historical
    /// section, with later history entries breaking ties.
    fn adopt_lead(&mut self, root: &Path, kits: &[Kit], now: &Kit, block: &[u8]) -> Result<bool> {
        let source = self.file_bytes(root, "CLAUDE.md")?;
        if !source
            .split(|b| *b == b'\n')
            .any(|line| line == b"## Claude's role in this project")
        {
            return Ok(false);
        }
        let mut candidates: Vec<_> = kits
            .iter()
            .enumerate()
            .filter_map(|(index, kit)| old_lead(kit).map(|section| (index, section)))
            .collect();
        candidates.sort_by_key(|(index, section)| {
            (section.split_inclusive(|b| *b == b'\n').count(), *index)
        });
        for (_, section) in candidates.into_iter().rev() {
            if let Some((kept, moved)) = blocks::adopt(&source, section) {
                self.adopted_lead = true;
                let mut dest = self.file_bytes(root, "AGENTS.md")?;
                blocks::append(&mut dest, block);
                if !moved.is_empty() {
                    if !dest.ends_with(b"\n") {
                        dest.push(b'\n');
                    }
                    dest.extend_from_slice(&moved);
                }
                self.writes.insert("CLAUDE.md".into(), kept);
                self.writes.insert("AGENTS.md".into(), dest);
                self.output
                    .push("moved    CLAUDE.md#lead -> AGENTS.md#lead".into());
                if !moved.is_empty() {
                    self.output.push(format!(
                        "moved project lines after AGENTS.md#lead:\n{}",
                        String::from_utf8_lossy(&moved)
                    ));
                }
                return Ok(true);
            }
        }
        let comparison = comparison(&now.id, "current-block", "AGENTS.md", block)?;
        self.output.push(format!("kept     CLAUDE.md#lead (old section did not match; remove the old section from CLAUDE.md, then run delegate upgrade again or delegate upgrade AGENTS.md#lead; to add the lead block to AGENTS.md by hand, use '{}')", comparison.display()));
        // Handled, but deliberately without writing either file.
        Ok(true)
    }

    fn managed(&mut self, root: &Path, kits: &[Kit], now: &Kit, record: &mut Record) -> Result<()> {
        for file in BLOCK_FILES {
            for (name, body) in template_blocks(now, file)? {
                let key = format!("{file}#{name}");
                let hash = sha256::hex(&body);
                let mut bytes = self.file_bytes(root, file)?;
                if let Some(block) = blocks::parse(file, &bytes)?
                    .into_iter()
                    .find(|block| block.name == name)
                {
                    let actual = &bytes[block.range.clone()];
                    let mut pristine = false;
                    for kit in kits {
                        if template_blocks(kit, file)?
                            .get(&name)
                            .is_some_and(|old| old == actual)
                        {
                            pristine = true;
                            break;
                        }
                    }
                    if pristine {
                        if actual != body {
                            bytes.splice(block.range, body.iter().copied());
                            self.writes.insert((*file).into(), bytes);
                            self.output.push(format!("updated  {key}"));
                        }
                        record.blocks.insert(key, hash);
                    } else {
                        let old = comparison(&now.id, "project-block", file, actual)?;
                        let new = comparison(&now.id, "current-block", file, &body)?;
                        self.output.push(format!("kept     {key} (edited; compare with git diff --no-index '{}' '{}'; take it with delegate upgrade {key})", old.display(), new.display()));
                        // Even an edited, previously unrecorded block was installed.
                        record
                            .blocks
                            .entry(key)
                            .or_insert_with(|| sha256::hex(actual));
                    }
                } else if record.blocks.contains_key(&key) || git_had_block(root, file, &name)? {
                    self.output.push(format!(
                        "missing  {key}; restore with delegate upgrade {key}"
                    ));
                    record.blocks.entry(key).or_insert(hash);
                } else if *file == "AGENTS.md"
                    && name == "lead"
                    && self.adopt_lead(root, kits, now, &body)?
                {
                    if self.writes.contains_key("AGENTS.md") {
                        record.blocks.insert(key, hash);
                    }
                } else {
                    blocks::append(&mut bytes, &body);
                    self.writes.insert((*file).into(), bytes);
                    record.blocks.insert(key.clone(), hash);
                    self.output.push(format!("added    {key}"));
                }
            }
        }
        Ok(())
    }

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
            let mut previous = if let Some(old) = old {
                old.as_bytes().to_vec()
            } else {
                self.file_bytes(root, path)?
            };
            if path == "CLAUDE.md"
                && self.adopted_lead
                && let Some(section) = old.and_then(|body| lead_section(body))
            {
                // The template recorded as seen may differ from the section
                // actually adopted. Neither removal is an outstanding hand merge.
                previous.truncate(previous.len() - section.len());
            }
            let previous = outside_blocks(path, &previous)?;
            let next = outside_blocks(path, body.as_bytes())?;
            let nonblank = |bytes: &[u8]| -> Vec<Vec<u8>> {
                bytes
                    .split(|b| *b == b'\n')
                    .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
                    .map(<[u8]>::to_vec)
                    .collect()
            };
            if nonblank(&previous) != nonblank(&next) {
                if old.is_none() {
                    self.output.push(format!("template {path}: couldn't tell what's new; comparing the project file with the current template"));
                }
                self.output.push(format!(
                    "template {path}\n{}",
                    template_diff(&now.id, path, &previous, &next)?
                ));
            }
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
                "kept     {CLAUDE_SETTINGS} (not valid JSON; left as is, fix it and run delegate upgrade again)"
            ));
            return Ok(());
        };
        let Some(object) = settings.as_object_mut() else {
            self.output.push(format!(
                "kept     {CLAUDE_SETTINGS} (not a JSON object; left as is)"
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
    // Validate both managed files before planning any writes, including targeted
    // upgrades of unrelated files and the settings migration.
    for file in BLOCK_FILES {
        blocks::parse(file, &read_optional(&root.join(file))?.unwrap_or_default())?;
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
        if let Some((file, name)) = path.split_once('#') {
            let available = template_blocks(&now, file)?;
            if !available.contains_key(name) {
                bail!(
                    "unknown block {path}; {file} blocks: {}; choose one of these blocks",
                    available.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        } else if path != CLAUDE_SETTINGS && !valid.contains(&path.as_str()) {
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
        blocks: BTreeMap::new(),
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
        plan.managed(&root, &kits, &now, &mut record)?;
        plan.full(&root, &kits, &now, &mut record, &hashes)?;
    } else {
        // A targeted upgrade has no side effects on other record entries.
        for path in paths {
            if let Some((file, name)) = path.split_once('#') {
                plan.take_block(&root, &now, &mut record, file, name)?;
            } else {
                plan.take(std::slice::from_ref(path), &now, &mut record);
            }
        }
    }
    if paths.is_empty() || paths.iter().any(|path| path == CLAUDE_SETTINGS) {
        plan.remove_status_line(&root)?;
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
                blocks: installed_blocks().unwrap(),
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
        expected.insert(CLAUDE_SETTINGS.into(), b"{\n  \"ours\": true\n}\n".to_vec());
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
            format!("{}\n", serde_json::to_string_pretty(&other).unwrap()).into_bytes(),
        );
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), expected);
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), expected);
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
            let before = project.snapshot();
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(project.snapshot(), before);
        }
        fs::remove_file(project.0.join(CLAUDE_SETTINGS)).unwrap();
        let before = project.snapshot();
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), before);
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
            blocks: BTreeMap::new(),
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
    fn malformed_blocks_refuse_before_any_file_or_record_is_written() {
        let malformed = [
            "<!-- delegate:begin x -->\n<!-- delegate:end x -->\n<!-- delegate:begin x -->\n<!-- delegate:end x -->\n",
            "<!-- delegate:begin x -->\n<!-- delegate:begin y -->\n<!-- delegate:end y -->\n<!-- delegate:end x -->\n",
            "<!-- delegate:end x -->\n",
            "<!-- delegate:begin x -->\n",
            "<!-- delegate:begin x -->\n<!-- delegate:end y -->\n",
        ];
        for file in BLOCK_FILES {
            for text in malformed {
                for targeted in [false, true] {
                    let project = Project::new();
                    project.install();
                    fs::write(project.0.join(file), text).unwrap();
                    fs::write(
                        project.0.join(CLAUDE_SETTINGS),
                        b"{\"statusLine\":{\"command\":\"delegate statusline\"}}",
                    )
                    .unwrap();
                    fs::remove_file(project.0.join("evals/run")).unwrap();
                    let before = project.snapshot();
                    let paths = if targeted {
                        vec!["evals/run".into()]
                    } else {
                        vec![]
                    };
                    let error = upgrade(&project.0, &paths).unwrap_err().to_string();
                    assert!(error.contains(&format!("{file}:")), "{error}");
                    assert_eq!(project.snapshot(), before);
                }
            }
        }
    }

    fn legacy_project(kit: &Kit) -> Project {
        let project = Project::new();
        let mut record = project.install();
        for (file, template) in &kit.templates {
            fs::write(project.0.join(file), template).unwrap();
        }
        record.kit.clone_from(&kit.id);
        record.files.clone_from(&kit.files);
        record.blocks.clear();
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        project
    }

    #[test]
    fn adoption_moves_project_lines_and_is_idempotent() {
        let kits = history().unwrap();
        let old = kits
            .iter()
            .rev()
            .find(|kit| old_lead(kit).is_some())
            .unwrap();
        let project = legacy_project(old);
        let before = fs::read(project.0.join("AGENTS.md")).unwrap();
        let source = old.templates["CLAUDE.md"].replace(
            "- End a design discussion",
            "- Our project rule.\n- End a design discussion",
        ) + "\nProject suffix.\n";
        fs::write(project.0.join("CLAUDE.md"), source).unwrap();
        upgrade(&project.0, &[]).unwrap();
        let agents = fs::read(project.0.join("AGENTS.md")).unwrap();
        let mut expected = before;
        blocks::append(
            &mut expected,
            &template_blocks(&current(), "AGENTS.md").unwrap()["lead"],
        );
        expected.extend_from_slice(b"- Our project rule.\n");
        assert_eq!(agents, expected);
        let prefix = old.templates["CLAUDE.md"]
            .split("## Claude's role in this project")
            .next()
            .unwrap();
        assert_eq!(
            fs::read_to_string(project.0.join("CLAUDE.md")).unwrap(),
            format!("{prefix}\nProject suffix.\n")
        );
        let before = project.snapshot();
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(project.snapshot(), before);
    }

    #[test]
    fn changed_shared_kit_line_refuses_adoption() {
        let kits = history().unwrap();
        let old = kits
            .iter()
            .rev()
            .find(|kit| old_lead(kit).is_some())
            .unwrap();
        {
            let line = "- End a design discussion with `/distill`: record decisions, update `PLAN.md`, nothing else.";
            assert!(old.templates["CLAUDE.md"].contains(line));
            let project = legacy_project(old);
            fs::write(
                project.0.join("CLAUDE.md"),
                old.templates["CLAUDE.md"].replace(line, &format!("{line} edited")),
            )
            .unwrap();
            let agents = fs::read(project.0.join("AGENTS.md")).unwrap();
            let claude = fs::read(project.0.join("CLAUDE.md")).unwrap();
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(fs::read(project.0.join("AGENTS.md")).unwrap(), agents);
            assert_eq!(fs::read(project.0.join("CLAUDE.md")).unwrap(), claude);
            let before = project.snapshot();
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(project.snapshot(), before);
        }
    }

    #[test]
    fn edited_wave_line_moves_as_project_text_even_with_newest_record() {
        let kits = history().unwrap();
        let line = "- Split plans into waves of tasks touching different files, and start each wave together.";
        let old = kits
            .iter()
            .find(|kit| {
                kit.templates
                    .get("CLAUDE.md")
                    .is_some_and(|body| body.contains(line))
            })
            .unwrap();
        let project = legacy_project(old);
        let mut record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        record.kit = current().id;
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        let edited = format!("{line} edited\n");
        fs::write(
            project.0.join("CLAUDE.md"),
            old.templates["CLAUDE.md"].replace(&format!("{line}\n"), &edited),
        )
        .unwrap();
        let mut plan = Plan::default();
        plan.managed(&project.0, &kits, &current(), &mut record)
            .unwrap();
        assert!(plan.writes["AGENTS.md"].ends_with(edited.as_bytes()));
        assert!(plan.output.iter().any(|line| line.contains(&edited)));
        assert!(
            !String::from_utf8_lossy(&plan.writes["CLAUDE.md"])
                .contains("## Claude's role in this project")
        );
    }

    #[test]
    fn adoption_prefers_most_lines_then_later_history_entry() {
        let project = Project::new();
        let heading = "## Claude's role in this project\n";
        let sections = [
            format!("{heading}one\ntwo\n"),
            format!("{heading}one\n"),
            format!("{heading}one\nthree\n"),
        ];
        let kits: Vec<_> = sections
            .iter()
            .enumerate()
            .map(|(index, section)| Kit {
                id: index.to_string(),
                files: BTreeMap::new(),
                templates: BTreeMap::from([("CLAUDE.md".into(), section.clone())]),
            })
            .collect();
        fs::write(
            project.0.join("CLAUDE.md"),
            format!("{heading}one\ntwo\nthree\n"),
        )
        .unwrap();
        let mut plan = Plan::default();
        let body = &template_blocks(&current(), "AGENTS.md").unwrap()["lead"];
        assert!(
            plan.adopt_lead(&project.0, &kits, &current(), body)
                .unwrap()
        );
        // The last section ties for longest: 'two' is the only project line.
        let mut expected = body.clone();
        expected.extend_from_slice(b"two\n");
        assert_eq!(plan.writes["AGENTS.md"], expected);
    }

    #[test]
    fn adoption_diffs_exclude_the_moved_section_and_blank_separators() {
        let kits = history().unwrap();
        let old = kits
            .iter()
            .rev()
            .find(|kit| old_lead(kit).is_some())
            .unwrap();
        for recorded in [true, false] {
            let project = legacy_project(old);
            let mut record = if recorded {
                toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap()
            } else {
                Record::new()
            };
            let mut plan = Plan::default();
            plan.managed(&project.0, &kits, &current(), &mut record)
                .unwrap();
            plan.full(&project.0, &kits, &current(), &mut record, &old.files)
                .unwrap();
            assert!(
                !plan
                    .output
                    .iter()
                    .any(|line| line.starts_with("template AGENTS.md")
                        || line.starts_with("template CLAUDE.md")),
                "{:?}",
                plan.output
            );
        }
    }

    #[test]
    fn adoption_does_not_report_a_different_recorded_section_as_stale() {
        let kits = history().unwrap();
        let shorter = kits.iter().find(|kit| old_lead(kit).is_some()).unwrap();
        let fuller = kits
            .iter()
            .max_by_key(|kit| old_lead(kit).map_or(0, <[u8]>::len))
            .unwrap();
        let project = legacy_project(fuller);
        let mut record = Record::new();
        record.files.clone_from(&shorter.files);
        let mut plan = Plan::default();
        plan.managed(&project.0, &kits, &current(), &mut record)
            .unwrap();
        plan.full(&project.0, &kits, &current(), &mut record, &fuller.files)
            .unwrap();
        assert!(
            !plan
                .output
                .iter()
                .any(|line| line.starts_with("template CLAUDE.md")),
            "{:?}",
            plan.output
        );
    }

    #[test]
    fn equal_targeted_block_is_silent_and_does_not_write() {
        let project = Project::new();
        let mut record = project.install();
        let before = project.snapshot();
        let mut plan = Plan::default();
        plan.take_block(&project.0, &current(), &mut record, "AGENTS.md", "lead")
            .unwrap();
        assert_eq!(plan.output, Vec::<String>::new());
        assert!(plan.writes.is_empty());
        upgrade(&project.0, &["AGENTS.md#lead".into()]).unwrap();
        assert_eq!(project.snapshot(), before);
    }

    #[test]
    fn appended_block_starts_empty_or_missing_file_without_separator() {
        for missing in [true, false] {
            let project = Project::new();
            let mut record = project.install();
            if missing {
                fs::remove_file(project.0.join("AGENTS.md")).unwrap();
            } else {
                fs::write(project.0.join("AGENTS.md"), "").unwrap();
            }
            let mut plan = Plan::default();
            plan.take_block(&project.0, &current(), &mut record, "AGENTS.md", "lead")
                .unwrap();
            assert_eq!(
                plan.writes["AGENTS.md"],
                template_blocks(&current(), "AGENTS.md").unwrap()["lead"]
            );
        }
    }

    #[test]
    fn deleted_block_stays_deleted_even_without_recorded_hash() {
        for committed in [false, true] {
            let project = Project::new();
            let mut record = project.install();
            project.commit();
            record.blocks.clear();
            fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
            let bytes = fs::read(project.0.join("AGENTS.md")).unwrap();
            let outside = blocks::outside("AGENTS.md", &bytes).unwrap();
            fs::write(project.0.join("AGENTS.md"), &outside).unwrap();
            if committed {
                project.commit();
            }
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(fs::read(project.0.join("AGENTS.md")).unwrap(), outside);
            let before = project.snapshot();
            upgrade(&project.0, &[]).unwrap();
            assert_eq!(project.snapshot(), before);
            upgrade(&project.0, &["AGENTS.md#lead".into()]).unwrap();
            assert_eq!(
                blocks::parse("AGENTS.md", &fs::read(project.0.join("AGENTS.md")).unwrap())
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn recordless_adoption_tries_newest_matching_section_first() {
        let kits = history().unwrap();
        let old = kits
            .iter()
            .rev()
            .find(|kit| old_lead(kit).is_some())
            .unwrap();
        let project = legacy_project(old);
        fs::remove_file(project.0.join(RECORD)).unwrap();
        let mut plan = Plan::default();
        let body = &template_blocks(&current(), "AGENTS.md").unwrap()["lead"];
        assert!(
            plan.adopt_lead(&project.0, &kits, &current(), body)
                .unwrap()
        );
        assert!(plan.writes.contains_key("AGENTS.md"));
        assert!(!String::from_utf8_lossy(&plan.writes["AGENTS.md"]).contains("Claude is the lead"));
    }

    #[test]
    fn review_adoption_still_possible_after_hand_fix_of_refused_section() {
        // Review evidence: a refused adoption must stay retryable once the
        // project restores the old kit lines; the first run rewrote record.kit.
        let kits = history().unwrap();
        let old = kits
            .iter()
            .rev()
            .find(|kit| old_lead(kit).is_some())
            .unwrap();
        let project = legacy_project(old);
        let line = "- End a design discussion with `/distill`: record decisions, update `PLAN.md`, nothing else.";
        fs::write(
            project.0.join("CLAUDE.md"),
            old.templates["CLAUDE.md"].replace(line, &format!("{line} edited")),
        )
        .unwrap();
        upgrade(&project.0, &[]).unwrap();
        fs::write(project.0.join("CLAUDE.md"), &old.templates["CLAUDE.md"]).unwrap();
        upgrade(&project.0, &[]).unwrap();
        let agents = fs::read(project.0.join("AGENTS.md")).unwrap();
        assert_eq!(blocks::parse("AGENTS.md", &agents).unwrap().len(), 1);
    }

    #[test]
    fn review_lead_block_added_when_claude_md_never_had_the_section() {
        // Review evidence: never recorded and absent, with no old section to
        // adopt (project wrote its own CLAUDE.md before init) -> appended.
        let project = Project::new();
        let mut record = project.install();
        record.blocks.clear();
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        let bytes = fs::read(project.0.join("AGENTS.md")).unwrap();
        fs::write(
            project.0.join("AGENTS.md"),
            blocks::outside("AGENTS.md", &bytes).unwrap(),
        )
        .unwrap();
        fs::write(
            project.0.join("CLAUDE.md"),
            "@AGENTS.md\n\nOur own notes.\n",
        )
        .unwrap();
        upgrade(&project.0, &[]).unwrap();
        let agents = fs::read(project.0.join("AGENTS.md")).unwrap();
        assert_eq!(blocks::parse("AGENTS.md", &agents).unwrap().len(), 1);
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
