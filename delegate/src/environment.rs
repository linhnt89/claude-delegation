//! Child environments for commands that judge work, and isolated unit-test configuration.

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Read configuration without letting the unit-test runner's DELEGATE_* settings affect tests.
/// Environment-dependent tests opt in only in a freshly sanitized child process.
pub fn var_os(name: &str) -> Option<OsString> {
    #[cfg(test)]
    if name.starts_with("DELEGATE_")
        && std::env::var_os("_DELEGATE_TEST_ENV").as_deref() != Some(OsStr::new("1"))
    {
        return None;
    }
    std::env::var_os(name)
}

pub fn var(name: &str) -> Result<String, std::env::VarError> {
    var_os(name)
        .ok_or(std::env::VarError::NotPresent)?
        .into_string()
        .map_err(std::env::VarError::NotUnicode)
}

/// No caller configuration or credentials reach a judge unless explicitly allowlisted.
/// Autopilot authorization is never a permitted extra, even if a project lists it.
pub fn judging<'a>(command: &'a mut Command, extra: &[String]) -> &'a mut Command {
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        #[cfg(test)]
        if name.to_str().is_some_and(|name| var_os(name).is_none()) {
            continue;
        }
        if allowed(&name, extra) {
            command.env(name, value);
        }
    }
    command
}

fn allowed(name: &OsStr, extra: &[String]) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    if matches!(name, "DELEGATE_AUTOPILOT" | "DELEGATE_AUTOPILOT_WAVE") {
        return false;
    }
    matches!(
        name,
        "PATH" | "HOME" | "USER" | "LANG" | "TERM" | "TMPDIR" | "CARGO_HOME" | "RUSTUP_HOME"
    ) || name.starts_with("LC_")
        || name.starts_with("MISE_")
        || name == "DELEGATE_HOME"
        || extra.iter().any(|extra| extra == name)
}

/// A unit-test subprocess starts with no ambient delegate configuration. Tests may then
/// set just the variables they need with `Command::env`, without mutating parallel tests.
#[cfg(test)]
pub fn test_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("DELEGATE_") {
            command.env_remove(name);
        }
    }
    command.env("_DELEGATE_TEST_ENV", "1");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_configuration_child() {
        if std::env::var_os("_DELEGATE_TEST_AMBIENT").is_none() {
            return;
        }
        // The parent deliberately bypasses test_command to simulate a polluted test runner.
        for name in [
            "DELEGATE_AUTOPILOT",
            "DELEGATE_HOME",
            "DELEGATE_WORKERS",
            "DELEGATE_SANDBOX",
        ] {
            assert!(std::env::var_os(name).is_some());
            assert_eq!(var_os(name), None);
            assert_eq!(var(name), Err(std::env::VarError::NotPresent));
        }
        let output = test_command("env").output().unwrap();
        assert!(output.status.success());
        assert!(
            !String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with("DELEGATE_"))
        );
    }

    #[test]
    fn ambient_configuration_is_not_test_configuration() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "environment::tests::ambient_configuration_child",
                "--nocapture",
            ])
            .env_remove("_DELEGATE_TEST_ENV")
            .env("_DELEGATE_TEST_AMBIENT", "1")
            .env("DELEGATE_AUTOPILOT", "stray")
            .env("DELEGATE_HOME", "/not/test/state")
            .env("DELEGATE_WORKERS", "/not/test/config")
            .env("DELEGATE_SANDBOX", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn judge_child() {
        if std::env::var_os("_DELEGATE_TEST_JUDGE").is_none() {
            return;
        }
        let extra = vec![
            "EXTRA_ALLOWED".into(),
            "DELEGATE_AUTOPILOT".into(),
            "DELEGATE_AUTOPILOT_WAVE".into(),
            "UNSET_EXTRA".into(),
        ];
        let mut command = Command::new("env");
        command.env("PREVIOUS_COMMAND_SETTING", "must disappear");
        let output = judging(&mut command, &extra).output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        for line in text.lines() {
            let (name, value) = line.split_once('=').unwrap();
            assert!(allowed(OsStr::new(name), &extra), "{line}");
            assert_eq!(std::env::var(name).unwrap(), value);
        }
        for name in [
            "PATH",
            "HOME",
            "LANG",
            "MISE_TEST_SETTING",
            "LC_TEST",
            "EXTRA_ALLOWED",
            "DELEGATE_HOME",
        ] {
            assert!(
                text.lines()
                    .any(|line| line == format!("{name}={}", std::env::var(name).unwrap())),
                "{text}"
            );
        }
        for name in [
            "DELEGATE_AUTOPILOT",
            "DELEGATE_AUTOPILOT_WAVE",
            "UNLISTED_SECRET",
            "UNSET_EXTRA",
            "TMPDIR",
            "_DELEGATE_BACKGROUND_RUN",
            "_DELEGATE_AUTOPILOT_GROUP",
            "PREVIOUS_COMMAND_SETTING",
        ] {
            assert!(
                !text
                    .lines()
                    .any(|line| line.starts_with(&format!("{name}="))),
                "{text}"
            );
        }
    }

    #[test]
    fn judge_environment_copies_only_set_allowed_variables() {
        let output = test_command(std::env::current_exe().unwrap())
            .args(["--exact", "environment::tests::judge_child", "--nocapture"])
            .env_clear()
            .env("_DELEGATE_TEST_ENV", "1")
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("_DELEGATE_TEST_JUDGE", "1")
            .env("DELEGATE_AUTOPILOT", "stray")
            .env("DELEGATE_AUTOPILOT_WAVE", "1")
            .env("_DELEGATE_BACKGROUND_RUN", "t001")
            .env("_DELEGATE_AUTOPILOT_GROUP", "stray")
            .env("DELEGATE_HOME", "/test/delegate")
            .env("HOME", "/test/home")
            .env("LANG", "C")
            .env("LC_TEST", "locale")
            .env("MISE_TEST_SETTING", "toolchain")
            .env("EXTRA_ALLOWED", "extra")
            .env("UNLISTED_SECRET", "secret")
            .env_remove("UNSET_EXTRA")
            .env_remove("TMPDIR")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn allowlist_and_unconditional_token_exclusion() {
        let extra = vec![
            "EXTRA_ALLOWED".into(),
            "DELEGATE_AUTOPILOT".into(),
            "DELEGATE_AUTOPILOT_WAVE".into(),
        ];
        for name in [
            "PATH",
            "HOME",
            "USER",
            "LANG",
            "LC_ALL",
            "LC_TIME",
            "TERM",
            "TMPDIR",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "MISE_DATA_DIR",
            "MISE_STATE_DIR",
            "DELEGATE_HOME",
            "EXTRA_ALLOWED",
        ] {
            assert!(allowed(OsStr::new(name), &extra), "{name}");
        }
        for name in [
            "DELEGATE_AUTOPILOT",
            "DELEGATE_AUTOPILOT_WAVE",
            "UNLISTED_SECRET",
            "DELEGATE_WORKERS",
            "DELEGATE_SANDBOX",
            "_DELEGATE_BACKGROUND_RUN",
            "_DELEGATE_AUTOPILOT_GROUP",
            "CARGO_TARGET_DIR",
            "LC",
            "MISE",
        ] {
            assert!(!allowed(OsStr::new(name), &extra), "{name}");
        }
    }
}
