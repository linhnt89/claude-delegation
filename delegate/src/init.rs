//! `delegate init`: add the project files and the user's worker config, never overwriting.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Result, bail};

use crate::config::{WORKERS_EXAMPLE, user_config_path};
use crate::task::git;

const PROJECT_FILES: &[(&str, &str)] = &[
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
const EXECUTABLE: &[&str] = &["evals/run"];

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
    for (rel, content) in PROJECT_FILES {
        write_new(&root.join(rel), content, rel)?;
    }
    let cfg = user_config_path();
    write_new(
        &cfg,
        WORKERS_EXAMPLE,
        &format!(
            "{} (your worker models, shared by all projects)",
            cfg.display()
        ),
    )?;
    println!(
        "\nNext: fill in AGENTS.md (commands first) and .delegate.toml rules, run /evaluate define, then commit."
    );
    Ok(())
}
