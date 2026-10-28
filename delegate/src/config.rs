//! Worker models per role and tier (user-level) and project rules (`.delegate.toml`).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

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
    /// Model family, inferred from the model name; keeps reviewers and check writers independent.
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

/// How long a task is expected to take, for the quota runway it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskSize {
    Small,
    Medium,
    Long,
}

impl TaskSize {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskSize::Small => "small",
            TaskSize::Medium => "medium",
            TaskSize::Long => "long",
        }
    }
}

impl fmt::Display for TaskSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Local wall-clock time, `HH:MM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub hour: u8,
    pub minute: u8,
}

impl Default for LocalTime {
    fn default() -> Self {
        LocalTime { hour: 0, minute: 8 }
    }
}

impl FromStr for LocalTime {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (hour, minute) = text
            .trim()
            .split_once(':')
            .ok_or_else(|| anyhow!("use a time such as \"00:08\""))?;
        let hour: u8 = hour.parse().map_err(|_| anyhow!("the hour must be 0-23"))?;
        let minute: u8 = minute
            .parse()
            .map_err(|_| anyhow!("the minute must be 0-59"))?;
        if hour > 23 || minute > 59 {
            bail!("use a time such as \"00:08\" (hour 0-23, minute 0-59)");
        }
        Ok(LocalTime { hour, minute })
    }
}

/// Runway a task needs, by its spec's `size:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunwayConfig {
    pub small: Duration,
    pub medium: Duration,
    pub long: Duration,
}

impl Default for RunwayConfig {
    fn default() -> Self {
        RunwayConfig {
            small: Duration::from_mins(30),
            medium: Duration::from_hours(1),
            long: Duration::from_hours(3),
        }
    }
}

impl RunwayConfig {
    pub fn for_size(&self, size: TaskSize) -> Duration {
        match size {
            TaskSize::Small => self.small,
            TaskSize::Medium => self.medium,
            TaskSize::Long => self.long,
        }
    }
}

impl<'de> Deserialize<'de> for RunwayConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default = "default_runway_small", deserialize_with = "ds_runway_small")]
            small: Duration,
            #[serde(
                default = "default_runway_medium",
                deserialize_with = "ds_runway_medium"
            )]
            medium: Duration,
            #[serde(default = "default_runway_long", deserialize_with = "ds_runway_long")]
            long: Duration,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(RunwayConfig {
            small: raw.small,
            medium: raw.medium,
            long: raw.long,
        })
    }
}

fn default_runway_small() -> Duration {
    RunwayConfig::default().small
}

fn default_runway_medium() -> Duration {
    RunwayConfig::default().medium
}

fn default_runway_long() -> Duration {
    RunwayConfig::default().long
}

fn ds_runway_small<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    deserialize_duration(d, "quota.runway.small")
}

fn ds_runway_medium<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    deserialize_duration(d, "quota.runway.medium")
}

fn ds_runway_long<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    deserialize_duration(d, "quota.runway.long")
}

fn deserialize_duration<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
    key: &str,
) -> Result<Duration, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_duration(&text).map_err(|why| serde::de::Error::custom(format!("{key}: {why}")))
}

fn parse_duration(text: &str) -> Result<Duration, String> {
    let value = text.trim();
    let split = value.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
        format!("use a duration such as \"30m\", \"1h\" or \"3h\", not {value:?}")
    })?;
    let (number, unit) = value.split_at(split);
    let amount: u64 = number
        .parse()
        .map_err(|_| format!("use a duration such as \"30m\", \"1h\" or \"3h\", not {value:?}"))?;
    let seconds = match unit {
        "s" => amount,
        "m" => amount.saturating_mul(60),
        "h" => amount.saturating_mul(3_600),
        "d" => amount.saturating_mul(86_400),
        _ => {
            return Err(format!(
                "use a duration such as \"30m\", \"1h\" or \"3h\", not {value:?}"
            ));
        }
    };
    if seconds == 0 {
        return Err(format!("use a duration greater than zero, not {value:?}"));
    }
    Ok(Duration::from_secs(seconds))
}

/// Hyper's prepaid daily credits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HyperConfig {
    pub daily_cap: u32,
    pub resets_at: LocalTime,
}

impl Default for HyperConfig {
    fn default() -> Self {
        HyperConfig {
            daily_cap: 250,
            resets_at: LocalTime::default(),
        }
    }
}

impl<'de> Deserialize<'de> for HyperConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default = "default_daily_cap", deserialize_with = "ds_daily_cap")]
            daily_cap: u32,
            #[serde(default, deserialize_with = "ds_resets_at")]
            resets_at: LocalTime,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(HyperConfig {
            daily_cap: raw.daily_cap,
            resets_at: raw.resets_at,
        })
    }
}

fn default_daily_cap() -> u32 {
    HyperConfig::default().daily_cap
}

fn ds_daily_cap<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    if value == 0 {
        return Err(serde::de::Error::custom(
            "quota.hyper.daily_cap: use a whole number of credits, 1 or more",
        ));
    }
    Ok(value)
}

fn ds_resets_at<'de, D: serde::Deserializer<'de>>(d: D) -> Result<LocalTime, D::Error> {
    let text = String::deserialize(d)?;
    text.parse::<LocalTime>()
        .map_err(|why| serde::de::Error::custom(format!("quota.hyper.resets_at: {why}")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuotaConfig {
    pub check: bool,
    pub runway: RunwayConfig,
    pub hyper: HyperConfig,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        QuotaConfig {
            check: true,
            runway: RunwayConfig::default(),
            hyper: HyperConfig::default(),
        }
    }
}

impl<'de> Deserialize<'de> for QuotaConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default = "default_quota_check")]
            check: bool,
            #[serde(default)]
            runway: RunwayConfig,
            #[serde(default)]
            hyper: HyperConfig,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(QuotaConfig {
            check: raw.check,
            runway: raw.runway,
            hyper: raw.hyper,
        })
    }
}

fn default_quota_check() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub implement: Tier,
    pub research: Tier,
    pub review: Tier,
    pub plan_review: Tier,
    #[serde(
        default = "default_step_up_after",
        deserialize_with = "deserialize_step_up_after"
    )]
    pub step_up_after: u64,
}

fn default_step_up_after() -> u64 {
    1
}

fn deserialize_step_up_after<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let invalid = || serde::de::Error::custom("step_up_after must be a whole number, 1 or more");
    let value = u64::deserialize(deserializer).map_err(|_| invalid())?;
    if value == 0 {
        return Err(invalid());
    }
    Ok(value)
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
    /// Candidate prefix -> account name, overriding the built-in account table.
    #[serde(default)]
    pub accounts: BTreeMap<String, String>,
    #[serde(default)]
    pub quota: QuotaConfig,
}

impl WorkersConfig {
    pub fn load() -> Result<Self> {
        let path = workers_config_path();
        if path.is_file() {
            let text = std::fs::read_to_string(&path)?;
            let config: Self =
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            config.validate_accounts(&path)?;
            return Ok(config);
        }
        eprintln!(
            "note: no {}, using the built-in defaults (run: delegate setup)",
            user_config_path().display()
        );
        Ok(toml::from_str(WORKERS_EXAMPLE)?)
    }

    /// `[accounts]` may only name a real account reader or `none`.
    pub fn validate_accounts(&self, path: &Path) -> Result<()> {
        for (key, value) in &self.accounts {
            let name = value.trim().to_lowercase();
            if name == "none" || matches!(name.as_str(), "claude" | "codex" | "copilot" | "hyper") {
                continue;
            }
            bail!(
                "[accounts] in {}: {value:?} for {key:?} is not an account; use claude, codex, copilot, hyper or none",
                path.display()
            );
        }
        Ok(())
    }

    pub fn default_tier(&self, role: Role) -> Tier {
        match role {
            Role::Implement => self.defaults.implement,
            Role::Research => self.defaults.research,
            Role::Review => self.defaults.review,
            Role::PlanReview => self.defaults.plan_review,
        }
    }

    /// Every configured candidate, in role and tier order, skipping unparsable entries.
    pub fn all_workers(&self) -> Vec<Worker> {
        let tables = [
            &self.implement,
            &self.research,
            &self.review,
            &self.plan_review,
        ];
        tables
            .into_iter()
            .flat_map(|table| table.values())
            .flatten()
            .filter_map(|candidate| candidate.parse().ok())
            .collect()
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

/// Configuration being read, or the path to create when using built-in defaults.
pub fn workers_config_path() -> PathBuf {
    std::env::var_os("DELEGATE_WORKERS")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(user_config_path)
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
    /// Changed paths that match any of this rule's path globs, in the order given.
    pub fn matching_paths(&self, paths: &[String]) -> Result<Vec<String>> {
        let matchers = self
            .paths
            .iter()
            .map(|pattern| {
                Glob::new(pattern)
                    .with_context(|| format!("bad glob '{pattern}' in .delegate.toml"))
                    .map(|glob| glob.compile_matcher())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(paths
            .iter()
            .filter(|path| matchers.iter().any(|matcher| matcher.is_match(path)))
            .cloned()
            .collect())
    }

    pub fn matches_any(&self, paths: &[String]) -> Result<bool> {
        Ok(!self.matching_paths(paths)?.is_empty())
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
    fn accounts_must_name_a_known_reader() {
        let valid = WORKERS_EXAMPLE.replace(
            "# \"codex:\" = \"codex\"",
            "\"codex:\" = \"none\"\n\"claude:\" = \"claude\"",
        );
        let config: WorkersConfig = toml::from_str(&valid).unwrap();
        config
            .validate_accounts(Path::new("/tmp/workers.toml"))
            .unwrap();

        let invalid = WORKERS_EXAMPLE.replace("# \"codex:\" = \"codex\"", "\"codex:\" = \"codxe\"");
        let config: WorkersConfig = toml::from_str(&invalid).unwrap();
        let error = config
            .validate_accounts(Path::new("/tmp/workers.toml"))
            .unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains("[accounts]"), "{text}");
        assert!(text.contains("/tmp/workers.toml"), "{text}");
        assert!(text.contains("codxe"), "{text}");
        assert!(
            text.contains("claude, codex, copilot, hyper or none"),
            "{text}"
        );
    }

    #[test]
    fn step_up_after_defaults_and_validates() {
        let without = WORKERS_EXAMPLE.replace("step_up_after = 1", "");
        let cfg: WorkersConfig = toml::from_str(&without).unwrap();
        assert_eq!(cfg.defaults.step_up_after, 1);
        for n in [1, 2, 3] {
            let text = without.replace("[defaults]", &format!("[defaults]\nstep_up_after = {n}"));
            let cfg: WorkersConfig = toml::from_str(&text).unwrap();
            assert_eq!(cfg.defaults.step_up_after, n);
        }
        for bad in ["0", "-1", "1.5", "\"two\"", "true", "[]", "{}"] {
            let text = without.replace("[defaults]", &format!("[defaults]\nstep_up_after = {bad}"));
            let error = toml::from_str::<WorkersConfig>(&text)
                .unwrap_err()
                .to_string();
            assert!(error.contains("step_up_after"), "{error}");
            assert!(error.contains("a whole number, 1 or more"), "{error}");
        }
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
    fn rule_lists_only_matching_changed_paths() {
        let rules: ProjectRules = toml::from_str(
            "[[rules]]\npaths = [\"src/auth/**\", \"src/secret.rs\"]\nmin_tier = \"deep\"\n",
        )
        .unwrap();
        let paths = vec![
            "README.md".to_string(),
            "src/auth/login.rs".to_string(),
            "src/auth/db/pool.rs".to_string(),
            "src/secret.rs".to_string(),
        ];
        assert_eq!(
            rules.rules[0].matching_paths(&paths).unwrap(),
            vec!["src/auth/login.rs", "src/auth/db/pool.rs", "src/secret.rs"]
        );
        assert_eq!(
            rules.rules[0]
                .matching_paths(&["src/main.rs".to_string()])
                .unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn tiers_order_and_step_up() {
        assert!(Tier::Deep > Tier::Standard && Tier::Standard > Tier::Fast);
        assert_eq!(Tier::Fast.next(), Tier::Standard);
        assert_eq!(Tier::Deep.next(), Tier::Deep);
    }
}
