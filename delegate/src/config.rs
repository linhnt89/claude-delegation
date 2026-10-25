//! Worker models per role and tier (user-level) and project rules (`.delegate.toml`).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use globset::Glob;
use serde::{Deserialize, Serialize};

pub const WORKERS_EXAMPLE: &str = include_str!("../templates/workers.example.toml");

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Fast,
    Standard,
    Deep,
}

impl Tier {
    pub fn next(self) -> Self {
        match self {
            Tier::Fast => Tier::Standard,
            Tier::Standard | Tier::Deep => Tier::Deep,
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Tier::Fast => "fast",
            Tier::Standard => "standard",
            Tier::Deep => "deep",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    Pi,
    Codex,
    Claude,
    Opencode,
    Gemini,
}

impl Harness {
    pub fn name(self) -> &'static str {
        match self {
            Harness::Pi => "pi",
            Harness::Codex => "codex",
            Harness::Claude => "claude",
            Harness::Opencode => "opencode",
            Harness::Gemini => "gemini",
        }
    }
}

impl FromStr for Harness {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "pi" => Harness::Pi,
            "codex" => Harness::Codex,
            "claude" => Harness::Claude,
            "opencode" => Harness::Opencode,
            "gemini" => Harness::Gemini,
            _ => bail!("unknown harness '{s}' (pi, codex, claude, opencode, gemini)"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

impl FromStr for Effort {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            "xhigh" => Effort::Xhigh,
            "max" => Effort::Max,
            _ => bail!("unknown effort '{s}' (low, medium, high, xhigh, max)"),
        })
    }
}

/// One candidate: `harness:model@effort`, model and effort optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worker {
    pub harness: Harness,
    pub model: String,
    pub effort: Option<Effort>,
}

const FAMILIES: &[(&str, &str)] = &[
    ("deepseek", "deepseek"),
    ("gemini", "gemini"),
    ("gpt", "gpt"),
    ("claude", "claude"),
    ("opus", "claude"),
    ("sonnet", "claude"),
    ("haiku", "claude"),
    ("fable", "claude"),
    ("glm", "glm"),
    ("qwen", "qwen"),
    ("kimi", "kimi"),
    ("grok", "grok"),
];

impl Worker {
    /// Model family, inferred from the model name; used to keep reviewers independent.
    pub fn family(&self) -> String {
        let name = self.model.to_lowercase();
        if let Some((_, fam)) = FAMILIES.iter().find(|(key, _)| name.contains(key)) {
            return (*fam).to_string();
        }
        match self.harness {
            Harness::Claude => "claude".into(),
            Harness::Codex => "gpt".into(),
            _ if name.is_empty() => self.harness.name().into(),
            _ => name,
        }
    }
}

impl FromStr for Worker {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let (harness, rest) = s
            .trim()
            .split_once(':')
            .ok_or_else(|| anyhow!("bad worker '{s}' (expected harness:model@effort)"))?;
        let (model, effort) = match rest.rsplit_once('@') {
            Some((m, e)) => (m, Some(e.parse()?)),
            None => (rest, None),
        };
        Ok(Worker {
            harness: harness.parse()?,
            model: model.to_string(),
            effort,
        })
    }
}

impl fmt::Display for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.harness.name(), self.model)?;
        if let Some(e) = self.effort {
            write!(f, "@{}", e.as_str())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Implement,
    Research,
    Review,
    PlanReview,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Implement => "implement",
            Role::Research => "research",
            Role::Review => "review",
            Role::PlanReview => "plan_review",
        }
    }
}

type TierTable = BTreeMap<Tier, Vec<String>>;

#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub implement: Tier,
    pub research: Tier,
    pub review: Tier,
    pub plan_review: Tier,
}

#[derive(Debug, Deserialize)]
pub struct WorkersConfig {
    pub planner_family: Option<String>,
    pub defaults: Defaults,
    #[serde(default)]
    implement: TierTable,
    #[serde(default)]
    research: TierTable,
    #[serde(default)]
    review: TierTable,
    #[serde(default)]
    plan_review: TierTable,
}

impl WorkersConfig {
    pub fn load() -> Result<Self> {
        let candidates = [
            std::env::var_os("DELEGATE_WORKERS").map(PathBuf::from),
            Some(user_config_path()),
        ];
        for path in candidates.into_iter().flatten() {
            if path.is_file() {
                let text = std::fs::read_to_string(&path)?;
                return toml::from_str(&text)
                    .with_context(|| format!("parsing {}", path.display()));
            }
        }
        eprintln!(
            "note: no {}, using the built-in defaults (run: delegate init)",
            user_config_path().display()
        );
        Ok(toml::from_str(WORKERS_EXAMPLE)?)
    }

    pub fn default_tier(&self, role: Role) -> Tier {
        match role {
            Role::Implement => self.defaults.implement,
            Role::Research => self.defaults.research,
            Role::Review => self.defaults.review,
            Role::PlanReview => self.defaults.plan_review,
        }
    }

    /// Candidates for a role and tier, in order, minus any from `skip_family`.
    pub fn candidates(
        &self,
        role: Role,
        tier: Tier,
        skip_family: Option<&str>,
    ) -> Result<Vec<Worker>> {
        let table = match role {
            Role::Implement => &self.implement,
            Role::Research => &self.research,
            Role::Review => &self.review,
            Role::PlanReview => &self.plan_review,
        };
        let list = table.get(&tier).ok_or_else(|| {
            let have: Vec<String> = table.keys().map(ToString::to_string).collect();
            anyhow!(
                "[{}] has no tier '{tier}' (has: {})",
                role.name(),
                have.join(", ")
            )
        })?;
        if list.is_empty() {
            bail!(
                "[{}].{tier} in workers.toml has no candidates; add at least one \"harness:model@effort\"",
                role.name()
            );
        }
        let workers = list
            .iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<Worker>>>()?;
        let kept: Vec<Worker> = workers
            .into_iter()
            .filter(|w| Some(w.family().as_str()) != skip_family)
            .collect();
        if kept.is_empty() {
            let family = skip_family.unwrap_or_default();
            bail!(
                "every [{}].{tier} candidate is in the {family} family",
                role.name()
            );
        }
        Ok(kept)
    }
}

pub fn user_config_path() -> PathBuf {
    home().join(".config/delegate/workers.toml")
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewRule {
    Always,
}

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub paths: Vec<String>,
    pub min_tier: Option<Tier>,
    pub max_tier: Option<Tier>,
    pub review: Option<ReviewRule>,
}

impl Rule {
    pub fn matches_any(&self, paths: &[String]) -> Result<bool> {
        for pattern in &self.paths {
            let glob = Glob::new(pattern)
                .with_context(|| format!("bad glob '{pattern}' in .delegate.toml"))?;
            let matcher = glob.compile_matcher();
            if paths.iter().any(|p| matcher.is_match(p)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn label(&self) -> String {
        format!("[{}]", self.paths.join(", "))
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectRules {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl ProjectRules {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".delegate.toml");
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn matching(&self, paths: &[String]) -> Result<Vec<&Rule>> {
        let mut out = Vec::new();
        for rule in &self.rules {
            if rule.matches_any(paths)? {
                out.push(rule);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workers() {
        let w: Worker = "pi:openai-codex/gpt-6.1-sol@high".parse().unwrap();
        assert_eq!(w.harness, Harness::Pi);
        assert_eq!(w.model, "openai-codex/gpt-6.1-sol");
        assert_eq!(w.effort, Some(Effort::High));
        assert_eq!(w.to_string(), "pi:openai-codex/gpt-6.1-sol@high");
        let w: Worker = "claude:opus".parse().unwrap();
        assert_eq!(w.effort, None);
        assert!("nonsense".parse::<Worker>().is_err());
        assert!("pi:x@huge".parse::<Worker>().is_err());
        assert!("vim:x".parse::<Worker>().is_err());
    }

    #[test]
    fn infers_families() {
        let fam = |s: &str| s.parse::<Worker>().unwrap().family();
        assert_eq!(fam("pi:openai-codex/gpt-6-luna"), "gpt");
        assert_eq!(fam("pi:github-copilot/gpt-6.1-sol"), "gpt");
        assert_eq!(fam("pi:github-copilot/claude-opus-5.5"), "claude");
        assert_eq!(fam("pi:hyper/deepseek-v4.1-flash-1017"), "deepseek");
        assert_eq!(fam("pi:github-copilot/gemini-4-argon"), "gemini");
        assert_eq!(fam("claude:sonnet"), "claude");
        assert_eq!(fam("claude:"), "claude");
        assert_eq!(fam("codex:"), "gpt");
    }

    #[test]
    fn example_config_is_valid() {
        let cfg: WorkersConfig = toml::from_str(WORKERS_EXAMPLE).unwrap();
        for role in [Role::Implement, Role::Review] {
            for tier in [Tier::Standard, Tier::Deep] {
                cfg.candidates(role, tier, None).unwrap();
            }
        }
        cfg.candidates(Role::Implement, Tier::Fast, None).unwrap();
        cfg.candidates(Role::Research, Tier::Fast, None).unwrap();
        let plan = cfg
            .candidates(
                Role::PlanReview,
                Tier::Standard,
                cfg.planner_family.as_deref(),
            )
            .unwrap();
        assert!(plan.iter().all(|w| w.family() != "claude"));
        let review = cfg
            .candidates(Role::Review, Tier::Standard, Some("gpt"))
            .unwrap();
        assert!(review.iter().all(|w| w.family() != "gpt"));
        assert!(cfg.candidates(Role::Review, Tier::Fast, None).is_err());
    }

    #[test]
    fn empty_candidate_list_names_the_role_and_tier() {
        let cfg: WorkersConfig = toml::from_str(
            r#"
[defaults]
implement = "standard"
research = "fast"
review = "standard"
plan_review = "standard"
[implement]
standard = []
"#,
        )
        .unwrap();
        let err = cfg
            .candidates(Role::Implement, Tier::Standard, None)
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "[implement].standard in workers.toml has no candidates; add at least one \"harness:model@effort\""
        );
    }

    #[test]
    fn family_filter_error_names_the_family() {
        let cfg: WorkersConfig = toml::from_str(
            r#"
[defaults]
implement = "standard"
research = "fast"
review = "standard"
plan_review = "standard"
[review]
standard = ["claude:sonnet@high", "pi:github-copilot/claude-opus-5.5@high"]
"#,
        )
        .unwrap();
        let err = cfg
            .candidates(Role::Review, Tier::Standard, Some("claude"))
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "every [review].standard candidate is in the claude family"
        );
    }

    #[test]
    fn copilot_is_always_last() {
        let cfg: WorkersConfig = toml::from_str(WORKERS_EXAMPLE).unwrap();
        for table in [&cfg.implement, &cfg.research, &cfg.review, &cfg.plan_review] {
            for list in table.values() {
                let first_copilot = list.iter().position(|s| s.contains("github-copilot/"));
                if let Some(i) = first_copilot {
                    assert!(
                        list[i..].iter().all(|s| s.contains("github-copilot/")),
                        "{list:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rules_match_globs() {
        let rules: ProjectRules = toml::from_str(
            "[[rules]]\npaths = [\"src/auth/**\"]\nmin_tier = \"deep\"\nreview = \"always\"\n",
        )
        .unwrap();
        let m = rules.matching(&["src/auth/login.rs".into()]).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].min_tier, Some(Tier::Deep));
        assert_eq!(m[0].review, Some(ReviewRule::Always));
        assert!(rules.matching(&["src/main.rs".into()]).unwrap().is_empty());
    }

    #[test]
    fn tiers_order_and_step_up() {
        assert!(Tier::Deep > Tier::Standard && Tier::Standard > Tier::Fast);
        assert_eq!(Tier::Fast.next(), Tier::Standard);
        assert_eq!(Tier::Deep.next(), Tier::Deep);
    }
}
