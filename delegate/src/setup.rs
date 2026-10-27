//! Machine setup and a read-only health check, independent of any project.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::{Harness, WORKERS_EXAMPLE, Worker, user_config_path};
use crate::{harness, vault};

const HARNESSES: [Harness; 5] = [
    Harness::Pi,
    Harness::Codex,
    Harness::Claude,
    Harness::Opencode,
    Harness::Gemini,
];
const ROLES: [&str; 4] = ["implement", "research", "review", "plan_review"];

fn workers(installed: &[Harness]) -> Result<String> {
    let mut config: toml::Value = toml::from_str(WORKERS_EXAMPLE)?;
    for role in ROLES {
        if let Some(table) = config.get_mut(role).and_then(toml::Value::as_table_mut) {
            for candidates in table.iter_mut().filter_map(|(_, v)| v.as_array_mut()) {
                candidates.retain(|v| {
                    v.as_str()
                        .and_then(|s| s.parse::<Worker>().ok())
                        .is_some_and(|w| installed.contains(&w.harness))
                });
                for harness in installed {
                    if !candidates.iter().any(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<Worker>().ok())
                            .is_some_and(|w| w.harness == *harness)
                    }) {
                        // Harnesses not in the example use their own configured default model.
                        candidates.push(toml::Value::String(format!("{}:", harness.name())));
                    }
                }
            }
        }
    }
    Ok(format!(
        "# Worker models on this machine; edit models and accounts to suit.\n{}",
        toml::to_string_pretty(&config)?
    ))
}

fn configured_harnesses(text: &str) -> Result<Vec<Harness>> {
    // Inspect even a partial user config; setup must not require all defaults to be present.
    let config: toml::Value = toml::from_str(text)?;
    let mut found = Vec::new();
    for role in ROLES {
        if let Some(table) = config.get(role).and_then(toml::Value::as_table) {
            for list in table.values().filter_map(toml::Value::as_array) {
                for candidate in list.iter().filter_map(toml::Value::as_str) {
                    let worker: Worker = candidate.parse()?;
                    if !found.contains(&worker.harness) {
                        found.push(worker.harness);
                    }
                }
            }
        }
    }
    Ok(found)
}

fn check_bwrap() {
    if !harness::on_path("bwrap") {
        println!("bwrap missing; install bubblewrap (or use DELEGATE_SANDBOX=0 without isolation)");
        return;
    }
    match Command::new("bwrap")
        .args([
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "/bin/true",
        ])
        .output()
    {
        Ok(out) if out.status.success() => println!("bwrap working"),
        Ok(out) => println!(
            "bwrap cannot create a sandbox: {}; enable unprivileged user namespaces or fix bubblewrap permissions (DELEGATE_SANDBOX=0 disables isolation)",
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .replace(['\n', '\r'], " ")
        ),
        Err(e) => println!(
            "bwrap cannot run: {e}; install bubblewrap and enable unprivileged user namespaces"
        ),
    }
}

/// `with_vault`: also connect or check the vault (off inside the sandbox, which never names it).
pub(crate) fn run(url: Option<&str>, with_vault: bool) -> Result<()> {
    let installed: Vec<Harness> = HARNESSES
        .into_iter()
        .filter(|h| harness::on_path(h.name()))
        .collect();
    for harness in HARNESSES {
        if installed.contains(&harness) {
            println!("{} found on PATH", harness.name());
        } else {
            println!(
                "{} missing; install and sign in to {} to add it",
                harness.name(),
                harness.name()
            );
        }
    }
    let path = user_config_path();
    if path.exists() {
        println!("kept {}", path.display());
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(workers(&installed)?.as_bytes())?;
                println!("added {} (edit your worker models here)", path.display());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(e).context("creating workers.toml; check config directory permissions");
            }
        }
    }
    match configured_harnesses(&fs::read_to_string(&path)?) {
        Ok(configured) => {
            for harness in configured {
                if !installed.contains(&harness) {
                    println!(
                        "configured {} not found; install it or edit {}",
                        harness.name(),
                        path.display()
                    );
                }
            }
        }
        Err(e) => println!(
            "cannot read worker choices: {e}; repair {} (left unchanged)",
            path.display()
        ),
    }
    check_bwrap();
    if !with_vault {
        return Ok(());
    }
    if let Some(url) = url {
        if let Err(e) = vault::connect(url) {
            println!(
                "cannot connect vault: {}",
                format!("{e:#}").replace(['\n', '\r'], " ")
            );
        }
    } else {
        vault::health()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_installed_harnesses_and_all_roles_are_configured() {
        for installed in [
            vec![],
            vec![Harness::Pi, Harness::Codex],
            vec![Harness::Opencode],
            HARNESSES.to_vec(),
        ] {
            let text = workers(&installed).unwrap();
            let config: toml::Value = toml::from_str(&text).unwrap();
            for role in ROLES {
                for list in config[role].as_table().unwrap().values() {
                    let have: Vec<_> = list
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().parse::<Worker>().unwrap().harness)
                        .collect();
                    assert!(have.iter().all(|h| installed.contains(h)));
                    assert!(installed.iter().all(|h| have.contains(h)));
                }
            }
        }
    }

    #[test]
    fn partial_config_can_report_missing_harnesses() {
        assert_eq!(
            configured_harnesses("[implement]\nstandard = [\"gemini:gemini-3-pro@high\"]").unwrap(),
            vec![Harness::Gemini]
        );
    }
}
