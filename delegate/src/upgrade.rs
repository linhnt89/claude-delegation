//! Content-addressed kit upgrades and cleanup of the legacy kit status line.
//! Project-owned text is advice; only explicitly marked kit blocks are write targets.

#[path = "blocks.rs"]
mod blocks;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink as create_symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::init::{CLAUDE_SETTINGS, EXECUTABLE, PROJECT_FILES, PROJECT_LINKS, STATUS_LINE};
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
                .chain(PROJECT_LINKS.iter())
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

/// Read a kit entry without following links or treating directories as files.
fn entry_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            use std::os::unix::ffi::OsStrExt;
            Ok(Some(fs::read_link(path)?.as_os_str().as_bytes().to_vec()))
        }
        Ok(meta) if meta.is_dir() => Ok(None),
        Ok(_) => read_optional(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("inspect {}; check permissions and retry", path.display())),
    }
}

fn tree_hashes(root: &Path, path: &str) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for entry in fs::read_dir(root.join(path))? {
        let entry = entry?;
        let rel = format!("{path}/{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            hashes.extend(tree_hashes(root, &rel)?);
        } else if let Some(bytes) = entry_bytes(&entry.path())? {
            hashes.insert(rel, sha256::hex(&bytes));
        }
    }
    Ok(hashes)
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

/// Explain only differences, never suggest taking unchanged files to fix extras.
fn collision_advice(
    root: &Path,
    directory: &str,
    expected: &BTreeMap<String, String>,
    now: &Kit,
) -> Result<Vec<String>> {
    let actual = tree_hashes(root, directory)?;
    let mut advice = Vec::new();
    for (path, hash) in expected {
        if actual.get(path) == Some(hash) {
            continue;
        }
        let body = PROJECT_FILES
            .iter()
            .find(|(p, _)| *p == path)
            .context("missing kit collision file")?
            .1;
        let compare = comparison(&now.id, "current", path, body.as_bytes())?;
        if actual.contains_key(path) {
            advice.push(format!("edited {path}; compare: git diff --no-index '{path}' '{}'; take with delegate upgrade {path}", compare.display()));
        } else {
            advice.push(format!(
                "missing {path}; compare kit file '{}'; restore with delegate upgrade {path}",
                compare.display()
            ));
        }
    }
    for path in actual.keys().filter(|p| !expected.contains_key(*p)) {
        advice.push(format!(
            "extra team file {path}; move {path} aside manually before retrying the layout take"
        ));
    }
    Ok(advice)
}

/// Enforce traversal safety before any planning can hide a symlink ancestor as
/// an obstruction. Regular-file obstructions are reported by the skill planner.
fn check_skill_ancestry(root: &Path, path: &str) -> Result<()> {
    let parent = Path::new(path).parent().context("kit path has no parent")?;
    for ancestor in parent.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(root.join(ancestor)) {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Use the existing refusal and advice, without inspecting children.
                return check_parents(root, path);
            }
            Ok(meta) if !meta.is_dir() => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect {}; check permissions and retry",
                        ancestor.display()
                    )
                });
            }
        }
    }
    Ok(())
}

/// Detect a team-owned obstruction before inspecting children or planning writes.
fn skill_obstruction(root: &Path, directory: &str) -> Result<Option<String>> {
    for ancestor in Path::new(directory)
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(root.join(ancestor)) {
            Ok(meta) if !meta.is_dir() => {
                return Ok(Some(ancestor.to_string_lossy().into_owned()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect {}; check permissions and retry",
                        ancestor.display()
                    )
                });
            }
        }
    }
    Ok(None)
}

fn established_skill_layout(
    root: &Path,
    record: Option<&Record>,
    link: &str,
    target: &str,
) -> bool {
    record.is_some_and(|r| r.files.contains_key(link))
        || fs::read_link(root.join(link)).ok().as_deref().map(Path::as_os_str)
            == Some(std::ffi::OsStr::new(target))
}

fn kept_skill_link(root: &Path, link: &str, target: &str) -> Result<String> {
    let meta = fs::symlink_metadata(root.join(link))
        .with_context(|| format!("inspect {link}; check permissions and retry"))?;
    let actual = if meta.file_type().is_symlink() {
        format!(
            "symlink target '{}'",
            fs::read_link(root.join(link))?.display()
        )
    } else if meta.is_file() {
        "regular file, not a symlink (even if its bytes equal the target)".into()
    } else {
        "non-symlink entry".into()
    };
    Ok(format!(
        "kept     {link} (edited; actual {actual}; expected symlink target '{target}'; take the kit's layout with delegate upgrade {link})"
    ))
}

#[derive(Debug, Default)]
struct Plan {
    writes: BTreeMap<String, Vec<u8>>,
    removals: Vec<String>,
    output: Vec<String>,
    adopted_lead: bool,
    links: BTreeMap<String, String>,
    folders: Vec<String>,
    skill_paths: BTreeSet<String>,
}

impl Plan {
    /// Plan each skill as a unit before the ordinary per-file upgrade.
    fn skills(
        &mut self,
        root: &Path,
        kits: &[Kit],
        now: &Kit,
        record: &mut Record,
        take: Option<&str>,
    ) -> Result<()> {
        for (link, target) in PROJECT_LINKS {
            if take.is_some_and(|path| path != *link) {
                continue;
            }
            let name = link.rsplit('/').next().context("kit link has no name")?;
            let canonical = format!(".agents/skills/{name}");
            let old_prefix = format!("{link}/");
            let new_prefix = format!("{canonical}/");
            if let Some(obstruction) = skill_obstruction(root, &canonical)? {
                self.skill_paths.extend(
                    now.files
                        .keys()
                        .filter(|p| p.starts_with(&new_prefix) || *p == link)
                        .cloned(),
                );
                self.output.push(format!("kept     {link} and {canonical} (collision; {obstruction} is not a directory; move {obstruction} aside manually before retrying delegate upgrade {link})"));
                continue;
            }
            let parent = Path::new(link).parent().context("kit link has no parent")?;
            if let Some(obstruction) = skill_obstruction(root, &parent.to_string_lossy())? {
                self.skill_paths.extend(
                    kits.iter()
                        .flat_map(|kit| kit.files.keys())
                        .filter(|p| {
                            p.starts_with(&old_prefix) || p.starts_with(&new_prefix) || *p == link
                        })
                        .cloned(),
                );
                self.output.push(format!("kept     {link} ({obstruction} is not a directory; move {obstruction} aside manually before retrying delegate upgrade {link})"));
                continue;
            }
            let metadata = fs::symlink_metadata(root.join(link));
            let old_folder = metadata.as_ref().is_ok_and(fs::Metadata::is_dir);
            let established = established_skill_layout(root, Some(record), link, target);
            if !established && metadata.as_ref().is_ok_and(|m| !m.is_dir()) && take.is_none() {
                // Do not introduce a competing canonical tree. An already-existing
                // canonical tree still receives normal per-file edit/deletion handling.
                let canonical_absent = fs::symlink_metadata(root.join(&canonical)).is_err();
                self.skill_paths.extend(
                    kits.iter()
                        .flat_map(|kit| kit.files.keys())
                        .filter(|p| {
                            p.starts_with(&old_prefix)
                                || (canonical_absent && p.starts_with(&new_prefix))
                                || *p == link
                        })
                        .cloned(),
                );
                self.output.push(kept_skill_link(root, link, target)?);
                continue;
            }
            if old_folder && record.files.contains_key(*link) && take.is_none() {
                // Comparing a directory to a link target file is not useful advice.
                self.skill_paths.insert((*link).into());
                self.output.push(format!("kept     {link} (link replaced by a folder; inspect {link} and {canonical}; take the kit's layout with delegate upgrade {link})"));
                continue;
            }
            let deleted_legacy = !established
                && fs::symlink_metadata(root.join(link)).is_err()
                && (record.files.keys().any(|p| p.starts_with(&old_prefix))
                    // A partial record need not contain the deleted skill. Git
                    // history of its children still proves the old installation;
                    // the trailing slash excludes the new-layout leaf link.
                    || git_knows(root, &old_prefix)?);
            if take.is_some() && !old_folder {
                // An absent canonical tree needs installation, but an existing one
                // belongs to the team: restoring a link must not change any byte.
                if fs::symlink_metadata(root.join(&canonical)).is_err() {
                    for (path, body) in PROJECT_FILES
                        .iter()
                        .filter(|(p, _)| p.starts_with(&new_prefix))
                    {
                        self.writes.insert((*path).into(), body.as_bytes().to_vec());
                        record
                            .files
                            .insert((*path).into(), now.files[*path].clone());
                        self.output.push(format!("added    {path}"));
                    }
                } else {
                    for path in now.files.keys().filter(|p| p.starts_with(&new_prefix)) {
                        if entry_bytes(&root.join(path))?.is_none() {
                            self.output.push(format!(
                                "missing  {path}; restore with delegate upgrade {path}"
                            ));
                        }
                    }
                }
                record.files.retain(|p, _| !p.starts_with(&old_prefix));
                if fs::read_link(root.join(link)).ok().as_deref().map(Path::as_os_str)
                    != Some(std::ffi::OsStr::new(target))
                {
                    self.links.insert((*link).into(), (*target).into());
                    self.output.push(format!("updated  {link}"));
                }
                record
                    .files
                    .insert((*link).into(), now.files[*link].clone());
                continue;
            }
            if old_folder || deleted_legacy || take.is_some() {
                for kit in kits {
                    self.skill_paths.extend(
                        kit.files
                            .keys()
                            .filter(|p| {
                                p.starts_with(&old_prefix)
                                    || p.starts_with(&new_prefix)
                                    || *p == link
                            })
                            .cloned(),
                    );
                }
                if deleted_legacy && take.is_none() {
                    self.output.push(format!(
                        "missing  {link}; restore with delegate upgrade {link}"
                    ));
                    continue;
                }
                // Only a real legacy migration needs to inspect the destination.
                let expected: BTreeMap<_, _> = now
                    .files
                    .iter()
                    .filter(|(p, _)| p.starts_with(&new_prefix))
                    .map(|(p, h)| (p.clone(), h.clone()))
                    .collect();
                let mut collisions = Vec::new();
                if let Ok(meta) = fs::symlink_metadata(root.join(&canonical)) {
                    if meta.is_dir() {
                        collisions.extend(collision_advice(root, &canonical, &expected, now)?);
                    } else {
                        collisions.push(format!("{canonical} is not a directory; move {canonical} aside manually before retrying delegate upgrade {link}"));
                    }
                }
                let mut source_extras = Vec::new();
                if old_folder {
                    let actual = tree_hashes(root, link)?;
                    for path in actual.keys() {
                        let canonical_path = path.replacen(&old_prefix, &new_prefix, 1);
                        if !expected.contains_key(&canonical_path) {
                            source_extras.push(format!("extra team file {path}; move {path} aside manually before retrying delegate upgrade {link}"));
                        }
                    }
                }
                if !collisions.is_empty() {
                    collisions.extend(source_extras.iter().cloned());
                    self.output.push(format!(
                        "kept     {link} and {canonical} (collision; {})",
                        collisions.join("; ")
                    ));
                    continue;
                }
                let pristine = if old_folder {
                    let actual = tree_hashes(root, link)?;
                    kits.iter().any(|kit| {
                        let mut expected: BTreeMap<_, _> = kit
                            .files
                            .iter()
                            .filter(|(p, _)| p.starts_with(&old_prefix))
                            .map(|(p, h)| (p.clone(), h.clone()))
                            .collect();
                        // Recorded ownership wins over historical inference: a team
                        // may deliberately pin text from a different historical kit.
                        expected.extend(
                            record
                                .files
                                .iter()
                                .filter(|(p, _)| p.starts_with(&old_prefix))
                                .map(|(p, h)| (p.clone(), h.clone())),
                        );
                        !expected.is_empty() && actual == expected
                    })
                } else {
                    !deleted_legacy
                };
                if (take.is_none() && !pristine) || !source_extras.is_empty() {
                    let compare = comparison(
                        &now.id,
                        "current",
                        &format!("{canonical}/SKILL.md"),
                        PROJECT_FILES
                            .iter()
                            .find(|(p, _)| *p == format!("{canonical}/SKILL.md"))
                            .context("missing kit skill")?
                            .1
                            .as_bytes(),
                    )?;
                    let source = format!("{link}/SKILL.md");
                    let inspection = if entry_bytes(&root.join(&source))?.is_some() {
                        format!(
                            "compare: git diff --no-index '{source}' '{}'",
                            compare.display()
                        )
                    } else {
                        format!("missing {source}; inspect kit file '{}'", compare.display())
                    };
                    let extras = if source_extras.is_empty() {
                        String::new()
                    } else {
                        format!("{}; ", source_extras.join("; "))
                    };
                    self.output.push(format!("kept     {link} (edited; {extras}{inspection}; take the kit's layout with delegate upgrade {link})"));
                    continue;
                }
                if old_folder {
                    self.folders.push((*link).into());
                }
                for (path, body) in PROJECT_FILES
                    .iter()
                    .filter(|(p, _)| p.starts_with(&new_prefix))
                {
                    self.writes.insert((*path).into(), body.as_bytes().to_vec());
                    record
                        .files
                        .insert((*path).into(), now.files[*path].clone());
                }
                record.files.retain(|p, _| !p.starts_with(&old_prefix));
                record
                    .files
                    .insert((*link).into(), now.files[*link].clone());
                self.links.insert((*link).into(), (*target).into());
                self.output.push(format!("moved    {link} -> {canonical}"));
            }
        }
        Ok(())
    }

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
            if OWNED.contains(&path.as_str())
                || self.skill_paths.contains(path)
                || PROJECT_LINKS
                    .iter()
                    .any(|(link, _)| path.starts_with(&format!("{link}/")))
            {
                continue;
            }
            if let Some((_, target)) = PROJECT_LINKS.iter().find(|(link, _)| *link == path) {
                let actual = entry_bytes(&root.join(path))?;
                let hash = actual.as_deref().map(sha256::hex);
                if hash.as_ref() == now.files.get(path)
                    && fs::symlink_metadata(root.join(path))?
                        .file_type()
                        .is_symlink()
                {
                    record.files.insert(path.clone(), now.files[path].clone());
                } else if fs::symlink_metadata(root.join(path)).is_ok() {
                    self.output.push(kept_skill_link(root, path, target)?);
                } else if record.files.contains_key(path) || git_knows(root, path)? {
                    self.output.push(format!(
                        "missing  {path}; restore with delegate upgrade {path}"
                    ));
                } else {
                    self.links.insert(path.clone(), (*target).into());
                    record.files.insert(path.clone(), now.files[path].clone());
                    self.output.push(format!("added    {path}"));
                }
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
    upgrade_with_kits(dir, paths, &history()?)
}

fn upgrade_with_kits(dir: &Path, paths: &[String], kits: &[Kit]) -> Result<()> {
    let root = PathBuf::from(
        git(dir, &["rev-parse", "--show-toplevel"])
            .context("not inside a git repository; enter your project and run delegate init")?,
    );
    let now = current();
    let mut kits = kits.to_vec();
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
    // Refuse relevant symlink ancestry before hashing, comparison copies, or any
    // writes. Do not let a symlink masquerade as a regular-file obstruction.
    if paths.is_empty() {
        for (link, _) in PROJECT_LINKS {
            check_skill_ancestry(&root, link)?;
            let name = link.rsplit('/').next().context("kit link has no name")?;
            check_skill_ancestry(&root, &format!(".agents/skills/{name}/SKILL.md"))?;
        }
    } else {
        for path in paths {
            if let Some((link, _)) = PROJECT_LINKS
                .iter()
                .find(|(link, _)| *link == path || path.starts_with(&format!("{link}/")))
            {
                check_skill_ancestry(&root, link)?;
                let name = link.rsplit('/').next().context("kit link has no name")?;
                check_skill_ancestry(&root, &format!(".agents/skills/{name}/SKILL.md"))?;
            } else if path.starts_with(".agents/skills/") {
                check_skill_ancestry(&root, path)?;
            }
        }
    }
    // Historical children of a skill directory no longer exist as kit entries
    // once that directory is a link. Exclude them from both disk evidence and
    // HEAD-based inference, or a second upgrade reintroduces retired record keys.
    let linked_skill_files: BTreeSet<_> = all_paths
        .iter()
        .filter(|path| {
            PROJECT_LINKS.iter().any(|(link, target)| {
                path.starts_with(&format!("{link}/"))
                    && established_skill_layout(&root, saved.as_ref(), link, target)
            })
        })
        .map(|path| (*path).clone())
        .collect();
    let mut obstructed = BTreeSet::new();
    for (link, _) in PROJECT_LINKS {
        let name = link.rsplit('/').next().context("kit link has no name")?;
        let canonical = format!(".agents/skills/{name}");
        for directory in [canonical.as_str(), *link] {
            if let Some(obstruction) = skill_obstruction(&root, directory)? {
                obstructed.insert(format!("{obstruction}/"));
            }
        }
    }
    let mut hashes = BTreeMap::new();
    for path in all_paths {
        if linked_skill_files.contains(path)
            || obstructed.iter().any(|prefix| path.starts_with(prefix))
        {
            continue;
        }
        if let Some(bytes) = entry_bytes(&root.join(path))? {
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
        if !linked_skill_files.contains(*path)
            && !obstructed.iter().any(|prefix| path.starts_with(prefix))
            && !hashes.contains_key(*path)
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
    record
        .files
        .retain(|path, _| !linked_skill_files.contains(path));
    let mut plan = Plan::default();
    if paths.is_empty() {
        if let Some(kit) = baseline {
            for (path, hash) in &kit.files {
                if linked_skill_files.contains(path)
                    || PROJECT_LINKS.iter().any(|(link, target)| {
                        *link == path
                            && !established_skill_layout(&root, Some(&record), link, target)
                    })
                {
                    continue;
                }
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
        plan.skills(&root, &kits, &now, &mut record, None)?;
        plan.managed(&root, &kits, &now, &mut record)?;
        plan.skill_paths.extend(
            kits.iter()
                .flat_map(|kit| kit.files.keys())
                .filter(|path| obstructed.iter().any(|prefix| path.starts_with(prefix)))
                .cloned(),
        );
        plan.full(&root, &kits, &now, &mut record, &hashes)?;
    } else {
        // Normalize aliases before planning so each skill is taken only once.
        let mut planned = BTreeSet::new();
        for path in paths {
            let normalized = PROJECT_LINKS
                .iter()
                .find(|(link, _)| *link == path || path.starts_with(&format!("{link}/")))
                .map_or(path.as_str(), |(link, _)| *link);
            if !planned.insert(normalized) {
                continue;
            }
            if let Some((file, name)) = path.split_once('#') {
                plan.take_block(&root, &now, &mut record, file, name)?;
            } else if let Some((link, _)) = PROJECT_LINKS
                .iter()
                .find(|(link, _)| *link == path || path.starts_with(&format!("{link}/")))
            {
                // Validated historical skill-file paths take the whole layout,
                // rather than removing a retired file without its replacement.
                plan.skills(&root, &kits, &now, &mut record, Some(link))?;
            } else if let Some(prefix) = obstructed.iter().find(|prefix| {
                path.starts_with(".agents/skills/") && path.starts_with(*prefix)
            })
            {
                let canonical = prefix.trim_end_matches('/');
                plan.output.push(format!("kept     {path} ({canonical} is not a directory; move {canonical} aside manually before retrying delegate upgrade {path})"));
            } else {
                plan.take(std::slice::from_ref(path), &now, &mut record);
            }
        }
    }
    if (paths.is_empty() || paths.iter().any(|path| path == CLAUDE_SETTINGS))
        && (skill_obstruction(&root, ".claude")?.is_none()
            || fs::symlink_metadata(root.join(".claude"))
                .is_ok_and(|meta| meta.file_type().is_symlink()))
    {
        plan.remove_status_line(&root)?;
    }
    let serialized = toml::to_string(&record)?;
    let record_changed = existing.as_deref() != Some(serialized.as_bytes());
    let mut changed = record_changed;
    for path in plan
        .writes
        .keys()
        .chain(&plan.removals)
        .chain(plan.links.keys())
        .chain(&plan.folders)
    {
        check_parents(&root, path)?;
    }
    if record_changed {
        check_parents(&root, RECORD)?;
    }
    // Install replacements before removing legacy sources so failed writes are retryable.
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
    for path in &plan.folders {
        let target = plan.links.get(path).context("missing planned skill link")?;
        replace_skill_folder(&root, path, target)?;
        changed = true;
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
    for (path, target) in &plan.links {
        changed |= install_link(&root, path, target)?;
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

/// Keep the complete source recoverable until its compatibility link is installed.
fn replace_skill_folder(root: &Path, path: &str, target: &str) -> Result<()> {
    let source = root.join(path);
    let backup = loop {
        let candidate =
            source.with_file_name(format!(".delegate-skill-backup-{}", uuid::Uuid::new_v4()));
        match fs::symlink_metadata(&candidate) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break candidate,
            Err(error) => {
                return Err(error).context("check skill backup path; check permissions and retry");
            }
        }
    };
    fs::rename(&source, &backup)
        .with_context(|| format!("replace {path}; check permissions and retry"))?;
    if let Err(error) = install_link(root, path, target) {
        // Never overwrite an entry that appeared while the source was backed up.
        match fs::symlink_metadata(&source) {
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                if let Err(rollback) = fs::rename(&backup, &source) {
                    return Err(error).context(format!(
                        "rollback failed ({rollback}); source preserved at {}; restore it manually to {path}",
                        backup.display()
                    ));
                }
            }
            _ => {
                return Err(error).context(format!(
                    "cannot safely roll back {path}; source preserved at {}; inspect {path} and restore the backup manually",
                    backup.display()
                ));
            }
        }
        return Err(error);
    }
    fs::remove_dir_all(&backup).with_context(|| {
        format!(
            "installed {path}, but could not clean up source backup {}; remaining source bytes are preserved there; check permissions and remove that backup manually",
            backup.display()
        )
    })?;
    Ok(())
}

/// Install an exact kit link, returning whether the filesystem changed.
fn install_link(root: &Path, path: &str, target: &str) -> Result<bool> {
    let dest = root.join(path);
    if fs::read_link(&dest).ok().as_deref().map(Path::as_os_str)
        == Some(std::ffi::OsStr::new(target))
    {
        return Ok(false);
    }
    if fs::symlink_metadata(&dest).is_ok() {
        fs::remove_file(&dest)?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    create_symlink(target, &dest)
        .with_context(|| format!("create {path}; check permissions and retry"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skill names shared by both kit layouts (decision 034).
    const SKILLS: &[&str] = &["delegate", "distill", "evaluate"];

    /// Resolve the skill file from the actual kit instead of hard-coding a
    /// layout: `.claude/skills/<name>/SKILL.md` on main,
    /// `.agents/skills/<name>/SKILL.md` on the shared-skills layout.
    fn skill_path(name: &str) -> String {
        let suffix = format!("/{name}/SKILL.md");
        PROJECT_FILES
            .iter()
            .find(|(p, _)| p.ends_with(suffix.as_str()))
            .unwrap_or_else(|| panic!("kit has no skill file for {name}"))
            .0
            .to_string()
    }

    /// Where the `.claude` compatibility link lives on the new layout (a real
    /// directory holding the skill on the old layout).
    fn skill_link(name: &str) -> String {
        format!(".claude/skills/{name}")
    }

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
            // On the shared-skills layout the kit also installs relative
            // directory links; on the old layout the skill files above are the
            // whole fixture, so this is a no-op there.
            for name in SKILLS {
                if skill_path(name).starts_with(".agents/") {
                    let link = self.0.join(skill_link(name));
                    if fs::symlink_metadata(&link).is_err() {
                        if let Some(parent) = link.parent() {
                            fs::create_dir_all(parent).unwrap();
                        }
                        std::os::unix::fs::symlink(
                            format!("../../.agents/skills/{name}"),
                            &link,
                        )
                        .unwrap();
                    }
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
            let mut snap: BTreeMap<String, Vec<u8>> = PROJECT_FILES
                .iter()
                .map(|(p, _)| *p)
                .chain([RECORD, ".claude/settings.json"])
                .filter_map(|p| {
                    read_optional(&self.0.join(p))
                        .unwrap()
                        .map(|b| (p.into(), b))
                })
                .collect();
            // Capture compatibility links by target (production hashes links
            // as their literal targets). Old-layout directories are skipped.
            for name in SKILLS {
                let link = skill_link(name);
                if fs::symlink_metadata(self.0.join(&link))
                    .is_ok_and(|m| m.file_type().is_symlink())
                {
                    use std::os::unix::ffi::OsStrExt;
                    let target = fs::read_link(self.0.join(&link)).unwrap();
                    snap.insert(link, target.as_os_str().as_bytes().to_vec());
                }
            }
            snap
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
        let edited = skill_path("delegate");
        let missing = skill_path("evaluate");
        record
            .files
            .insert(edited.clone(), sha256::hex(b"old skill"));
        record
            .files
            .insert(missing.clone(), sha256::hex(b"old evaluate"));
        write_file(
            &project.0.join(RECORD),
            toml::to_string(&record).unwrap().as_bytes(),
        )
        .unwrap();
        fs::write(project.0.join(&edited), "our skill").unwrap();
        fs::remove_file(project.0.join(&missing)).unwrap();
        let mut before = project.snapshot();
        upgrade(&project.0, &[edited.clone(), missing.clone()]).unwrap();
        for (path, body) in PROJECT_FILES {
            if edited.as_str() == *path || missing.as_str() == *path {
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
        let missing = skill_path("evaluate");
        let edited = skill_path("delegate");
        fs::remove_file(project.0.join(&missing)).unwrap();
        fs::write(project.0.join(&edited), b"our skill").unwrap();
        upgrade(&project.0, &[]).unwrap();
        assert!(!project.0.join(&missing).exists());
        assert_eq!(fs::read(project.0.join(&edited)).unwrap(), b"our skill");
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert_eq!(record.files[missing.as_str()], current().files[missing.as_str()]);
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
            let new = skill_path("evaluate");
            // Never installed in this project's history: unlike `missing`, this
            // file should be added, even when the other files identify a kit.
            fs::remove_file(project.0.join(&new)).unwrap();
            if !identifiable {
                for (path, _) in PROJECT_FILES {
                    if !OWNED.contains(path) && *path != new.as_str() {
                        fs::write(project.0.join(path), b"our edited kit file").unwrap();
                    }
                }
                // Links are kit files too: left pristine, they still identify
                // the kit. Retarget them so the fixture is genuinely
                // unidentifiable on the shared-skills layout; a no-op where
                // those paths are real directories.
                for name in SKILLS {
                    let dest = project.0.join(skill_link(name));
                    if fs::symlink_metadata(&dest)
                        .is_ok_and(|m| m.file_type().is_symlink())
                    {
                        fs::remove_file(&dest).unwrap();
                        std::os::unix::fs::symlink("../../edited-elsewhere", &dest).unwrap();
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
                fs::read(project.0.join(&new)).unwrap(),
                PROJECT_FILES
                    .iter()
                    .find(|(p, _)| *p == new.as_str())
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
        let skill = skill_path("delegate");
        fs::remove_file(project.0.join(&skill)).unwrap();
        std::os::unix::fs::symlink("../../../AGENTS.md", project.0.join(&skill)).unwrap();
        upgrade(&project.0, std::slice::from_ref(&skill)).unwrap();
        assert_eq!(fs::read(project.0.join("AGENTS.md")).unwrap(), own);
        assert!(
            !fs::symlink_metadata(project.0.join(&skill))
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
    fn skill_links_hash_targets_and_retargeting_is_an_edit() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = PROJECT_LINKS[0].0;
        let target = PROJECT_LINKS[0].1;
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert_eq!(record.files[link], sha256::hex(target.as_bytes()));
        fs::remove_file(project.0.join(link)).unwrap();
        create_symlink("../../elsewhere", project.0.join(link)).unwrap();
        let mut after = record.clone();
        let mut plan = Plan::default();
        plan.full(&project.0, &history().unwrap(), &current(), &mut after, &BTreeMap::new()).unwrap();
        assert!(plan.output.iter().any(|line| line.starts_with(&format!("kept     {link}"))));
        assert!(!plan.links.contains_key(link));
        assert_eq!(after.files[link], record.files[link]);
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(
            fs::read_link(project.0.join(link)).unwrap(),
            Path::new("../../elsewhere")
        );
        let installed: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert_eq!(installed.files[link], record.files[link]);
    }

    #[test]
    fn taking_missing_or_retargeted_links_preserves_canonical_edits() {
        for retargeted in [false, true] {
            let project = Project::new();
            crate::init::init(&project.0).unwrap();
            let link = ".claude/skills/delegate";
            let canonical = ".agents/skills/delegate/SKILL.md";
            fs::write(project.0.join(canonical), "team edit").unwrap();
            fs::write(project.0.join(".agents/skills/delegate/notes.md"), "team notes").unwrap();
            fs::remove_file(project.0.join(link)).unwrap();
            if retargeted {
                create_symlink("../../elsewhere", project.0.join(link)).unwrap();
            }
            let record: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
            upgrade(&project.0, &[link.into()]).unwrap();
            assert_eq!(fs::read_link(project.0.join(link)).unwrap(), Path::new("../../.agents/skills/delegate"));
            assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team edit");
            assert_eq!(fs::read(project.0.join(".agents/skills/delegate/notes.md")).unwrap(), b"team notes");
            let installed: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
            assert_eq!(installed, record);
            let mut plan = Plan::default();
            plan.skills(&project.0, &history().unwrap(), &current(), &mut record.clone(), Some(link)).unwrap();
            assert_eq!(plan.output, Vec::<String>::new());
            assert!(plan.links.is_empty());
            assert!(plan.writes.is_empty());
        }
    }

    fn old_skill(project: &Project, name: &str) {
        let path = format!(".claude/skills/{name}/SKILL.md");
        let body = PROJECT_FILES
            .iter()
            .find(|(p, _)| *p == format!(".agents/skills/{name}/SKILL.md"))
            .unwrap()
            .1;
        write_file(&project.0.join(path), body.as_bytes()).unwrap();
    }

    /// Today's kit at the retired `.claude/skills/<name>/SKILL.md` layout. No
    /// project ever shipped that kit, so tests build it instead of relying on a
    /// fake catalog entry. It goes first so `identify` prefers it to a real
    /// historical kit that happens to share today's skill bodies.
    fn legacy_kit() -> Kit {
        let now = current();
        let mut files = now.files.clone();
        for (link, _) in PROJECT_LINKS {
            let name = link.rsplit('/').next().unwrap();
            let hash = files[&skill_path(name)].clone();
            files.remove(*link);
            files.insert(format!("{link}/SKILL.md"), hash);
        }
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
            templates: now.templates,
        }
    }

    fn history_with_legacy_layout() -> Vec<Kit> {
        let mut kits = history().unwrap();
        kits.insert(0, legacy_kit());
        kits
    }

    #[test]
    fn migrated_skills_do_not_reinfer_retired_entries_from_head() {
        let kits = history_with_legacy_layout();
        for recorded in [false, true] {
            let project = Project::new();
            let mut record = project.install();
            for name in SKILLS {
                let link = skill_link(name);
                fs::remove_file(project.0.join(&link)).unwrap();
                old_skill(&project, name);
                fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
                let canonical = skill_path(name);
                let hash = record.files.remove(&canonical).unwrap();
                record.files.remove(&link);
                record.files.insert(format!("{link}/SKILL.md"), hash);
            }
            if recorded {
                fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
            } else {
                fs::remove_file(project.0.join(RECORD)).unwrap();
            }
            project.commit();
            upgrade_with_kits(&project.0, &[], &kits).unwrap();
            let first = project.snapshot();
            let installed: Record =
                toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
            for name in SKILLS {
                assert!(fs::read_link(project.0.join(skill_link(name))).is_ok());
                assert!(!installed.files.contains_key(&format!("{}/SKILL.md", skill_link(name))));
            }
            // HEAD still contains the old directories until the owner commits.
            upgrade_with_kits(&project.0, &[], &kits).unwrap();
            assert_eq!(first, project.snapshot());
        }
    }

    #[test]
    fn skill_ancestry_refusal_names_the_requested_path() {
        let project = Project::new();
        fs::create_dir_all(project.0.join("shared")).unwrap();
        create_symlink("shared", project.0.join(".agents")).unwrap();
        let requested = ".agents/skills/delegate/SKILL.md";
        let error = check_skill_ancestry(&project.0, requested).unwrap_err().to_string();
        assert!(error.contains(&format!("cannot upgrade {requested} through symlink")), "{error}");
        assert!(!error.contains("/_"), "{error}");
        assert_eq!(fs::read_dir(project.0.join("shared")).unwrap().count(), 0);
    }

    #[test]
    fn raw_link_variants_are_repaired_exactly_and_init_keeps_them() {
        for target in ["../../.agents/skills/delegate/", "../../.agents/skills/./delegate"] {
            let project = Project::new();
            let link = ".claude/skills/delegate";
            fs::create_dir_all(project.0.join(".claude/skills")).unwrap();
            create_symlink(target, project.0.join(link)).unwrap();
            assert!(!established_skill_layout(&project.0, None, link, "../../.agents/skills/delegate"));
            crate::init::init(&project.0).unwrap();
            assert!(!project.0.join(".agents/skills/delegate").exists());
            assert_eq!(fs::read_link(project.0.join(link)).unwrap().as_os_str(), std::ffi::OsStr::new(target));
            upgrade(&project.0, &[link.into()]).unwrap();
            assert_eq!(fs::read_link(project.0.join(link)).unwrap().as_os_str(), std::ffi::OsStr::new("../../.agents/skills/delegate"));
            let mut record: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
            let mut plan = Plan::default();
            plan.full(&project.0, &history().unwrap(), &current(), &mut record, &BTreeMap::new()).unwrap();
            assert!(!plan.output.iter().any(|line| line.starts_with(&format!("kept     {link}"))));
        }
    }

    #[test]
    fn init_skips_canonical_ancestor_obstructions_and_finishes_record() {
        for ancestor in [".agents", ".agents/skills"] {
            for symlink in [false, true] {
                let project = Project::new();
                let path = project.0.join(ancestor);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                if symlink {
                    fs::create_dir_all(project.0.join("shared")).unwrap();
                    fs::write(project.0.join("shared/notes"), "team bytes").unwrap();
                    create_symlink(project.0.join("shared"), &path).unwrap();
                } else {
                    fs::write(&path, "team bytes").unwrap();
                }
                crate::init::init(&project.0).unwrap();
                assert!(project.0.join("AGENTS.md").is_file());
                let record: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
                assert!(record.files.contains_key("evals/run"));
                assert!(!record.files.contains_key(".agents/skills/delegate/SKILL.md"));
                assert!(fs::symlink_metadata(project.0.join(".claude/skills/delegate")).is_err());
                if symlink {
                    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
                    assert_eq!(fs::read(project.0.join("shared/notes")).unwrap(), b"team bytes");
                    assert_eq!(fs::read_dir(project.0.join("shared")).unwrap().count(), 1);
                } else {
                    assert_eq!(fs::read(&path).unwrap(), b"team bytes");
                }
            }
        }
    }

    #[test]
    fn init_does_not_add_a_competing_copy_to_an_old_layout() {
        let project = Project::new();
        old_skill(&project, "delegate");
        crate::init::init(&project.0).unwrap();
        assert!(!project.0.join(".agents/skills/delegate").exists());
        assert!(
            fs::symlink_metadata(project.0.join(".claude/skills/delegate"))
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn init_does_not_add_a_link_to_a_mixed_canonical_layout() {
        let project = Project::new();
        let canonical = project.0.join(".agents/skills/delegate");
        fs::create_dir_all(&canonical).unwrap();
        fs::write(canonical.join("notes.md"), "team notes").unwrap();
        crate::init::init(&project.0).unwrap();
        assert!(fs::symlink_metadata(project.0.join(".claude/skills/delegate")).is_err());
        assert!(!canonical.join("SKILL.md").exists());
        assert_eq!(fs::read(canonical.join("notes.md")).unwrap(), b"team notes");
    }

    #[test]
    fn deleted_legacy_skill_has_working_restore_advice() {
        let project = Project::new();
        let link = ".claude/skills/delegate";
        let mut record = Record::new();
        record.files.insert(format!("{link}/SKILL.md"), "old hash".into());
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        let mut plan = Plan::default();
        plan.skills(&project.0, &history().unwrap(), &current(), &mut record, None).unwrap();
        assert!(plan.output.contains(&format!("missing  {link}; restore with delegate upgrade {link}")));
        assert!(!plan.output.iter().any(|line| line.contains("git diff")));
        upgrade(&project.0, &[link.into()]).unwrap();
        assert!(project.0.join(format!("{link}/SKILL.md")).is_file());
    }

    #[test]
    fn recorded_link_replaced_by_folder_is_kept_with_layout_advice() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        fs::remove_file(project.0.join(link)).unwrap();
        old_skill(&project, "delegate");
        fs::write(project.0.join(".agents/skills/delegate/SKILL.md"), "team edit").unwrap();
        let mut record: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        let mut plan = Plan::default();
        plan.skills(&project.0, &history().unwrap(), &current(), &mut record, None).unwrap();
        assert!(plan.output.iter().any(|line| line.starts_with(&format!("kept     {link}")) && line.contains(&format!("delegate upgrade {link}"))));
        assert!(!plan.output.iter().any(|line| line.contains("git diff")));
        upgrade(&project.0, &[link.into()]).unwrap();
        assert!(fs::symlink_metadata(project.0.join(link)).unwrap().is_dir());
        assert_eq!(fs::read(project.0.join(".agents/skills/delegate/SKILL.md")).unwrap(), b"team edit");
    }

    #[test]
    fn edited_old_skill_is_kept_whole() {
        let project = Project::new();
        old_skill(&project, "delegate");
        fs::write(
            project.0.join(".claude/skills/delegate/team.md"),
            "team addition",
        )
        .unwrap();
        let mut plan = Plan::default();
        plan.skills(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut Record::new(),
            None,
        )
        .unwrap();
        assert!(plan.writes.is_empty());
        assert!(plan.links.is_empty());
        assert!(
            plan.output
                .iter()
                .any(|line| line.starts_with("kept     .claude/skills/delegate"))
        );
    }

    #[test]
    fn destination_edits_block_both_automatic_and_targeted_moves() {
        for committed in [false, true] {
            let project = Project::new();
            old_skill(&project, "delegate");
            write_file(
                &project.0.join(".agents/skills/delegate/SKILL.md"),
                b"our edit",
            )
            .unwrap();
            if committed {
                project.commit();
            }
            for take in [None, Some(".claude/skills/delegate")] {
                let mut plan = Plan::default();
                plan.skills(
                    &project.0,
                    &history().unwrap(),
                    &current(),
                    &mut Record::new(),
                    take,
                )
                .unwrap();
                assert!(plan.writes.is_empty());
                assert!(plan.links.is_empty());
                assert_eq!(plan.folders, Vec::<String>::new());
                assert!(
                    plan.output
                        .iter()
                        .any(|line| line.contains(".agents/skills/delegate"))
                );
            }
        }
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
    fn history_lists_only_shipped_kits() {
        let kits = history().unwrap();
        let now = current();
        let legacy = legacy_kit();
        assert_eq!(
            kits.iter().filter(|kit| kit.id == now.id).count(),
            1,
            "history must contain the current kit exactly once"
        );
        let (newest, older) = kits.split_last().unwrap();
        assert_eq!(newest.id, now.id, "the current kit must be last");
        assert_eq!(newest.files, now.files);
        for kit in older {
            assert_ne!(kit.id, now.id, "{} repeats the current kit id", kit.id);
            assert_ne!(
                kit.files, legacy.files,
                "{} is today's kit under the retired skill layout; build it in tests instead",
                kit.id
            );
        }
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

    // Review t120: following the printed advice must be able to restore a
    // deleted link when the team keeps its own file in `.agents/skills/<name>/`.
    #[test]
    fn review_restoring_a_deleted_link_with_team_files_in_agents() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        fs::write(project.0.join(".agents/skills/delegate/notes.md"), "team").unwrap();
        fs::remove_file(project.0.join(link)).unwrap();
        project.commit();
        // `missing  .claude/skills/delegate; restore with delegate upgrade .claude/skills/delegate`
        upgrade(&project.0, &[]).unwrap();
        // Collision advice: take the kit's files first, then retry.
        upgrade(&project.0, &[".agents/skills/delegate/SKILL.md".into()]).unwrap();
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(
            fs::read_link(project.0.join(link)).ok().as_deref(),
            Some(Path::new("../../.agents/skills/delegate")),
            "the advised commands never restore the link"
        );
        assert!(project.0.join(".agents/skills/delegate/notes.md").exists());
    }

    // Review t120: a per-file take of a retired old-layout skill path deletes
    // the team's edited skill and installs the kit's skill nowhere.
    #[test]
    fn review_taking_an_old_skill_file_leaves_a_skill_in_place() {
        let project = Project::new();
        old_skill(&project, "delegate");
        let old = ".claude/skills/delegate/SKILL.md";
        fs::write(project.0.join(old), "team edit").unwrap();
        project.commit();
        upgrade(&project.0, &[old.into()]).unwrap();
        assert!(
            project.0.join(old).exists()
                || project.0.join(".agents/skills/delegate/SKILL.md").exists(),
            "the delegate skill is gone from both layouts"
        );
    }

    #[test]
    fn layout_take_normalizes_aliases_in_every_order() {
        let link = ".claude/skills/delegate";
        let child = format!("{link}/SKILL.md");
        for paths in [
            vec![link.into(), child.clone()],
            vec![child, link.into()],
            vec![link.into(), link.into()],
        ] {
            let project = Project::new();
            old_skill(&project, "delegate");
            upgrade(&project.0, &paths).unwrap();
            assert!(project.0.join(".agents/skills/delegate/SKILL.md").is_file());
            assert!(fs::read_link(project.0.join(link)).is_ok());
        }
    }

    #[test]
    fn polluted_link_record_is_cleaned_without_taking_canonical_edits() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        let canonical = ".agents/skills/delegate/SKILL.md";
        let mut record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        record
            .files
            .insert(format!("{link}/SKILL.md"), "stale".into());
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        fs::remove_file(project.0.join(link)).unwrap();
        fs::write(project.0.join(canonical), "team edit").unwrap();
        fs::write(project.0.join(".agents/skills/delegate/notes.md"), "notes").unwrap();
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team edit");
        assert_eq!(
            fs::read(project.0.join(".agents/skills/delegate/notes.md")).unwrap(),
            b"notes"
        );
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert!(!record.files.contains_key(&format!("{link}/SKILL.md")));
    }

    #[test]
    fn link_take_installs_absent_tree_but_preserves_deliberate_missing_files() {
        for recorded in [false, true] {
            let project = Project::new();
            if recorded {
                crate::init::init(&project.0).unwrap();
                fs::remove_file(project.0.join(".claude/skills/delegate")).unwrap();
                fs::remove_dir_all(project.0.join(".agents/skills/delegate")).unwrap();
            } else {
                old_skill(&project, "distill");
            }
            let link = ".claude/skills/delegate";
            upgrade(&project.0, &[link.into()]).unwrap();
            let canonical = ".agents/skills/delegate/SKILL.md";
            assert!(project.0.join(canonical).is_file());
            fs::remove_file(project.0.join(canonical)).unwrap();
            fs::remove_file(project.0.join(link)).unwrap();
            let mut plan = Plan::default();
            plan.skills(
                &project.0,
                &history().unwrap(),
                &current(),
                &mut Record::new(),
                Some(link),
            )
            .unwrap();
            assert!(plan.writes.is_empty());
            assert!(plan.output.iter().any(|s| s
                == &format!("missing  {canonical}; restore with delegate upgrade {canonical}")));
            upgrade(&project.0, &[link.into()]).unwrap();
            assert!(!project.0.join(canonical).exists());
        }
    }

    #[test]
    fn layout_take_never_deletes_extra_source_files() {
        let project = Project::new();
        old_skill(&project, "delegate");
        let extra = ".claude/skills/delegate/notes.md";
        fs::write(project.0.join(extra), "team").unwrap();
        let mut plan = Plan::default();
        plan.skills(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut Record::new(),
            Some(".claude/skills/delegate"),
        )
        .unwrap();
        assert!(
            plan.output
                .iter()
                .any(|s| s.contains(extra) && s.contains("manually"))
        );
        assert_eq!(plan.folders, Vec::<String>::new());
        upgrade(&project.0, &[".claude/skills/delegate".into()]).unwrap();
        assert_eq!(fs::read(project.0.join(extra)).unwrap(), b"team");
    }

    // Review t120 r5: naming the same skill twice (the link and its old
    // SKILL.md, both accepted paths) plans the folder removal twice; the
    // second remove_dir_all fails after the first already deleted the folder,
    // so the skill ends up in neither layout and upgrade errors out.
    #[test]
    fn review_r5_naming_one_skill_twice_keeps_a_skill() {
        let project = Project::new();
        old_skill(&project, "delegate");
        let old = ".claude/skills/delegate/SKILL.md";
        fs::write(project.0.join(old), "team edit").unwrap();
        project.commit();
        let result = upgrade(&project.0, &[".claude/skills/delegate".into(), old.into()]);
        assert!(result.is_ok(), "upgrade failed: {result:?}");
        assert!(
            project.0.join(".agents/skills/delegate/SKILL.md").is_file(),
            "the delegate skill is gone from both layouts"
        );
    }

    // Review t120 r5: a recorded link replaced by a copy of the kit's folder
    // makes a full upgrade seed the retired `.claude/skills/<name>/SKILL.md`
    // key into the record. Once the folder is removed, the link counts as a
    // "deleted legacy" skill, and restoring it is refused as a collision
    // unless the team throws away its canonical edits.
    #[test]
    fn review_r5_restoring_a_link_after_a_folder_copy_keeps_canonical_edits() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/evaluate";
        let canonical = ".agents/skills/evaluate/SKILL.md";
        fs::remove_file(project.0.join(link)).unwrap();
        old_skill(&project, "evaluate");
        fs::write(project.0.join(canonical), "team edit").unwrap();
        upgrade(&project.0, &[]).unwrap();
        let record: Record = toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert!(
            !record.files.contains_key(&format!("{link}/SKILL.md")),
            "a kept folder put a retired legacy key into the record"
        );
        fs::remove_dir_all(project.0.join(link)).unwrap();
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(
            fs::read_link(project.0.join(link)).ok().as_deref(),
            Some(Path::new("../../.agents/skills/evaluate")),
        );
        assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team edit");
    }

    // Review t120 r5: a collision caused by a team file in
    // `.agents/skills/<name>/` lists only kit files to take; following that
    // advice never completes the move, and the extra file is never named.
    #[test]
    fn review_r5_collision_advice_names_the_extra_team_file() {
        let project = Project::new();
        old_skill(&project, "delegate");
        let body = PROJECT_FILES
            .iter()
            .find(|(p, _)| *p == ".agents/skills/delegate/SKILL.md")
            .unwrap()
            .1;
        write_file(
            &project.0.join(".agents/skills/delegate/SKILL.md"),
            body.as_bytes(),
        )
        .unwrap();
        fs::write(project.0.join(".agents/skills/delegate/notes.md"), "team").unwrap();
        let mut plan = Plan::default();
        plan.skills(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut Record::new(),
            None,
        )
        .unwrap();
        assert!(
            plan.output.iter().any(|line| line.contains("notes.md")),
            "collision advice doesn't name what blocks the move: {:?}",
            plan.output
        );
    }

    #[test]
    fn canonical_obstruction_is_kept_for_full_and_targeted_upgrades() {
        let link = ".claude/skills/delegate";
        let canonical = ".agents/skills/delegate";
        for legacy in [false, true] {
            for targets in [vec![], vec![link.into()], vec![format!("{canonical}/SKILL.md")]] {
                let project = Project::new();
                crate::init::init(&project.0).unwrap();
                fs::remove_file(project.0.join(link)).unwrap();
                if legacy {
                    old_skill(&project, "delegate");
                }
                fs::remove_dir_all(project.0.join(canonical)).unwrap();
                fs::write(project.0.join(canonical), "team obstruction").unwrap();
                upgrade(&project.0, &targets).unwrap();
                assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team obstruction");
                if legacy {
                    assert!(project.0.join(format!("{link}/SKILL.md")).is_file());
                } else {
                    assert!(fs::symlink_metadata(project.0.join(link)).is_err());
                }
                let mut plan = Plan::default();
                plan.skills(&project.0, &history().unwrap(), &current(), &mut Record::new(), None).unwrap();
                assert!(plan.output.iter().any(|line| line.contains(canonical) && line.contains("move") && line.contains("aside manually")));
                assert!(plan.writes.is_empty());
                assert!(plan.links.is_empty());
                assert_eq!(plan.folders, Vec::<String>::new());
            }
        }
    }

    #[test]
    fn source_team_files_survive_targeted_layout_take() {
        let project = Project::new();
        old_skill(&project, "delegate");
        let link = ".claude/skills/delegate";
        let team = format!("{link}/team.md");
        fs::write(project.0.join(&team), "team data").unwrap();
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(fs::read(project.0.join(team)).unwrap(), b"team data");
        assert!(project.0.join(format!("{link}/SKILL.md")).is_file());
        assert!(!project.0.join(".agents/skills/delegate").exists());
    }

    #[test]
    fn regular_file_link_take_preserves_canonical_edits() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        let canonical = ".agents/skills/delegate/SKILL.md";
        fs::remove_file(project.0.join(link)).unwrap();
        fs::write(project.0.join(link), "../../.agents/skills/delegate").unwrap();
        fs::write(project.0.join(canonical), "team edit").unwrap();
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        let mut after = record.clone();
        let mut plan = Plan::default();
        plan.full(&project.0, &history().unwrap(), &current(), &mut after, &BTreeMap::new()).unwrap();
        assert!(plan.output.iter().any(|line| line.starts_with(&format!("kept     {link} ")) && line.contains(&format!("delegate upgrade {link}"))));
        assert_eq!(after.files[link], record.files[link]);
        upgrade(&project.0, &[]).unwrap();
        assert!(fs::symlink_metadata(project.0.join(link)).unwrap().is_file());
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(fs::read_link(project.0.join(link)).unwrap(), Path::new("../../.agents/skills/delegate"));
        assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team edit");
    }

    // Review t120 r6: an old folder whose only change is an added team file
    // has no `.agents/skills/<name>/` at all, yet it is reported as a
    // collision with that (nonexistent) folder instead of the contract's
    // `kept     .claude/skills/<name> (edited; ...)` line.
    #[test]
    fn review_r6_extra_file_in_old_folder_is_reported_as_edited() {
        let project = Project::new();
        old_skill(&project, "delegate");
        fs::write(project.0.join(".claude/skills/delegate/team.md"), "team").unwrap();
        let mut plan = Plan::default();
        plan.skills(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut Record::new(),
            None,
        )
        .unwrap();
        assert!(!project.0.join(".agents/skills/delegate").exists());
        assert!(
            plan.output
                .iter()
                .any(|l| l.starts_with("kept     .claude/skills/delegate (edited;")),
            "{:?}",
            plan.output
        );
    }

    // Review t120 r6: deleting the old SKILL.md by hand leaves an empty
    // folder; the kept line's compare command names the deleted file, so
    // running it fails with "Could not access".
    #[test]
    fn review_r6_kept_old_folder_compare_command_runs() {
        let project = Project::new();
        old_skill(&project, "delegate");
        fs::remove_file(project.0.join(".claude/skills/delegate/SKILL.md")).unwrap();
        let mut plan = Plan::default();
        plan.skills(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut Record::new(),
            None,
        )
        .unwrap();
        let line = plan
            .output
            .iter()
            .find(|l| l.starts_with("kept     .claude/skills/delegate"))
            .expect("kept line");
        if let Some(rest) = line.split("git diff --no-index '").nth(1) {
            let path = rest.split('\'').next().unwrap();
            assert!(
                project.0.join(path).exists(),
                "advised compare names a missing file: {line}"
            );
        }
    }

    // Review t120 r6: a plain text file holding the link's target (what git
    // checks out with core.symlinks=false) hashes like the link and is
    // treated as the pristine link: nothing is reported, Claude Code finds
    // no skill.
    #[test]
    fn review_r6_text_file_in_place_of_link_is_reported() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        fs::remove_file(project.0.join(link)).unwrap();
        fs::write(project.0.join(link), "../../.agents/skills/delegate").unwrap();
        let mut record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        let mut plan = Plan::default();
        plan.full(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut record,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            plan.output
                .iter()
                .any(|l| l.contains(&format!("{link} ")) || l.contains(&format!("{link};"))),
            "a file replacing the link is silently pristine: {:?}",
            plan.output
        );
    }

    // Review t120 r6: a canonical skill path that is a file, not a folder,
    // aborts the whole upgrade with a permissions hint instead of being kept.
    #[test]
    fn review_r6_canonical_skill_file_does_not_abort_upgrade() {
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        fs::remove_dir_all(project.0.join(".agents/skills/delegate")).unwrap();
        fs::write(project.0.join(".agents/skills/delegate"), "x").unwrap();
        let result = upgrade(&project.0, &[]);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn old_skill_pointers_and_files_preserve_legacy_evidence() {
        let kits = history_with_legacy_layout();
        let link = ".claude/skills/delegate";
        let old = format!("{link}/SKILL.md");
        for recorded in [false, true] {
            for replacement in ["file", "shared", "dangling"] {
                let project = Project::new();
                for name in SKILLS {
                    old_skill(&project, name);
                }
                let mut record = Record::new();
                let hash = sha256::hex(&fs::read(project.0.join(&old)).unwrap());
                record.files.insert(old.clone(), hash.clone());
                if recorded {
                    write_file(
                        &project.0.join(RECORD),
                        toml::to_string(&record).unwrap().as_bytes(),
                    )
                    .unwrap();
                }
                if replacement == "shared" {
                    fs::create_dir_all(project.0.join("shared")).unwrap();
                    fs::rename(project.0.join(link), project.0.join("shared/delegate")).unwrap();
                } else {
                    fs::remove_dir_all(project.0.join(link)).unwrap();
                }
                if replacement == "file" {
                    fs::write(project.0.join(link), "team file").unwrap();
                } else {
                    create_symlink("../../shared/delegate", project.0.join(link)).unwrap();
                }
                project.commit();
                let mut plan = Plan::default();
                plan.skills(
                    &project.0,
                    &kits,
                    &current(),
                    &mut record.clone(),
                    None,
                )
                .unwrap();
                let kept = plan
                    .output
                    .iter()
                    .find(|line| line.starts_with(&format!("kept     {link} (edited;")))
                    .unwrap();
                assert!(kept.contains("delegate upgrade .claude/skills/delegate"));
                assert!(!kept.contains("git diff"));
                upgrade_with_kits(&project.0, &[], &kits).unwrap();
                assert!(fs::symlink_metadata(project.0.join(".agents/skills/delegate")).is_err());
                let after: Record =
                    toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
                if recorded {
                    assert_eq!(after.files[&old], hash);
                    assert!(!after.files.contains_key(link));
                }
                assert!(fs::read_link(project.0.join(".claude/skills/distill")).is_ok());
                let before = project.snapshot();
                upgrade_with_kits(&project.0, &[], &kits).unwrap();
                assert_eq!(project.snapshot(), before);
                upgrade_with_kits(&project.0, &[link.into()], &kits).unwrap();
                assert_eq!(
                    fs::read_link(project.0.join(link)).unwrap(),
                    Path::new("../../.agents/skills/delegate")
                );
                if replacement == "shared" {
                    assert_eq!(
                        sha256::hex(&fs::read(project.0.join("shared/delegate/SKILL.md")).unwrap()),
                        hash
                    );
                }
            }
        }
    }

    #[test]
    fn recorded_retargeted_link_reports_target_and_preserves_canonical() {
        let project = Project::new();
        let record = project.install();
        let link = ".claude/skills/delegate";
        let canonical = ".agents/skills/delegate/SKILL.md";
        fs::remove_file(project.0.join(link)).unwrap();
        create_symlink("../../shared/delegate", project.0.join(link)).unwrap();
        fs::write(project.0.join(canonical), "team edit").unwrap();
        let mut plan = Plan::default();
        plan.full(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut record.clone(),
            &BTreeMap::new(),
        )
        .unwrap();
        let message = plan
            .output
            .iter()
            .find(|line| line.starts_with(&format!("kept     {link} ")))
            .unwrap();
        assert!(message.contains("actual symlink target '../../shared/delegate'"));
        assert!(message.contains("expected symlink target '../../.agents/skills/delegate'"));
        assert!(!message.contains("git diff"));
        upgrade(&project.0, &[]).unwrap();
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(fs::read(project.0.join(canonical)).unwrap(), b"team edit");
    }

    #[test]
    fn skill_ancestor_obstructions_are_named_and_never_written_through() {
        for obstruction in [".agents", ".agents/skills", ".claude", ".claude/skills"] {
            for target in [
                None,
                Some(".claude/skills/delegate"),
                Some(".agents/skills/delegate/SKILL.md"),
            ] {
                let project = Project::new();
                project.install();
                fs::remove_dir_all(project.0.join(obstruction)).unwrap();
                fs::write(project.0.join(obstruction), "team obstruction").unwrap();
                fs::set_permissions(
                    project.0.join("evals/run"),
                    fs::Permissions::from_mode(0o644),
                )
                .unwrap();
                let mut plan = Plan::default();
                plan.skills(
                    &project.0,
                    &history().unwrap(),
                    &current(),
                    &mut Record::new(),
                    None,
                )
                .unwrap();
                assert!(plan.output.iter().any(|line| line.contains(&format!(
                    "{obstruction} is not a directory; move {obstruction} aside manually"
                ))));
                let paths = target.into_iter().map(str::to_owned).collect::<Vec<_>>();
                upgrade(&project.0, &paths).unwrap();
                assert_eq!(
                    fs::read(project.0.join(obstruction)).unwrap(),
                    b"team obstruction"
                );
                if target.is_none() {
                    assert_ne!(
                        fs::metadata(project.0.join("evals/run"))
                            .unwrap()
                            .permissions()
                            .mode() & 0o111,
                        0
                    );
                }
            }
        }
    }

    // Review t120 r7: an old-layout skill folder replaced by a regular file
    // aborts the whole upgrade (ENOTDIR + permissions hint) instead of being
    // kept while the other skills move.
    #[test]
    fn review_r7_old_skill_folder_replaced_by_file_does_not_abort_upgrade() {
        let project = Project::new();
        for name in SKILLS {
            old_skill(&project, name);
        }
        fs::remove_dir_all(project.0.join(".claude/skills/delegate")).unwrap();
        fs::write(project.0.join(".claude/skills/delegate"), "team file").unwrap();
        project.commit();
        let result = upgrade(&project.0, &[]);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            fs::read(project.0.join(".claude/skills/delegate")).unwrap(),
            b"team file"
        );
        assert!(fs::read_link(project.0.join(".claude/skills/distill")).is_ok());
    }

    // Review t120 r7: an old-layout skill reached through the team's own
    // symlink (e.g. a shared skills folder) is kept, yet the upgrade still
    // installs a competing .agents/skills/<name>/ copy next to it.
    #[test]
    fn review_r7_symlinked_old_skill_folder_gets_no_competing_copy() {
        let project = Project::new();
        for name in SKILLS {
            old_skill(&project, name);
        }
        fs::create_dir_all(project.0.join("shared")).unwrap();
        fs::rename(
            project.0.join(".claude/skills/delegate"),
            project.0.join("shared/delegate"),
        )
        .unwrap();
        create_symlink(
            "../../shared/delegate",
            project.0.join(".claude/skills/delegate"),
        )
        .unwrap();
        project.commit();
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(
            fs::read_link(project.0.join(".claude/skills/delegate")).unwrap(),
            Path::new("../../shared/delegate")
        );
        assert!(
            fs::symlink_metadata(project.0.join(".agents/skills/delegate")).is_err(),
            "a competing .agents/skills/delegate was added while .claude/skills/delegate was kept"
        );
    }

    /// Review r8: a recorded old skill whose bytes differ from the record is an
    /// edit (as for every other kit file), even when they match some kit.
    #[test]
    fn review_r8_old_skill_differing_from_its_record_is_kept() {
        let project = Project::new();
        let mut record = project.install();
        for name in SKILLS {
            let link = skill_link(name);
            fs::remove_file(project.0.join(&link)).unwrap();
            old_skill(&project, name);
            fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
            let hash = record.files.remove(&skill_path(name)).unwrap();
            record.files.remove(&link);
            record.files.insert(format!("{link}/SKILL.md"), hash);
        }
        // The record names an older kit's text; the folder holds different text
        // that happens to be some kit's (a team pinning a version by hand).
        let older = history()
            .unwrap()
            .iter()
            .filter_map(|kit| kit.files.get(".claude/skills/delegate/SKILL.md").cloned())
            .find(|hash| *hash != record.files[".claude/skills/delegate/SKILL.md"])
            .expect("an earlier kit with different delegate skill text");
        record
            .files
            .insert(".claude/skills/delegate/SKILL.md".into(), older);
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        project.commit();
        let before = fs::read(project.0.join(".claude/skills/delegate/SKILL.md")).unwrap();
        upgrade(&project.0, &[]).unwrap();
        assert!(
            fs::symlink_metadata(project.0.join(".claude/skills/delegate"))
                .unwrap()
                .is_dir(),
            "an old skill that differs from its record was moved as pristine"
        );
        assert_eq!(
            fs::read(project.0.join(".claude/skills/delegate/SKILL.md")).unwrap(),
            before
        );
        assert!(fs::symlink_metadata(project.0.join(".agents/skills/delegate")).is_err());
    }

    #[test]
    fn uncommitted_historical_skill_pin_does_not_override_record() {
        let project = Project::new();
        let mut record = project.install();
        let link = ".claude/skills/delegate";
        let source = format!("{link}/SKILL.md");
        fs::remove_file(project.0.join(link)).unwrap();
        old_skill(&project, "delegate");
        let body = fs::read(project.0.join(&source)).unwrap();
        let hash = sha256::hex(&body);
        let older = history()
            .unwrap()
            .iter()
            .filter_map(|kit| kit.files.get(&source).cloned())
            .find(|candidate| *candidate != hash)
            .unwrap();
        record.files.remove(link);
        record.files.remove(".agents/skills/delegate/SKILL.md");
        record.files.insert(source.clone(), older);
        fs::remove_dir_all(project.0.join(".agents/skills/delegate")).unwrap();
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        upgrade(&project.0, &[]).unwrap();
        assert_eq!(fs::read(project.0.join(source)).unwrap(), body);
        assert!(project.0.join(link).is_dir());
        assert!(!project.0.join(".agents/skills/delegate").exists());
    }

    #[test]
    fn skill_symlink_ancestors_refuse_before_changes_but_allow_unrelated_take() {
        for ancestor in [".claude", ".claude/skills", ".agents", ".agents/skills"] {
            for targeted in [false, true] {
                let project = Project::new();
                project.install();
                let elsewhere = project.0.join("shared");
                fs::rename(project.0.join(ancestor), &elsewhere).unwrap();
                create_symlink(&elsewhere, project.0.join(ancestor)).unwrap();
                fs::write(elsewhere.join("team.txt"), "team content").unwrap();
                let before = project.snapshot();
                let paths = if targeted {
                    vec![".claude/skills/delegate".into()]
                } else {
                    vec![]
                };
                let error = upgrade(&project.0, &paths).unwrap_err().to_string();
                assert!(error.contains("through symlink"), "{error}");
                assert!(error.contains(ancestor), "{error}");
                assert_eq!(project.snapshot(), before);
                assert_eq!(
                    fs::read(elsewhere.join("team.txt")).unwrap(),
                    b"team content"
                );
                upgrade(&project.0, &["evals/run".into()]).unwrap();
                assert_eq!(project.snapshot(), before);
            }
        }
    }

    #[test]
    fn init_keeps_canonical_regular_file_with_correct_link() {
        let project = Project::new();
        let canonical = project.0.join(".agents/skills/delegate");
        fs::create_dir_all(canonical.parent().unwrap()).unwrap();
        fs::write(&canonical, "team content").unwrap();
        fs::create_dir_all(project.0.join(".claude/skills")).unwrap();
        create_symlink(
            "../../.agents/skills/delegate",
            project.0.join(".claude/skills/delegate"),
        )
        .unwrap();
        crate::init::init(&project.0).unwrap();
        assert_eq!(fs::read(canonical).unwrap(), b"team content");
        let record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        assert!(
            !record
                .files
                .contains_key(".agents/skills/delegate/SKILL.md")
        );
        assert!(!record.files.contains_key(".claude/skills/delegate"));
    }

    /// Review r8: a `.claude` that is a symlink to a real directory is not "not a
    /// directory"; a full upgrade must not silently skip the kit status line.
    #[test]
    fn review_r8_symlinked_claude_dir_does_not_silently_skip_status_line() {
        let project = Project::new();
        project.install();
        let elsewhere = project.0.join("shared-claude");
        fs::rename(project.0.join(".claude"), &elsewhere).unwrap();
        create_symlink("shared-claude", project.0.join(".claude")).unwrap();
        let line = b"{\"statusLine\": {\"command\": \"delegate statusline\"}}\n";
        fs::write(elsewhere.join("settings.json"), line).unwrap();
        project.commit();
        let result = upgrade(&project.0, &[]);
        assert!(
            result.is_err() || fs::read(elsewhere.join("settings.json")).unwrap() != line,
            "upgrade succeeded (\"up to date\") but left the kit's status line in place"
        );
    }

    /// Review r9: the skill-layout symlink refusal must not spread to unrelated
    /// kit folders. Before t120 a pristine project whose `evals/` (or `docs/`)
    /// is a symlink upgraded as "up to date"; nothing needs writing through it.
    #[test]
    fn review_r9_symlinked_unrelated_kit_folder_with_nothing_to_write_still_upgrades() {
        for folder in ["evals", "docs"] {
            let project = Project::new();
            project.install();
            let elsewhere = project.0.join(format!("shared-{folder}"));
            fs::rename(project.0.join(folder), &elsewhere).unwrap();
            create_symlink(format!("shared-{folder}"), project.0.join(folder)).unwrap();
            project.commit();
            let before = project.snapshot();
            let result = upgrade(&project.0, &[]);
            assert!(
                result.is_ok(),
                "full upgrade with a symlinked {folder}/ and nothing to write refused: {}",
                result.unwrap_err()
            );
            assert_eq!(project.snapshot(), before);
        }
    }

    /// Review r9: a deleted old-layout skill gets no `.agents` copy ("add no
    /// .agents/skills/<name>/"), even when the record lacks that skill's entry;
    /// git history alone proved the deletion before t120 ("missing").
    #[test]
    fn review_r9_deleted_unrecorded_old_skill_gets_no_canonical_copy() {
        let project = Project::new();
        let mut record = project.install();
        for name in SKILLS {
            let link = skill_link(name);
            fs::remove_file(project.0.join(&link)).unwrap();
            old_skill(&project, name);
            fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
            let hash = record.files.remove(&skill_path(name)).unwrap();
            record.files.remove(&link);
            if *name != "distill" {
                record.files.insert(format!("{link}/SKILL.md"), hash);
            }
        }
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        project.commit();
        fs::remove_dir_all(project.0.join(".claude/skills/distill")).unwrap();
        project.commit();
        upgrade(&project.0, &[]).unwrap();
        assert!(
            fs::symlink_metadata(project.0.join(".agents/skills/distill")).is_err(),
            "a deliberately deleted skill was re-added under .agents/skills"
        );
    }

    /// Review t120 final: a link whose target differs only by a trailing slash
    /// (shell tab completion) is reported `kept` by a plain upgrade, but the
    /// advised `delegate upgrade .claude/skills/<name>` compares with `Path`
    /// equality, sees nothing to do and leaves the link as it was, so the
    /// advice never clears the report.
    #[test]
    fn review_final_trailing_slash_link_take_restores_the_kit_target() {
        use std::os::unix::ffi::OsStrExt;
        let project = Project::new();
        crate::init::init(&project.0).unwrap();
        let link = ".claude/skills/delegate";
        fs::remove_file(project.0.join(link)).unwrap();
        create_symlink("../../.agents/skills/delegate/", project.0.join(link)).unwrap();
        project.commit();
        let mut record: Record =
            toml::from_str(&fs::read_to_string(project.0.join(RECORD)).unwrap()).unwrap();
        let mut plan = Plan::default();
        plan.full(
            &project.0,
            &history().unwrap(),
            &current(),
            &mut record,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            plan.output.iter().any(|l| l.starts_with(&format!("kept     {link}"))),
            "plain upgrade treats the trailing-slash link as edited: {:?}",
            plan.output
        );
        upgrade(&project.0, &[link.into()]).unwrap();
        assert_eq!(
            fs::read_link(project.0.join(link)).unwrap().as_os_str().as_bytes(),
            b"../../.agents/skills/delegate",
            "the advised layout take left the reported link unchanged"
        );
    }

    /// Review t120 round 10: a migration removes the pristine old folders
    /// before the canonical copies are written. When a write then fails (here
    /// an unwritable `.agents/skills`), the upgrade errors out with the old
    /// skills already deleted and nothing installed; the next plain upgrade
    /// reports them as user deletions (`missing`) instead of migrating.
    #[test]
    fn review_r10_failed_migration_write_keeps_the_old_skill_folders() {
        let project = Project::new();
        let mut record = project.install();
        for name in SKILLS {
            let link = skill_link(name);
            fs::remove_file(project.0.join(&link)).unwrap();
            old_skill(&project, name);
            fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
            let hash = record.files.remove(&skill_path(name)).unwrap();
            record.files.remove(&link);
            record.files.insert(format!("{link}/SKILL.md"), hash);
        }
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        fs::create_dir_all(project.0.join(".agents/skills/team")).unwrap();
        fs::write(project.0.join(".agents/skills/team/SKILL.md"), "team").unwrap();
        project.commit();
        let saved_record = fs::read(project.0.join(RECORD)).unwrap();
        let sources: Vec<_> = SKILLS.iter().map(|name| {
            fs::read(project.0.join(format!(".claude/skills/{name}/SKILL.md"))).unwrap()
        }).collect();
        let skills = project.0.join(".agents/skills");
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o555)).unwrap();
        let result = upgrade(&project.0, &[]);
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err(), "the write into a read-only directory should fail");
        for name in SKILLS {
            assert!(
                project.0.join(format!(".claude/skills/{name}/SKILL.md")).is_file(),
                "a failed upgrade deleted .claude/skills/{name} without installing its replacement"
            );
        }
        assert_eq!(fs::read(project.0.join(RECORD)).unwrap(), saved_record);
        for (name, bytes) in SKILLS.iter().zip(sources) {
            assert_eq!(fs::read(project.0.join(format!(".claude/skills/{name}/SKILL.md"))).unwrap(), bytes);
        }
        upgrade(&project.0, &[]).unwrap();
        for name in SKILLS {
            assert_eq!(fs::read_link(project.0.join(skill_link(name))).unwrap().as_os_str(),
                std::ffi::OsStr::new(&format!("../../.agents/skills/{name}")));
            assert!(project.0.join(skill_path(name)).is_file());
        }
    }

    /// Review t120 round 11: canonical writes now precede folder removal, but
    /// all old folders are still removed before any link is created. When a
    /// later folder's removal fails (here a read-only `.claude/skills/evaluate`),
    /// the earlier skills are left with neither folder nor link and the record
    /// unwritten; the retry then treats them as user deletions (`missing`)
    /// instead of completing the migration.
    #[test]
    fn review_r11_failed_folder_removal_does_not_strand_earlier_skills() {
        let project = Project::new();
        let mut record = project.install();
        for name in SKILLS {
            let link = skill_link(name);
            fs::remove_file(project.0.join(&link)).unwrap();
            old_skill(&project, name);
            fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
            let hash = record.files.remove(&skill_path(name)).unwrap();
            record.files.remove(&link);
            record.files.insert(format!("{link}/SKILL.md"), hash);
        }
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        project.commit();
        let last = project.0.join(skill_link(SKILLS[SKILLS.len() - 1]));
        fs::set_permissions(&last, fs::Permissions::from_mode(0o555)).unwrap();
        let result = upgrade(&project.0, &[]);
        fs::set_permissions(&last, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err(), "removing a read-only skill folder should fail");
        upgrade(&project.0, &[]).unwrap();
        for name in SKILLS {
            assert_eq!(
                fs::read_link(project.0.join(skill_link(name)))
                    .ok()
                    .map(PathBuf::into_os_string),
                Some(std::ffi::OsString::from(format!("../../.agents/skills/{name}"))),
                "retry after a failed folder removal left {name} without its link"
            );
        }
        // The successful link leaves the read-only source in a named backup.
        for backup in skill_backups(&project) {
            fs::set_permissions(backup, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn skill_backups(project: &Project) -> Vec<PathBuf> {
        fs::read_dir(project.0.join(".claude/skills"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.file_name().unwrap().to_string_lossy().starts_with(".delegate-skill-backup-"))
            .collect()
    }

    #[test]
    fn skill_link_failure_restores_source_without_touching_siblings() {
        let project = Project::new();
        let source = project.0.join(skill_link("delegate"));
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), b"source bytes").unwrap();
        let sibling = source.with_file_name(".delegate-skill-backup-team");
        fs::create_dir(&sibling).unwrap();
        fs::write(sibling.join("team"), b"untouched").unwrap();
        let error = replace_skill_folder(&project.0, &skill_link("delegate"), "invalid\0target")
            .unwrap_err();
        assert!(error.to_string().contains("create .claude/skills/delegate"));
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), b"source bytes");
        assert_eq!(fs::read(sibling.join("team")).unwrap(), b"untouched");
        assert_eq!(skill_backups(&project), vec![sibling]);
    }

    #[test]
    fn skill_backup_cleanup_failure_keeps_link_and_named_source_backup() {
        let project = Project::new();
        let source = project.0.join(skill_link("delegate"));
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), b"source bytes").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o555)).unwrap();
        let error = replace_skill_folder(
            &project.0, &skill_link("delegate"), "../../.agents/skills/delegate"
        ).unwrap_err();
        let backups = skill_backups(&project);
        assert_eq!(backups.len(), 1);
        assert!(error.to_string().contains(backups[0].to_str().unwrap()));
        assert!(error.to_string().contains("remove that backup manually"));
        assert_eq!(fs::read(backups[0].join("SKILL.md")).unwrap(), b"source bytes");
        assert_eq!(fs::read_link(&source).unwrap().as_os_str(), "../../.agents/skills/delegate");
        assert!(!install_link(&project.0, &skill_link("delegate"), "../../.agents/skills/delegate").unwrap());
        fs::set_permissions(&backups[0], fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn targeted_settings_through_symlink_only_refuses_pending_writes() {
        let project = Project::new();
        project.install();
        let external = project.0.join("external-claude");
        fs::rename(project.0.join(".claude"), &external).unwrap();
        create_symlink("external-claude", project.0.join(".claude")).unwrap();
        let before = fs::read(external.join("settings.json")).unwrap();
        upgrade(&project.0, &[CLAUDE_SETTINGS.into()]).unwrap();
        assert_eq!(fs::read(external.join("settings.json")).unwrap(), before);
        let pending = serde_json::to_vec(&serde_json::json!({
            "statusLine": {"type": "command", "command": STATUS_LINE}, "team": true
        })).unwrap();
        fs::write(external.join("settings.json"), &pending).unwrap();
        let record = fs::read(project.0.join(RECORD)).unwrap();
        let error = upgrade(&project.0, &[CLAUDE_SETTINGS.into()]).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
        assert_eq!(fs::read(external.join("settings.json")).unwrap(), pending);
        assert_eq!(fs::read(project.0.join(RECORD)).unwrap(), record);
    }

    /// Review t120 round 12: `remove_dir_all` is not atomic. With a read-only
    /// `.claude/skills` (but writable skill folders) it empties the first old
    /// skill folder, then fails to remove the folder itself. The retry, after
    /// permissions are restored, sees an empty `.claude/skills/<name>/` and
    /// reports it as a user-deleted SKILL.md (`kept ... edited`) instead of
    /// completing the migration.
    #[test]
    fn review_r12_partial_folder_removal_is_not_reported_as_a_user_edit() {
        let project = Project::new();
        let mut record = project.install();
        for name in SKILLS {
            let link = skill_link(name);
            fs::remove_file(project.0.join(&link)).unwrap();
            old_skill(&project, name);
            fs::remove_dir_all(project.0.join(format!(".agents/skills/{name}"))).unwrap();
            let hash = record.files.remove(&skill_path(name)).unwrap();
            record.files.remove(&link);
            record.files.insert(format!("{link}/SKILL.md"), hash);
        }
        fs::write(project.0.join(RECORD), toml::to_string(&record).unwrap()).unwrap();
        project.commit();
        let skills = project.0.join(".claude/skills");
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o555)).unwrap();
        let result = upgrade(&project.0, &[]);
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err(), "removing from a read-only .claude/skills should fail");
        assert_eq!(fs::read_to_string(project.0.join(RECORD)).unwrap(), toml::to_string(&record).unwrap());
        for name in SKILLS {
            let source = project.0.join(format!("{}/SKILL.md", skill_link(name)));
            let canonical = project.0.join(skill_path(name));
            assert_eq!(fs::read(source).unwrap(), fs::read(canonical).unwrap());
        }
        assert_eq!(skill_backups(&project), Vec::<PathBuf>::new());
        upgrade(&project.0, &[]).unwrap();
        for name in SKILLS {
            assert_eq!(
                fs::read_link(project.0.join(skill_link(name)))
                    .ok()
                    .map(PathBuf::into_os_string),
                Some(std::ffi::OsString::from(format!("../../.agents/skills/{name}"))),
                "retry after a partial folder removal left {name} unmigrated"
            );
        }
    }

    /// Review t120 round 13: a targeted take of an edited non-skill kit file
    /// (the pane) through a symlinked `.claude` or `.claude/skills` must keep
    /// the pre-t120 explicit symlink refusal from `check_parents`. Instead the
    /// `obstructed` prefix (built from `skill_obstruction`, which treats any
    /// symlink as "not a directory") swallows the take, prints false
    /// "is not a directory" advice, and reports "up to date".
    #[test]
    fn review_r13_targeted_pane_take_through_symlinked_claude_refuses() {
        let pane = ".claude/skills/delegate-pane/hooks/hooks.json";
        for ancestor in [".claude", ".claude/skills"] {
            let project = Project::new();
            project.install();
            project.commit();
            let elsewhere = project.0.join("shared");
            fs::rename(project.0.join(ancestor), &elsewhere).unwrap();
            create_symlink(&elsewhere, project.0.join(ancestor)).unwrap();
            let mut edited = fs::read(project.0.join(pane)).unwrap();
            edited.extend_from_slice(b"\nteam edit\n");
            fs::write(project.0.join(pane), &edited).unwrap();
            let result = upgrade(&project.0, &[pane.into()]);
            assert_eq!(fs::read(project.0.join(pane)).unwrap(), edited);
            let error = result
                .expect_err("targeted take through a symlinked ancestor silently did nothing")
                .to_string();
            assert!(error.contains("through symlink"), "{error}");
        }
    }
}
