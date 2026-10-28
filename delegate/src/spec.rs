//! Reading the parts of a task spec the runner acts on.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use globset::Glob;

use crate::config::TaskSize;

/// Body of a `## name` section, up to the next `## ` heading.
pub fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let heading = format!("## {name}");
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end();
        match start {
            None if trimmed == heading => start = Some(offset + line.len()),
            Some(s) if trimmed.starts_with("## ") => return &text[s..offset],
            _ => {}
        }
        offset += line.len();
    }
    start.map_or("", |s| &text[s..])
}

/// Contents of every `backtick span` in a line.
fn backtick_spans(line: &str) -> Vec<String> {
    line.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// Commands from checklist lines like ``- [ ] `cargo test` `` in the Acceptance section.
pub fn acceptance_commands(text: &str) -> Vec<String> {
    section(text, "Acceptance")
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("- [")?;
            let after_box = rest.get(2..)?.trim_start();
            after_box
                .starts_with('`')
                .then(|| backtick_spans(after_box).into_iter().next())
                .flatten()
        })
        .filter(|cmd| !cmd.trim().is_empty())
        .collect()
}

/// Paths in the Scope section's `- In:` item, including wrapped lines.
pub fn scope_paths(text: &str) -> Vec<String> {
    let mut in_scope = false;
    let mut paths = Vec::new();
    for line in section(text, "Scope").lines().map(str::trim_start) {
        if in_scope {
            if line.starts_with("- ") {
                break;
            }
            paths.extend(backtick_spans(line));
        } else if let Some(rest) = line.strip_prefix("- In:") {
            in_scope = true;
            paths.extend(backtick_spans(rest));
        }
    }
    paths
}

/// Both sides' overlapping scope entries, sorted and deduplicated for warnings.
pub fn overlapping_paths(left: &[String], right: &[String]) -> Vec<String> {
    let mut shared = BTreeSet::new();
    for a in left.iter().filter(|path| !path.is_empty()) {
        for b in right.iter().filter(|path| !path.is_empty()) {
            if paths_overlap(a, b) {
                shared.insert(a.clone());
                shared.insert(b.clone());
            }
        }
    }
    shared.into_iter().collect()
}

fn paths_overlap(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches('/');
    let b = b.trim_end_matches('/');
    if a == b
        || a.strip_prefix(b).is_some_and(|rest| rest.starts_with('/'))
        || b.strip_prefix(a).is_some_and(|rest| rest.starts_with('/'))
    {
        return true;
    }
    match (a.find('*'), b.find('*')) {
        (Some(i), Some(j)) => a[..i].starts_with(&b[..j]) || b[..j].starts_with(&a[..i]),
        (Some(_), None) => glob_matches(a, b),
        (None, Some(_)) => glob_matches(b, a),
        (None, None) => false,
    }
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    // Use the same matcher as .delegate.toml rules. A malformed scope glob
    // should at worst warn, never suppress a warning or prevent a run.
    Glob::new(pattern).map_or(true, |glob| glob.compile_matcher().is_match(path))
}

/// Tasks that wrote the checks, listed on `checks_by:` lines anywhere in the spec.
pub fn checks_by(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("checks_by:") {
            for id in value.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                if !ids.iter().any(|seen| seen == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids
}

/// Task size from a `size:` line anywhere in the spec; medium when absent.
pub fn size(text: &str) -> Result<TaskSize> {
    let mut value = None;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("size:") {
            value = Some(rest.trim());
        }
    }
    match value.unwrap_or("medium") {
        "small" => Ok(TaskSize::Small),
        "medium" => Ok(TaskSize::Medium),
        "long" => Ok(TaskSize::Long),
        other => bail!("size '{other}' is not valid; use size: small, size: medium or size: long"),
    }
}

/// Spec title without the leading `# Task `: `t005: Add divide`.
pub fn title(text: &str, id: &str) -> String {
    let first = text.lines().next().unwrap_or_default();
    let t = first.trim_start_matches('#').trim();
    let t = t.strip_prefix("Task ").unwrap_or(t);
    if t.is_empty() {
        id.to_string()
    } else {
        t.to_string()
    }
}

/// User-facing title without the task id, or the id for an unfilled spec.
pub fn display_title(text: &str, id: &str) -> String {
    let heading = title(text, id);
    let name = heading
        .strip_prefix(&format!("{id}:"))
        .unwrap_or(&heading)
        .trim();
    if name.is_empty() || name == "<name>" {
        id.to_string()
    } else {
        name.to_string()
    }
}

/// Goal section text, without template placeholder lines.
pub fn goal(text: &str) -> String {
    section(text, "Goal")
        .trim()
        .lines()
        .filter(|l| !l.trim_start().starts_with('<'))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "# Task t005: Add divide\n\n## Goal\nAdd divide.\nRaise on zero.\n\n\
        ## Scope\n- In: `calc.py`, `tests/**`\n- Out: everything else\n\n\
        ## Acceptance\n- [ ] `python3 -m unittest -q`\n- [x] `cargo test` passes\n\
        - [ ] observable behavior\n  - [ ] `nested ok`\n\n## When done\n- `not a check`\n";

    #[test]
    fn reads_check_writers_anywhere_and_deduplicates() {
        assert_eq!(checks_by(SPEC), Vec::<String>::new());
        assert_eq!(checks_by("checks_by: \n"), Vec::<String>::new());
        assert_eq!(
            checks_by(
                "# Task t005\nchecks_by: t001, t002\n## Goal\nGoal\n  checks_by: t002, t003, \n"
            ),
            ["t001", "t002", "t003"]
        );
        assert_eq!(
            checks_by("<!-- checks_by: t001 -->\nprose checks_by: t002"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn reads_size_from_any_line_and_defaults_to_medium() {
        use crate::config::TaskSize;
        assert_eq!(size("# Task t1\n").unwrap(), TaskSize::Medium);
        assert_eq!(size("size: small\n").unwrap(), TaskSize::Small);
        assert_eq!(
            size("## Goal\nlong text\n\nsize: long\n").unwrap(),
            TaskSize::Long
        );
        assert_eq!(
            size("size: small\n## Answers\nsize: medium\n").unwrap(),
            TaskSize::Medium
        );
        let error = size("size: huge\n").unwrap_err().to_string();
        assert!(error.contains("size"), "{error}");
        for value in ["small", "medium", "long"] {
            assert!(error.contains(value), "{error}");
        }
    }

    #[test]
    fn reads_sections() {
        assert_eq!(section(SPEC, "Goal"), "Add divide.\nRaise on zero.\n\n");
        assert_eq!(section(SPEC, "Missing"), "");
        assert!(section(SPEC, "When done").contains("not a check"));
    }

    #[test]
    fn extracts_acceptance_commands() {
        assert_eq!(
            acceptance_commands(SPEC),
            ["python3 -m unittest -q", "cargo test", "nested ok"]
        );
        assert_eq!(
            acceptance_commands("## Acceptance\n- [ ] ``\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn display_titles_omit_the_id_and_fall_back_for_empty_or_template_specs() {
        assert_eq!(display_title(SPEC, "t005"), "Add divide");
        for text in [
            "",
            "\n## Goal\nSomething\n",
            "   \n",
            "# Task t005: <name>\n",
            "# Task t005:   <name>  \n",
            "# Task t005: \n",
            "# Task t005\n",
        ] {
            assert_eq!(display_title(text, "t005"), "t005", "{text:?}");
        }
        assert_eq!(display_title("# Task t005: Foo: bar\n", "t005"), "Foo: bar");
        // The commit-subject helper deliberately retains the id and colon.
        assert_eq!(title(SPEC, "t005"), "t005: Add divide");
    }

    #[test]
    fn scope_paths_reads_continuations_until_the_next_item_or_section() {
        assert_eq!(
            scope_paths(
                "## Scope\n- In: `src/cart.rs`,\n  `src/price.rs` and\n  `src/tax.rs`\n- Out: `private/**`\n  `also-out`\n"
            ),
            ["src/cart.rs", "src/price.rs", "src/tax.rs"]
        );
        assert_eq!(
            scope_paths("## Scope\n- In:\n  `only-on-continuation.rs`\n"),
            ["only-on-continuation.rs"]
        );
        assert_eq!(
            scope_paths("## Scope\n- In: `in.rs`\n- Other: `out.rs`\n"),
            ["in.rs"]
        );
        assert_eq!(
            scope_paths("## Scope\n- In:\n  `in.rs`\n## Acceptance\n`out.rs`\n"),
            ["in.rs"]
        );
        assert_eq!(
            scope_paths("## Scope\n- Out: `out.rs`\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn scope_overlap_is_symmetric_and_conservative() {
        for (a, b, expected) in [
            ("src/price.rs", "src/price.rs", true),
            ("src/price.rs", "src/tax.rs", false),
            ("src/*.rs", "src/price.rs", true),
            ("src/**", "src/cart/price.rs", true),
            ("src/*.rs", "src/readme.md", false),
            ("src/**", "docs/readme.md", false),
            ("src/*.rs", "src/cart.*", true),
            ("src/**", "src/cart/*.rs", true),
            ("src/a*.rs", "src/ab*.md", true),
            ("src/a*.rs", "src/b*.rs", false),
            ("src/**", "docs/**", false),
            ("**/*.rs", "docs/**", true),
            ("src", "src/cart/price.rs", true),
            ("src/", "src/cart.rs", true),
            ("src/", "src", true),
            ("src/", "src/**", true),
            ("src", "src-other/cart.rs", false),
            ("src/cart", "src/cart.rs", false),
            ("src/[*.rs", "src/price.rs", true),
        ] {
            assert_eq!(paths_overlap(a, b), expected, "{a} vs {b}");
            assert_eq!(paths_overlap(b, a), expected, "{b} vs {a}");
        }
    }

    #[test]
    fn scope_overlap_lists_both_entries_once_and_ignores_empty_scopes() {
        let left = ["src/**", "src/price.rs", "src/price.rs"].map(str::to_owned);
        let right = ["src/price.rs", "src/cart.rs", "docs/readme.md"].map(str::to_owned);
        assert_eq!(
            overlapping_paths(&left, &right),
            ["src/**", "src/cart.rs", "src/price.rs"]
        );
        assert_eq!(overlapping_paths(&left, &[]), Vec::<String>::new());
        assert_eq!(overlapping_paths(&[], &right), Vec::<String>::new());
        assert_eq!(overlapping_paths(&[], &[]), Vec::<String>::new());
        assert_eq!(
            overlapping_paths(&[String::new()], &right),
            Vec::<String>::new()
        );
    }

    #[test]
    fn extracts_scope_and_title_and_goal() {
        assert_eq!(scope_paths(SPEC), ["calc.py", "tests/**"]);
        assert_eq!(title(SPEC, "t005"), "t005: Add divide");
        assert_eq!(title("", "t9"), "t9");
        assert_eq!(goal(SPEC), "Add divide.\nRaise on zero.");
        assert_eq!(goal("## Goal\n<placeholder>\n"), "");
    }
}
