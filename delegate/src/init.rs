//! `delegate init`: add project files; machine configuration belongs to `setup`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Result, bail};

use crate::config::user_config_path;
use crate::task::git;

// Keep include_str! calls on one line: historical kit readers parse this list.
#[rustfmt::skip]
pub(crate) const PROJECT_FILES: &[(&str, &str)] = &[
    ("AGENTS.md", include_str!("../templates/project/AGENTS.md")),
    ("CLAUDE.md", include_str!("../templates/project/CLAUDE.md")),
    ("PLAN.md", include_str!("../templates/project/PLAN.md")),
    (
        ".delegate.toml",
        include_str!("../templates/project/.delegate.toml"),
    ),
    (
        "docs/decisions/000-template.md",
        include_str!("../templates/project/docs/decisions/000-template.md"),
    ),
    (
        ".claude/skills/delegate/SKILL.md",
        include_str!("../templates/project/.claude/skills/delegate/SKILL.md"),
    ),
    (
        ".claude/skills/distill/SKILL.md",
        include_str!("../templates/project/.claude/skills/distill/SKILL.md"),
    ),
    (
        ".claude/skills/evaluate/SKILL.md",
        include_str!("../templates/project/.claude/skills/evaluate/SKILL.md"),
    ),
    (
        ".claude/skills/delegate-pane/.claude-plugin/plugin.json",
        include_str!("../templates/project/.claude/skills/delegate-pane/.claude-plugin/plugin.json"),
    ),
    (
        ".claude/skills/delegate-pane/hooks/hooks.json",
        include_str!("../templates/project/.claude/skills/delegate-pane/hooks/hooks.json"),
    ),
    (
        ".claude/skills/delegate-pane/hooks/register.tsx",
        include_str!("../templates/project/.claude/skills/delegate-pane/hooks/register.tsx"),
    ),
    (
        ".claude/skills/delegate-pane/hooks/register.test.tsx",
        include_str!("../templates/project/.claude/skills/delegate-pane/hooks/register.test.tsx"),
    ),
    (
        ".claude/skills/delegate-pane/types/index.d.ts",
        include_str!("../templates/project/.claude/skills/delegate-pane/types/index.d.ts"),
    ),
    (
        "evals/README.md",
        include_str!("../templates/project/evals/README.md"),
    ),
    (
        "evals/jobs.md",
        include_str!("../templates/project/evals/jobs.md"),
    ),
    (
        "evals/scenarios/_template/scenario.md",
        include_str!("../templates/project/evals/scenarios/_template/scenario.md"),
    ),
    ("evals/run", include_str!("../templates/project/evals/run")),
];

/// Project files that must be executable.
pub(crate) const EXECUTABLE: &[&str] = &["evals/run"];

/// Claude Code's settings file and the status line command we install there.
const CLAUDE_SETTINGS: &str = ".claude/settings.json";
const STATUS_LINE: &str = "delegate statusline";

fn status_line() -> serde_json::Value {
    serde_json::json!({"type": "command", "command": STATUS_LINE})
}

/// Add `statusLine` to Claude Code's settings without ever replacing the user's
/// own line or their other settings; an unreadable file is left untouched.
fn configure_status_line(root: &Path) -> Result<()> {
    let path = root.join(CLAUDE_SETTINGS);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let settings = serde_json::json!({"statusLine": status_line()});
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&settings)?),
        )?;
        println!("added    {CLAUDE_SETTINGS} (Claude Code status line)");
        return Ok(());
    }
    let Ok(mut settings) = serde_json::from_slice::<serde_json::Value>(&fs::read(&path)?) else {
        println!(
            "kept     {CLAUDE_SETTINGS} (not valid JSON; left as is, fix it and run delegate init again)"
        );
        return Ok(());
    };
    let Some(object) = settings.as_object_mut() else {
        println!("kept     {CLAUDE_SETTINGS} (not a JSON object; add the status line by hand)");
        return Ok(());
    };
    if object.contains_key("statusLine") {
        println!(
            "kept     {CLAUDE_SETTINGS} (has its own status line; run `{STATUS_LINE}` from yours to show tasks)"
        );
        return Ok(());
    }
    object.insert("statusLine".into(), status_line());
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&settings)?),
    )?;
    println!("added    status line to {CLAUDE_SETTINGS}");
    Ok(())
}

fn write_new(path: &Path, content: &str, label: &str) -> Result<()> {
    if path.exists() {
        println!("skipped  {label} (exists)");
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        if EXECUTABLE.contains(&label) {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
        println!("added    {label}");
    }
    Ok(())
}

pub fn init(dir: &Path) -> Result<()> {
    let Ok(root) = git(dir, &["rev-parse", "--show-toplevel"]) else {
        bail!("{} is not inside a git repository", dir.display());
    };
    let root = Path::new(&root);
    let mut record = crate::upgrade::Record::new();
    for (rel, content) in PROJECT_FILES {
        let path = root.join(rel);
        if !path.exists() {
            record
                .files
                .insert((*rel).into(), crate::sha256::hex(content.as_bytes()));
        }
        write_new(&path, content, rel)?;
    }
    // Like every other init file, an existing record is never overwritten.
    write_new(
        &root.join(crate::upgrade::RECORD),
        &toml::to_string(&record)?,
        crate::upgrade::RECORD,
    )?;
    configure_status_line(root)?;
    println!(
        "\nNext: fill in AGENTS.md (commands first) and .delegate.toml rules, run /evaluate define, then commit (including .delegate-kit.toml)."
    );
    if !user_config_path().is_file() {
        println!("this machine isn't set up: run delegate setup");
    }
    Ok(())
}
