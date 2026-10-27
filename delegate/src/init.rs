//! `delegate init`: add project files; machine configuration belongs to `setup`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

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

/// Claude Code's user-owned settings and the optional delegate status line command.
pub(crate) const CLAUDE_SETTINGS: &str = ".claude/settings.json";
pub(crate) const STATUS_LINE: &str = "delegate statusline";

/// Explain the optional status line without changing or creating settings.
fn configure_status_line(root: &Path) -> Result<()> {
    println!(
        "status line: delegate adds none; tasks show above the prompt (add `{STATUS_LINE}` to a status line to see them there too)"
    );
    let path = root.join(CLAUDE_SETTINGS);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("read {CLAUDE_SETTINGS}; check permissions and retry"));
        }
    };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Err(_) => println!(
            "kept     {CLAUDE_SETTINGS} (not valid JSON; left as is, fix it and run delegate init again)"
        ),
        Ok(settings) if !settings.is_object() => {
            println!("kept     {CLAUDE_SETTINGS} (not a JSON object; left as is)");
        }
        Ok(_) => {}
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_advice_never_changes_or_creates_settings() {
        let root =
            std::env::temp_dir().join(format!("delegate-init-test-{}", uuid::Uuid::new_v4()));
        configure_status_line(&root).unwrap();
        assert!(!root.exists());
        let path = root.join(CLAUDE_SETTINGS);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for bytes in [
            "{\"permissions\": {\"allow\": [\"Bash(ls:*)\"]}}\n",
            "{\"statusLine\": {\"command\": \"delegate statusline\"}}\n",
            "{\"statusLine\": {\"command\": \"~/bin/my-line\"}}\n",
            "{\"statusLine\": null}\n",
            "{invalid json\n",
            "[]\n",
            "null\n",
        ] {
            fs::write(&path, bytes).unwrap();
            configure_status_line(&root).unwrap();
            assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
