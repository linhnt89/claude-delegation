//! Reading the parts of a task spec the runner acts on.

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

/// Paths listed in backticks on the Scope section's `- In:` line.
pub fn scope_paths(text: &str) -> Vec<String> {
    section(text, "Scope")
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("- In:"))
        .map(backtick_spans)
        .unwrap_or_default()
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
    fn extracts_scope_and_title_and_goal() {
        assert_eq!(scope_paths(SPEC), ["calc.py", "tests/**"]);
        assert_eq!(title(SPEC, "t005"), "t005: Add divide");
        assert_eq!(title("", "t9"), "t9");
        assert_eq!(goal(SPEC), "Add divide.\nRaise on zero.");
        assert_eq!(goal("## Goal\n<placeholder>\n"), "");
    }
}
