//! Prompts given to workers. `{id}` and `{specs}` are filled in by the caller.

pub const ASK: &str = "You are a delegated worker on task {id}.
Read AGENTS.md, then .task/SPEC.md, then the code the spec points to. Do NOT edit any files yet.
Decide whether you can complete the spec without guessing.
Write .task/QUESTIONS.md. First line exactly READY if you can. Otherwise first line QUESTIONS,
then a numbered list of the questions that block you, each with the options you see.
Then stop.";

pub const WORK: &str = "Now do the task.
If .task/CHECKS.txt exists, the previous round failed those acceptance checks; read it first, before changing anything, and make them pass.
Code and tests are the source of truth; if a doc disagrees with the code, follow the code and note it in the report.
If something turns out to be ambiguous, stop and write the question in the report instead of guessing.
Do not commit, push, or change git branches.
When finished, write .task/REPORT.md following .task/REPORT.template.md (40 lines max).";

/// Extra line for the candidate that takes over after a quota-limited worker.
pub const HANDOFF: &str = "The previous worker on this task ran out of quota; its work is committed. Continue from that committed work.";

pub const FULL_INTRO: &str = "You are a delegated worker on task {id}.
Read AGENTS.md, then .task/SPEC.md, and do exactly what the spec asks.
";

pub const REVIEW: &str = "You are an adversarial reviewer for task {id}. Someone else implemented it.
Read .task/SPEC.md (what was asked) and .task/DIFF.patch (what was done), then the code around it.
The implementer's report is withheld on purpose. Review the code against the spec, not the paperwork.
Try to break it: edge cases, inputs the spec implies, wrong assumptions, gaps between spec and code,
missing or weak tests, regressions elsewhere. Don't review style.
Do not fix anything. You may add new tests that fail, as evidence; nothing else.
Write .task/REVIEW.md. First line exactly: FINDINGS: <n> high, <n> medium, <n> low
Then one entry per finding: severity, file:line, what is wrong, how to reproduce
(name the failing test if you added one). High means wrong behavior the spec asks for or data loss.
If you find nothing, say so; don't invent findings.";

pub const PLAN_REVIEW: &str =
    "You are reviewing a plan before any of it is implemented. Someone else wrote it.
Read AGENTS.md, PLAN.md, the decision records in docs/decisions/, and every task spec in {specs}.
Check the plan against the code:
- tasks too big for one reviewable diff, or that should be split differently
- hidden dependencies between tasks, or tasks that can run in parallel but touch the same files
- acceptance checks that don't prove the goal, or that can't be run
- specs that contradict the code, a decision record, or each other
- missing edge cases or missing tasks
Change nothing. Reply with the review as your final message: first line exactly
FINDINGS: <n> high, <n> medium, <n> low
then one entry per finding: severity, task id or file, the problem, a suggested fix.
If the plan is sound, say so; don't invent findings.";

pub const RESEARCH: &str = "You are a delegated researcher on task {id}.
Read AGENTS.md, then .task/SPEC.md, and answer what it asks by reading the code and docs.
Change no files except .task/REPORT.md. The report is the deliverable: findings with file:line
references, what you are unsure of, and what you did not check. Keep it under 80 lines.
Code is the source of truth; if a doc disagrees with the code, say so.";

/// A fresh lead gets only the repository handoff, not an earlier session's context.
pub fn autopilot(wave: u32) -> String {
    format!(
        "You are the autopilot lead for wave {wave}. Follow the ## Mandate in PLAN.md and the delegate skill's Autopilot section. Never wait for the user: park anything needing them under ## Waiting for you. End this wave with the handoff in PLAN.md and exit."
    )
}

/// Prompt for a single-phase run (no question phase, or a harness that can't resume).
pub fn full(id: &str) -> String {
    let work_body = WORK.split_once('\n').map_or(WORK, |(_, rest)| rest);
    FULL_INTRO.replace("{id}", id) + work_body
}

/// High-severity count from a `FINDINGS: <n> high, ...` first line.
pub fn high_findings(text: &str) -> Option<u32> {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("FINDINGS:"))?;
    let rest = line.split_once("FINDINGS:")?.1.trim_start();
    let (n, tail) = rest.split_once(char::is_whitespace)?;
    tail.trim_start()
        .starts_with("high")
        .then(|| n.parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_prompt_has_intro_and_work_rules() {
        let p = full("t1");
        assert!(p.starts_with("You are a delegated worker on task t1."));
        assert!(p.contains("Do not commit"));
        assert!(!p.contains("Now do the task"));
    }

    #[test]
    fn work_prompts_point_to_failed_checks() {
        for prompt in [WORK.to_string(), full("t1")] {
            assert!(prompt.contains("If .task/CHECKS.txt exists"));
            assert!(prompt.contains("the previous round failed those acceptance checks"));
            assert!(prompt.contains("read it first, before changing anything, and make them pass"));
        }
    }

    #[test]
    fn parses_findings_line() {
        assert_eq!(
            high_findings("FINDINGS: 2 high, 0 medium, 1 low\n..."),
            Some(2)
        );
        assert_eq!(
            high_findings("intro\nFINDINGS: 0 high, 1 medium, 0 low"),
            Some(0)
        );
        assert_eq!(high_findings("no findings line"), None);
        assert_eq!(high_findings("FINDINGS: ? high"), None);
    }
}
