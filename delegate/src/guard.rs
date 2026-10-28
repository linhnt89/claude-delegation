//! `delegate guard`: Claude Code hooks that keep the lead out of product files (decision 015).
//!
//! Each hook reads one JSON payload on stdin and prints a verdict, or nothing to allow.
//! An edit whose input can't be read is refused; anything else that can't be judged is
//! allowed: the guard is a seatbelt, not a sandbox. A hook takes no lock, registers no
//! checkout, touches no vault or network and writes no task state, so it runs on every
//! edit and inside a worker's sandbox.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use globset::GlobMatcher;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::task::git;

/// The hook a Claude Code event runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Event {
    /// `PreToolUse` for Edit, Write and `NotebookEdit`: refuse the lead's product edits
    PreTool,
    /// `UserPromptSubmit`: record the product files at the start of the turn
    Prompt,
    /// `Stop`: hold the turn while product files differ from that record
    Stop,
}

/// (event, matcher, command) of every guard hook in `.claude/settings.json`.
pub(crate) const HOOKS: [(&str, Option<&str>, &str); 3] = [
    (
        "PreToolUse",
        Some("Edit|Write|NotebookEdit"),
        "delegate guard pre-tool",
    ),
    ("UserPromptSubmit", None, "delegate guard prompt"),
    ("Stop", None, "delegate guard stop"),
];

pub(crate) const ADDED: &str = "added    lead guard hooks to .claude/settings.json: Claude as lead can't change product files (DELEGATE_GUARD=off in the lead's environment turns the guard off)";
pub(crate) const BAD_HOOKS: &str = "kept     .claude/settings.json (couldn't read its \"hooks\": not the shape Claude Code reads; left as is, so the lead guard was not added)";

/// Records older than this belong to sessions long gone.
const RECORD_AGE: Duration = Duration::from_hours(14 * 24);
/// Files a refusal or a held turn names before summing up the rest.
const NAMED: usize = 20;

/// What the hook knows of its environment; tests build it directly.
#[derive(Debug, Default)]
struct Env {
    off: bool,
    worker: Option<String>,
    project: Option<PathBuf>,
    data: PathBuf,
    delegate_home: Option<PathBuf>,
}

impl Env {
    fn current() -> Self {
        Env {
            off: std::env::var_os("DELEGATE_GUARD").is_some_and(|v| v == "off"),
            worker: std::env::var("DELEGATE_WORKER")
                .ok()
                .filter(|id| !id.is_empty()),
            project: std::env::var_os("CLAUDE_PROJECT_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute()),
            data: crate::vault::data_dir(),
            delegate_home: std::env::var_os("DELEGATE_HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty()),
        }
    }
}

/// Run one hook. Never fails: a hook that can't judge allows, except an unreadable edit.
pub fn run(event: Event) {
    let env = Env::current();
    if env.off {
        return;
    }
    // No payload to read (a terminal, a read error) judges like an unreadable one.
    let mut input = String::new();
    if std::io::stdin().is_terminal() || std::io::stdin().read_to_string(&mut input).is_err() {
        input.clear();
    }
    if let Some(verdict) = hook(event, &input, &env) {
        println!("{verdict}");
    }
}

fn hook(event: Event, input: &str, env: &Env) -> Option<Value> {
    if env.off {
        return None;
    }
    let payload: Option<Value> = serde_json::from_str(input).ok();
    let cwd = payload
        .as_ref()
        .and_then(|p| p.get("cwd"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let project = env
        .project
        .clone()
        .or_else(|| cwd.clone())
        .or_else(|| std::env::current_dir().ok());
    let repo = project.as_deref().and_then(Repo::at);
    if repo
        .as_ref()
        .is_some_and(|repo| repo.is_workers(env.worker.as_deref()))
    {
        return None;
    }
    // An edit whose file can't be read from its input can't be judged: refused.
    if event == Event::PreTool
        && let Err(why) = edit_path(payload.as_ref())
    {
        return Some(unreadable(why));
    }
    let (payload, project, repo) = (payload?, project?, repo?);
    let roots = roots(&repo, env);
    let task = roots.iter().any(|r| r.task && r.path == repo.root);
    let rules = Rules {
        globs: crate::config::lead_files(&repo.root).unwrap_or_default(),
    };
    match event {
        Event::PreTool => pre_tool(
            &repo,
            &roots,
            &rules,
            &payload,
            cwd.as_deref().unwrap_or(&project),
        ),
        Event::Prompt => {
            prompt(&repo, task, &rules, &payload, &env.data);
            None
        }
        Event::Stop => stop(&repo, task, &rules, &payload, &env.data),
    }
}

/// The lead's checkout: the git toplevel of Claude's project dir.
#[derive(Debug)]
struct Repo {
    root: PathBuf,
    common: PathBuf,
}

impl Repo {
    fn at(project: &Path) -> Option<Self> {
        let out = git(
            project,
            &[
                "rev-parse",
                "--show-toplevel",
                "--path-format=absolute",
                "--git-common-dir",
            ],
        )
        .ok()?;
        let mut lines = out.lines();
        let root = fs::canonicalize(lines.next()?).ok()?;
        let common = fs::canonicalize(lines.next()?).ok()?;
        Some(Repo { root, common })
    }

    /// A worker is the runner's: `DELEGATE_WORKER` names the task whose worktree this is.
    fn is_workers(&self, worker: Option<&str>) -> bool {
        let Some(id) = worker else {
            return false;
        };
        self.root.file_name().is_some_and(|n| n == "worktree")
            && self
                .root
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == id)
    }
}

/// A worktree the guard judges paths in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Root {
    path: PathBuf,
    /// A task worktree, where `.task/**` is also the lead's.
    task: bool,
}

/// The lead's repo, the checkout's main worktree and its task worktrees
/// (`<task home>/<id>/worktree`). Other worktrees of the checkout are the user's own.
fn roots(repo: &Repo, env: &Env) -> Vec<Root> {
    let mut found: Vec<PathBuf> = Vec::new();
    if let Ok(list) = git(&repo.root, &["worktree", "list", "--porcelain"]) {
        for line in list.lines() {
            if let Some(path) = line.strip_prefix("worktree ")
                && let Ok(path) = fs::canonicalize(path)
            {
                found.push(path);
            }
        }
    }
    let main = repo.common.parent().map(Path::to_path_buf);
    let home = env
        .delegate_home
        .clone()
        .or_else(|| {
            let main = main.as_deref()?;
            crate::checkout::peek(&repo.common, main, &env.data).ok()
        })
        .and_then(|home| fs::canonicalize(home).ok());
    let is_task = |path: &Path| {
        path.file_name().is_some_and(|n| n == "worktree")
            && home
                .as_deref()
                .is_some_and(|home| path.parent().and_then(Path::parent) == Some(home))
    };
    let mut roots = vec![Root {
        path: repo.root.clone(),
        task: is_task(&repo.root),
    }];
    for path in found {
        let task = is_task(&path);
        let is_main = main.as_deref() == Some(path.as_path());
        if (task || is_main) && !roots.iter().any(|r| r.path == path) {
            roots.push(Root { path, task });
        }
    }
    roots
}

/// The lead's own files: default ones and `[guard] lead_files`.
#[derive(Debug, Default)]
struct Rules {
    globs: Vec<GlobMatcher>,
}

impl Rules {
    /// `rel` is a `/`-separated path relative to a worktree root.
    fn is_lead(&self, rel: &str, task: bool) -> bool {
        let parts: Vec<&str> = rel.split('/').collect();
        let default = match parts.as_slice() {
            [name] => Path::new(name).extension().is_some_and(|ext| ext == "md"),
            ["docs", _, ..] | ["evals", "jobs.md"] | ["evals", "scenarios", _, "scenario.md"] => {
                true
            }
            [".task", _, ..] => task,
            _ => false,
        };
        default || self.globs.iter().any(|glob| glob.is_match(rel))
    }
}

/// Resolve `..` and symlinks, including links reached after stepping back out of a
/// folder that doesn't exist yet. New files are judged by their resolved parents.
fn resolve(path: &Path) -> PathBuf {
    resolve_within(path, 0)
}

fn resolve_within(path: &Path, depth: u32) -> PathBuf {
    let mut resolved = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            other => {
                resolved.push(other);
                // Resolve each link before the next component, not just an existing
                // prefix: `missing/../link` can reach an existing (or dangling) link.
                if depth < 40
                    && let Ok(target) = fs::read_link(&resolved)
                {
                    resolved.pop();
                    resolved = resolve_within(&resolved.join(target), depth + 1);
                }
            }
        }
    }
    resolved
}

fn relative(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// The file an Edit, Write or `NotebookEdit` call changes, or why it can't be read.
fn edit_path(payload: Option<&Value>) -> Result<&str, &'static str> {
    let payload = payload.ok_or("not JSON")?;
    let input = payload
        .as_object()
        .ok_or("not a JSON object")?
        .get("tool_input")
        .ok_or("no tool_input")?
        .as_object()
        .ok_or("tool_input is not an object")?;
    let path = ["file_path", "notebook_path"]
        .iter()
        .find_map(|key| input.get(*key))
        .ok_or("no file_path or notebook_path")?
        .as_str()
        .ok_or("the file path is not a string")?;
    if path.is_empty() {
        return Err("the file path is empty");
    }
    Ok(path)
}

fn unreadable(why: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!(
                "delegate guard: couldn't read this edit's input ({why}), so it can't tell \
                 which file the edit changes and refused it. Claude as lead doesn't change \
                 product files: if this is one, write a spec for it and /delegate it to a worker."
            ),
        }
    })
}

fn pre_tool(
    repo: &Repo,
    roots: &[Root],
    rules: &Rules,
    payload: &Value,
    base: &Path,
) -> Option<Value> {
    let given = edit_path(Some(payload)).ok()?;
    let resolved = resolve(&base.join(given));
    let root = roots
        .iter()
        .filter(|r| resolved.starts_with(&r.path))
        .max_by_key(|r| r.path.components().count())?;
    let rel = relative(&resolved, &root.path)?;
    if !rel.is_empty() && rules.is_lead(&rel, root.task) {
        return None;
    }
    let name = if root.path == repo.root {
        if given == rel {
            given.to_owned()
        } else {
            format!("{given} ({rel})")
        }
    } else {
        format!("{given} ({rel} in {})", root.path.display())
    };
    let what = if root.task && root.path != repo.root {
        "a product file of a task worktree, and only its worker changes it (decision 004)"
    } else {
        "a product file, and Claude as lead doesn't change product files"
    };
    Some(json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!(
                "delegate guard: {name} is {what}, however small the change. \
                 Write a spec for it and /delegate it to a worker."
            ),
        }
    }))
}

/// Product files at the start of a turn.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    head: Option<String>,
    /// Modified or untracked product files and their content (see [`content`]).
    files: BTreeMap<String, String>,
}

fn session(payload: &Value) -> Option<String> {
    let id = payload.get("session_id")?.as_str()?;
    if id.is_empty() {
        return None;
    }
    let plain = id.len() <= 128
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    Some(if plain {
        id.to_owned()
    } else {
        crate::sha256::hex(id.as_bytes())
    })
}

/// Records live beside, never inside, the checkout homes that `sweep` cleans.
fn record_dir(data: &Path, repo: &Repo) -> PathBuf {
    let key = crate::sha256::hex(repo.common.as_os_str().as_encoded_bytes());
    data.join("guard").join(&key[..32])
}

fn head(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "--verify", "-q", "HEAD"]).ok()
}

/// A file's content as the guard compares it: missing, a symlink target or a digest.
fn content(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Err(_) => "missing".into(),
        Ok(meta) if meta.file_type().is_symlink() => fs::read_link(path).map_or_else(
            |_| "unreadable".into(),
            |target| format!("link:{}", target.to_string_lossy()),
        ),
        Ok(meta) if meta.is_file() => fs::read(path).map_or_else(
            |_| "unreadable".into(),
            |bytes| format!("file:{}", crate::sha256::hex(&bytes)),
        ),
        Ok(_) => "other".into(),
    }
}

/// Product files `git status` lists as modified or untracked (not ignored).
fn dirty(root: &Path, task: bool, rules: &Rules) -> Option<BTreeSet<String>> {
    let out = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )
    .ok()?;
    Some(
        out.split('\0')
            .filter_map(|entry| entry.get(3..))
            .map(|path| path.trim_end_matches('/').to_owned())
            .filter(|path| !path.is_empty() && !rules.is_lead(path, task))
            .collect(),
    )
}

fn prompt(repo: &Repo, task: bool, rules: &Rules, payload: &Value, data: &Path) {
    let Some(session) = session(payload) else {
        return;
    };
    let Some(files) = dirty(&repo.root, task, rules) else {
        return;
    };
    let record = Record {
        head: head(&repo.root),
        files: files
            .into_iter()
            .map(|rel| {
                let hash = content(&repo.root.join(&rel));
                (rel, hash)
            })
            .collect(),
    };
    let dir = record_dir(data, repo);
    prune(&dir);
    let _ = save(&dir.join(format!("{session}.json")), &record);
}

fn save(path: &Path, record: &Record) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::checkout::atomic_write(path, &serde_json::to_vec(record)?)
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age > RECORD_AGE);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn stop(repo: &Repo, task: bool, rules: &Rules, payload: &Value, data: &Path) -> Option<Value> {
    // `stop_hook_active` is ignored: files still changed hold the turn again.
    let session = session(payload)?;
    let path = record_dir(data, repo).join(format!("{session}.json"));
    let record: Record = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let root = &repo.root;
    let now = head(root);
    let mut candidates: BTreeSet<String> = record.files.keys().cloned().collect();
    candidates.extend(dirty(root, task, rules)?);
    let moved = now.is_some() && now != record.head;
    if moved {
        let committed = match &record.head {
            Some(start) => git(
                root,
                &["diff", "--name-only", "-z", "--no-renames", start, "HEAD"],
            ),
            None => git(root, &["ls-tree", "-r", "--name-only", "-z", "HEAD"]),
        }
        .ok()?;
        candidates.extend(
            committed
                .split('\0')
                .filter(|p| !p.is_empty() && !rules.is_lead(p, task))
                .map(str::to_owned),
        );
    }
    let unrecorded: Vec<String> = candidates
        .iter()
        .filter(|p| !record.files.contains_key(*p))
        .cloned()
        .collect();
    let at_start = match &record.head {
        Some(start) if !unrecorded.is_empty() => tree_contents(root, start, &unrecorded)?,
        _ => BTreeMap::new(),
    };
    let mut changed: Vec<(String, String)> = Vec::new();
    for rel in candidates {
        let current = content(&root.join(&rel));
        let start = record
            .files
            .get(&rel)
            .or_else(|| at_start.get(&rel))
            .map_or("missing", String::as_str);
        if current != start {
            changed.push((rel, current));
        }
    }
    if moved && !changed.is_empty() {
        let range = record
            .head
            .as_ref()
            .map_or_else(|| "HEAD".to_owned(), |start| format!("{start}..HEAD"));
        let files: Vec<String> = changed.iter().map(|(rel, _)| rel.clone()).collect();
        for commit in exempt_commits(root, &range, &files)? {
            let contents = tree_contents(root, &commit, &files)?;
            changed.retain(|(rel, current)| {
                contents.get(rel).map_or("missing", String::as_str) != current
            });
            if changed.is_empty() {
                break;
            }
        }
    }
    if changed.is_empty() {
        return None;
    }
    let names: Vec<&str> = changed
        .iter()
        .take(NAMED)
        .map(|(rel, _)| rel.as_str())
        .collect();
    let more = changed.len().saturating_sub(NAMED);
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Some(json!({
        "decision": "block",
        "reason": format!(
            "delegate guard: this turn changed product files: {}{more}. Claude as lead \
             doesn't change product files, not even through the shell. Undo the change \
             (restore them as they were when the turn started), then write a spec and \
             /delegate it to a worker.",
            names.join(", ")
        ),
    }))
}

/// Commits in `range` whose changes aren't the lead's: `delegate merge` commits (a
/// `Worker:` trailer) and commits on a remote (a pull). Only those touching `files`.
fn exempt_commits(root: &Path, range: &str, files: &[String]) -> Option<Vec<String>> {
    let mut touching = vec![
        "--literal-pathspecs",
        "log",
        "--full-history",
        "--format=%H%x1f%(trailers:key=Worker,valueonly,separator=%x2c)%x1e",
        range,
        "--",
    ];
    touching.extend(files.iter().map(String::as_str));
    let log = git(root, &touching).ok()?;
    let local: BTreeSet<String> = git(root, &["rev-list", range, "--not", "--remotes"])
        .ok()?
        .lines()
        .map(str::to_owned)
        .collect();
    Some(
        log.split('\x1e')
            .filter_map(|entry| {
                let (commit, trailer) = entry.trim().split_once('\x1f')?;
                let worker = !trailer.trim().is_empty();
                (worker || !local.contains(commit)).then(|| commit.to_owned())
            })
            .collect(),
    )
}

/// The content of each of `paths` in `commit`, as [`content`] reads the worktree;
/// a path missing from the commit is left out.
fn tree_contents(root: &Path, commit: &str, paths: &[String]) -> Option<BTreeMap<String, String>> {
    let wanted: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    let mut args = vec!["--literal-pathspecs", "ls-tree", "-r", "-z", commit];
    if paths.len() <= 500 {
        args.push("--");
        args.extend(wanted.iter().copied());
    }
    let listing = git(root, &args).ok()?;
    let mut blobs: Vec<(String, bool, String)> = Vec::new();
    let mut contents = BTreeMap::new();
    for entry in listing.split('\0').filter(|e| !e.is_empty()) {
        let (meta, path) = entry.split_once('\t')?;
        if !wanted.contains(path) {
            continue;
        }
        let mut meta = meta.split(' ');
        let (mode, kind, oid) = (meta.next()?, meta.next()?, meta.next()?);
        if kind == "blob" {
            blobs.push((path.to_owned(), mode == "120000", oid.to_owned()));
        } else {
            contents.insert(path.to_owned(), "other".to_owned());
        }
    }
    if blobs.is_empty() {
        return Some(contents);
    }
    let input = blobs.iter().fold(String::new(), |mut input, (_, _, oid)| {
        input.push_str(oid);
        input.push('\n');
        input
    });
    let bodies = cat_blobs(root, input)?;
    if bodies.len() != blobs.len() {
        return None;
    }
    for ((path, link, _), body) in blobs.into_iter().zip(bodies) {
        let value = if link {
            format!("link:{}", String::from_utf8_lossy(&body))
        } else {
            format!("file:{}", crate::sha256::hex(&body))
        };
        contents.insert(path, value);
    }
    Some(contents)
}

/// Blob bodies, in order, from one `git cat-file --batch`.
fn cat_blobs(root: &Path, input: String) -> Option<Vec<Vec<u8>>> {
    let mut child = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["cat-file", "--batch"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    // Write from a thread: the output pipe may fill before all of the input is read.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child.wait_with_output().ok()?;
    writer.join().ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let mut out = output.stdout.as_slice();
    let mut bodies = Vec::new();
    while !out.is_empty() {
        let end = out.iter().position(|&b| b == b'\n')?;
        let header = std::str::from_utf8(&out[..end]).ok()?;
        let size: usize = header.rsplit(' ').next()?.parse().ok()?;
        let body = out.get(end + 1..end + 1 + size)?;
        bodies.push(body.to_vec());
        out = out.get(end + 2 + size..).unwrap_or_default();
    }
    Some(bodies)
}

/// Outcome of adding the guard's hooks to `.claude/settings.json`'s bytes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Install {
    /// The new settings, to write.
    Added(Vec<u8>),
    /// Every hook is there already.
    Present,
    NotJson,
    NotObject,
    /// `hooks` (or one event's list) isn't the shape Claude Code reads.
    BadHooks,
}

/// Add each guard hook that isn't there; everything else is kept.
pub(crate) fn install(existing: Option<&[u8]>) -> Install {
    let mut settings = match existing {
        None => json!({}),
        Some(bytes) => match serde_json::from_slice::<Value>(bytes) {
            Ok(value) => value,
            Err(_) => return Install::NotJson,
        },
    };
    let Some(object) = settings.as_object_mut() else {
        return Install::NotObject;
    };
    let hooks = object.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Install::BadHooks;
    };
    if HOOKS
        .iter()
        .any(|(event, _, _)| hooks.get(*event).is_some_and(|v| !v.is_array()))
    {
        return Install::BadHooks;
    }
    let mut added = false;
    for (event, matcher, command) in HOOKS {
        let Some(groups) = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
        else {
            return Install::BadHooks;
        };
        let present = groups.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|list| {
                    list.iter()
                        .any(|h| h.get("command").and_then(Value::as_str) == Some(command))
                })
        });
        if present {
            continue;
        }
        let mut group = json!({"hooks": [{"type": "command", "command": command}]});
        if let Some(matcher) = matcher {
            group["matcher"] = json!(matcher);
        }
        groups.push(group);
        added = true;
    }
    if !added {
        return Install::Present;
    }
    match serde_json::to_string_pretty(&settings) {
        Ok(text) => Install::Added(format!("{text}\n").into_bytes()),
        Err(_) => Install::NotJson,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let f = Self(
                fs::canonicalize(std::env::temp_dir())
                    .unwrap()
                    .join(format!("delegate-guard-{}", uuid::Uuid::new_v4())),
            );
            fs::create_dir_all(f.root()).unwrap();
            f.git(&["init", "-q"]);
            for (path, body) in [
                ("PLAN.md", "# Plan\n"),
                ("docs/decisions/009-x.md", "# Decision\n"),
                ("evals/jobs.md", "# Jobs\n"),
                ("evals/scenarios/S001-shop/scenario.md", "# Scenario\n"),
                ("evals/scenarios/S001-shop/check", "#!/bin/sh\n"),
                ("shop/pricing.py", "# Pricing\n"),
                ("shop/cart.py", "# Cart\n"),
                ("shop/notes.md", "# Notes\n"),
            ] {
                f.write(path, body);
            }
            symlink("../shop", f.root().join("docs/link")).unwrap();
            f.commit("Shop");
            f
        }

        fn root(&self) -> PathBuf {
            self.0.join("shop-repo")
        }

        fn data(&self) -> PathBuf {
            self.0.join("data/delegate")
        }

        fn write(&self, path: &str, body: &str) {
            let path = self.root().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }

        fn git(&self, args: &[&str]) -> String {
            git(&self.root(), args).unwrap()
        }

        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&[
                "-c",
                "user.name=Guard test",
                "-c",
                "user.email=guard@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                message,
            ]);
            self.git(&["rev-parse", "HEAD"])
        }

        fn env(&self) -> Env {
            Env {
                project: Some(self.root()),
                data: self.data(),
                ..Env::default()
            }
        }

        fn call(&self, event: Event, extra: &Value, env: &Env) -> Option<Value> {
            let mut payload = json!({"session_id": "guard-test", "cwd": self.root()});
            for (key, value) in extra.as_object().unwrap() {
                payload[key] = value.clone();
            }
            hook(event, &payload.to_string(), env)
        }

        fn edit(&self, path: &str) -> Option<Value> {
            self.edit_as(path, "Edit", &self.env())
        }

        fn edit_as(&self, path: &str, tool: &str, env: &Env) -> Option<Value> {
            let key = if tool == "NotebookEdit" {
                "notebook_path"
            } else {
                "file_path"
            };
            self.call(
                Event::PreTool,
                &json!({"tool_name": tool, "tool_input": {key: path}}),
                env,
            )
        }

        fn allowed(&self, path: &str) -> bool {
            self.edit(path).is_none()
        }

        fn start(&self) {
            assert_eq!(self.call(Event::Prompt, &json!({}), &self.env()), None);
        }

        fn stop(&self) -> Option<String> {
            self.call(Event::Stop, &json!({"stop_hook_active": true}), &self.env())
                .map(|v| {
                    assert_eq!(v["decision"], "block");
                    v["reason"].as_str().unwrap().to_owned()
                })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lead_files_and_product_files() {
        let f = Fixture::new();
        for lead in [
            "PLAN.md",
            "CLAUDE.md",
            "./AGENTS.md",
            "docs/decisions/009-x.md",
            "docs/new folder/deeper/notes.py",
            "evals/jobs.md",
            "evals/scenarios/S001-shop/scenario.md",
            "evals/scenarios/S002-new/scenario.md",
        ] {
            assert!(f.allowed(lead), "{lead} should be the lead's");
        }
        for product in [
            "shop/pricing.py",
            "shop/notes.md",
            "shop/api/v2/handlers.py",
            "tests/test_split.py",
            "evals/run",
            "evals/scenarios/S001-shop/check",
            "evals/scenarios/S001-shop/nested/scenario.md",
            "evals/notes.md",
            ".claude/settings.json",
            ".delegate.toml",
            ".task/REPORT.md",
        ] {
            let verdict = f
                .edit(product)
                .unwrap_or_else(|| panic!("{product} allowed"));
            let reason = verdict["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap();
            assert_eq!(verdict["hookSpecificOutput"]["permissionDecision"], "deny");
            assert_eq!(verdict["hookSpecificOutput"]["hookEventName"], "PreToolUse");
            assert!(reason.contains(product) && reason.contains("spec"));
            assert!(reason.contains("/delegate"), "{reason}");
        }
        for tool in ["Write", "NotebookEdit"] {
            assert!(f.edit_as("shop/prices.ipynb", tool, &f.env()).is_some());
            assert!(f.edit_as("docs/prices.ipynb", tool, &f.env()).is_none());
        }
    }

    #[test]
    fn paths_are_resolved_before_judging() {
        let f = Fixture::new();
        let root = f.root();
        let abs = |p: &str| root.join(p).to_string_lossy().into_owned();
        assert!(!f.allowed(&abs("shop/pricing.py")));
        assert!(f.allowed(&abs("PLAN.md")));
        assert!(!f.allowed("./shop/pricing.py"));
        assert!(!f.allowed("docs/../shop/pricing.py"));
        assert!(!f.allowed("docs/link/pricing.py"));
        assert!(!f.allowed("docs/link/new folder/new.py"));
        assert!(f.allowed("shop/../docs/new.py"));
        // New files in new folders, spelled through a folder that doesn't exist yet.
        assert!(!f.allowed("shop/new/../newer/x.py"));
        assert!(f.allowed("docs/new/../newer/x.py"));
        // A symlink in a product folder into the lead's folder, and out of the repo.
        let outside = f.0.join("outside");
        fs::create_dir_all(&outside).unwrap();
        symlink("../docs", root.join("shop/lead-docs")).unwrap();
        symlink(&outside, root.join("shop/outside")).unwrap();
        assert!(f.allowed("shop/lead-docs/notes.md"));
        assert!(f.allowed("shop/outside/notes.py"));
        assert!(f.allowed(&outside.join("scratch.py").to_string_lossy()));
        assert!(f.allowed(&f.0.join("scratchpad.py").to_string_lossy()));
        // A dangling symlink is judged by where writing through it lands.
        symlink("../shop/ghost.py", root.join("docs/ghost.py")).unwrap();
        assert!(!f.allowed("docs/ghost.py"));
        // A root `*.md` symlinked into the product is product.
        symlink("shop/notes.md", root.join("NOTES.md")).unwrap();
        assert!(!f.allowed("NOTES.md"));
    }

    #[test]
    fn a_new_folder_then_dotdot_still_resolves_the_symlink_after_it() {
        let f = Fixture::new();
        symlink("../docs", f.root().join("shop/lead-docs")).unwrap();
        // Write creates `docs/new/`, steps back out of it, then writes through `docs/link`
        // into the product: `shop/pricing.py` is what gets changed.
        assert!(!f.allowed("docs/new/../link/pricing.py"));
        // The mirror: the write lands in the lead's docs through `shop/lead-docs`.
        assert!(f.allowed("shop/new/../lead-docs/notes.md"));
    }

    #[test]
    fn lead_files_from_delegate_toml() {
        let f = Fixture::new();
        assert!(!f.allowed("evals/lib/streams/pi.steps"));
        f.write(
            ".delegate.toml",
            "[[rules]]\npaths = [\"shop/**\"]\nreview = \"always\"\n\n[guard]\nlead_files = [\"evals/lib/streams/*.steps\"]\n",
        );
        assert!(f.allowed("evals/lib/streams/pi.steps"));
        assert!(!f.allowed("evals/lib/streams/pi.jsonl"));
        assert!(!f.allowed("evals/lib/common.sh"));
        f.write(".delegate.toml", "[guard]\nlead_files = 3\n");
        assert!(!f.allowed("evals/lib/streams/pi.steps"));
    }

    #[test]
    fn off_switch_workers_and_unjudgeable_input_allow() {
        // Edits are refused when their input can't be read (below); the rest allow.
        let f = Fixture::new();
        let off = Env {
            off: true,
            ..f.env()
        };
        assert!(f.edit_as("shop/pricing.py", "Edit", &off).is_none());
        // A stray DELEGATE_WORKER outside that task's worktree is still a lead.
        let stray = Env {
            worker: Some("t001".into()),
            ..f.env()
        };
        assert!(f.edit_as("shop/pricing.py", "Edit", &stray).is_some());
        for event in [Event::Prompt, Event::Stop] {
            assert_eq!(hook(event, "not json", &f.env()), None);
            assert_eq!(hook(event, "", &f.env()), None);
            assert_eq!(hook(event, "[]", &f.env()), None);
        }
        let nowhere = Env {
            project: Some(f.0.clone()),
            ..f.env()
        };
        let payload = json!({"cwd": f.0, "tool_input": {"file_path": "x.py"}});
        assert_eq!(hook(Event::PreTool, &payload.to_string(), &nowhere), None);
    }

    #[test]
    fn unreadable_edits_are_refused_except_for_workers_and_off() {
        let f = Fixture::new();
        let wt = task_worktree(&f);
        let unreadable = [
            String::new(),
            "not json".into(),
            "[]".into(),
            "{}".into(),
            json!({"cwd": f.root()}).to_string(),
            json!({"cwd": f.root(), "tool_input": []}).to_string(),
            json!({"cwd": f.root(), "tool_input": "shop/pricing.py"}).to_string(),
            json!({"cwd": f.root(), "tool_input": {}}).to_string(),
            json!({"cwd": f.root(), "tool_input": {"content": "x"}}).to_string(),
            json!({"cwd": f.root(), "tool_input": {"file_path": ""}}).to_string(),
            json!({"cwd": f.root(), "tool_input": {"file_path": 5}}).to_string(),
            json!({"cwd": f.root(), "tool_input": {"notebook_path": null}}).to_string(),
        ];
        let not_a_repo = Env {
            project: Some(f.0.clone()),
            ..f.env()
        };
        for input in &unreadable {
            for env in [&f.env(), &not_a_repo] {
                let verdict = hook(Event::PreTool, input, env)
                    .unwrap_or_else(|| panic!("allowed an unreadable edit: {input:?}"));
                let out = &verdict["hookSpecificOutput"];
                assert_eq!(out["permissionDecision"], "deny", "{input}");
                let reason = out["permissionDecisionReason"].as_str().unwrap();
                assert!(
                    reason.contains("couldn't read this edit's input"),
                    "{reason}"
                );
            }
            let off = Env {
                off: true,
                ..f.env()
            };
            assert_eq!(hook(Event::PreTool, input, &off), None, "{input}");
            let worker = Env {
                worker: Some("t001".into()),
                project: Some(wt.clone()),
                delegate_home: Some(f.0.join("task-home")),
                ..f.env()
            };
            assert_eq!(hook(Event::PreTool, input, &worker), None, "{input}");
            for event in [Event::Prompt, Event::Stop] {
                assert_eq!(hook(event, input, &f.env()), None, "{input}");
            }
        }
    }

    /// A task worktree of this checkout, at `<task home>/<id>/worktree`.
    fn task_worktree(f: &Fixture) -> PathBuf {
        let home = f.0.join("task-home");
        let wt = home.join("t001/worktree");
        fs::create_dir_all(wt.parent().unwrap()).unwrap();
        f.git(&["worktree", "add", "-q", "--detach", &wt.to_string_lossy()]);
        wt
    }

    #[test]
    fn task_worktrees_follow_the_same_rules_and_workers_pass() {
        let f = Fixture::new();
        let wt = task_worktree(&f);
        let other = f.0.join("users-own-worktree");
        f.git(&[
            "worktree",
            "add",
            "-q",
            "--detach",
            &other.to_string_lossy(),
        ]);
        let env = Env {
            delegate_home: Some(f.0.join("task-home")),
            ..f.env()
        };
        let edit = |path: &Path| f.edit_as(&path.to_string_lossy(), "Write", &env);
        assert!(edit(&wt.join("shop/pricing.py")).is_some());
        assert!(edit(&wt.join(".task/REVIEW.patch")).is_none());
        assert!(edit(&wt.join("PLAN.md")).is_none());
        assert!(edit(&other.join("shop/pricing.py")).is_none());
        // The runner's worker in its own worktree: every hook allows, silently.
        let worker = Env {
            worker: Some("t001".into()),
            project: Some(wt.clone()),
            ..env
        };
        let payload = |extra: Value| {
            let mut p = json!({"session_id": "w", "cwd": wt});
            for (k, v) in extra.as_object().unwrap() {
                p[k] = v.clone();
            }
            p.to_string()
        };
        let edit = payload(json!({"tool_input": {"file_path": "shop/pricing.py"}}));
        assert_eq!(hook(Event::PreTool, &edit, &worker), None);
        assert_eq!(hook(Event::Prompt, &payload(json!({})), &worker), None);
        fs::write(wt.join("shop/pricing.py"), "# worker\n").unwrap();
        assert_eq!(hook(Event::Stop, &payload(json!({})), &worker), None);
        // A lead started by hand in that worktree is still a lead.
        let lead = Env {
            worker: None,
            ..worker
        };
        assert!(hook(Event::PreTool, &edit, &lead).is_some());
        let task_file = payload(json!({"tool_input": {"file_path": ".task/SPEC.md"}}));
        assert_eq!(hook(Event::PreTool, &task_file, &lead), None);
    }

    #[test]
    fn shell_edits_hold_the_turn_until_undone() {
        let f = Fixture::new();
        // No record for the session: allow.
        assert_eq!(f.stop(), None);
        f.write("shop/pricing.py", "# user edit before the turn\n");
        f.start();
        f.write("shop/cart.py", "# lead shell change\n");
        f.write("shop/new/module.py", "# lead new file\n");
        f.write("PLAN.md", "# Plan\nnote\n");
        f.write("docs/more.md", "more\n");
        let reason = f.stop().expect("turn held");
        assert!(reason.contains("shop/cart.py") && reason.contains("shop/new/module.py"));
        assert!(!reason.contains("PLAN.md") && !reason.contains("pricing"));
        assert!(reason.contains("Undo") && reason.contains("delegate"));
        // Still changed: held again, whatever stop_hook_active says.
        assert!(f.stop().is_some());
        f.write("shop/cart.py", "# Cart\n");
        fs::remove_file(f.root().join("shop/new/module.py")).unwrap();
        assert_eq!(f.stop(), None);
        // Deleting a product file is a change too; a commit by the lead as well.
        fs::remove_file(f.root().join("shop/cart.py")).unwrap();
        assert!(f.stop().unwrap().contains("shop/cart.py"));
        f.write("shop/cart.py", "# Cart\n");
        f.write("shop/cart.py", "# committed by the lead\n");
        f.commit("Lead commit");
        assert!(f.stop().unwrap().contains("shop/cart.py"));
        // A user's edit between turns is the next turn's baseline.
        f.start();
        assert_eq!(f.stop(), None);
    }

    #[test]
    fn worker_merges_and_pulls_are_not_blamed() {
        let f = Fixture::new();
        f.start();
        f.write("shop/cart.py", "# merged worker change\n");
        f.commit("t001: Cart\n\nWorker: pi:gpt-test@high");
        f.write("shop/pricing.py", "# pulled change\n");
        let pulled = f.commit("Upstream pricing");
        f.git(&["update-ref", "refs/remotes/origin/main", &pulled]);
        assert_eq!(f.stop(), None);
        // A plain local commit in the same turn is the lead's.
        f.write("shop/notes.md", "# lead\n");
        f.commit("Local");
        let reason = f.stop().unwrap();
        assert!(reason.contains("shop/notes.md"));
        assert!(!reason.contains("cart") && !reason.contains("pricing"));
        // Changing a merged file afterwards is blamed again.
        f.git(&["checkout", "-q", "HEAD~1", "--", "shop/notes.md"]);
        f.write("shop/cart.py", "# lead change after the merge\n");
        assert!(f.stop().unwrap().contains("shop/cart.py"));
    }

    #[test]
    fn quiet_turns_have_no_output_and_leave_no_task_state() {
        let f = Fixture::new();
        f.start();
        f.write("PLAN.md", "# Plan\nonly lead files\n");
        assert_eq!(f.stop(), None);
        assert!(f.edit("PLAN.md").is_none());
        let entries: Vec<_> = fs::read_dir(f.data())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("guard")]);
        assert!(!f.root().join(".git/delegate-checkout-id").exists());
    }

    #[test]
    fn install_adds_each_hook_once_and_keeps_the_rest() {
        let Install::Added(fresh) = install(None) else {
            panic!("no hooks added to missing settings");
        };
        assert_eq!(install(Some(&fresh)), Install::Present);
        let ours = json!({
            "permissions": {"allow": ["Bash(ls:*)"]},
            "hooks": {"PreToolUse": [{"matcher": "Edit|Write", "hooks": [
                {"type": "command", "command": ": ours"}]}]}
        });
        let Install::Added(bytes) = install(Some(ours.to_string().as_bytes())) else {
            panic!("no hooks added");
        };
        let after: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(after["permissions"], ours["permissions"]);
        assert_eq!(
            after["hooks"]["PreToolUse"][0],
            ours["hooks"]["PreToolUse"][0]
        );
        assert_eq!(
            after["hooks"]["PreToolUse"][1]["matcher"],
            "Edit|Write|NotebookEdit"
        );
        assert_eq!(
            after["hooks"]["Stop"][0]["hooks"][0]["command"],
            "delegate guard stop"
        );
        assert_eq!(install(Some(&bytes)), Install::Present);
        assert_eq!(install(Some(b"{ nope")), Install::NotJson);
        assert_eq!(install(Some(b"[]")), Install::NotObject);
        assert_eq!(install(Some(b"{\"hooks\": []}")), Install::BadHooks);
        assert_eq!(
            install(Some(b"{\"hooks\": {\"Stop\": {}}}")),
            Install::BadHooks
        );
    }
}
