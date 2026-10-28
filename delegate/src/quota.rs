//! Native quota readers and runway for the accounts behind worker candidates.
//!
//! Logins are only ever read. Nothing here writes a login file, follows a
//! refresh for Claude or Codex, or prints a token.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config::{Harness, HyperConfig, LocalTime, QuotaConfig, TaskSize, Worker, home};

pub const SCHEMA_VERSION: u32 = 1;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const HYPER_EXPIRY_BUFFER_MS: i64 = 30_000;
const FIVE_HOURS: i64 = 5 * 3_600;
const SEVEN_DAYS: i64 = 7 * 86_400;
const ONE_DAY: i64 = 86_400;
const THIRTY_DAYS: i64 = 30 * 86_400;

const CLAUDE_BASE: &str = "https://api.anthropic.com";
const CODEX_BASE: &str = "https://chatgpt.com";
const COPILOT_BASE: &str = "https://api.github.com";
const HYPER_BASE: &str = "https://hyper.charm.land";

/// One account delegate can read, and the login its candidate prefix uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    pub name: AccountName,
    pub source: LoginSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountName {
    Claude,
    Codex,
    Copilot,
    Hyper,
}

impl AccountName {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountName::Claude => "claude",
            AccountName::Codex => "codex",
            AccountName::Copilot => "copilot",
            AccountName::Hyper => "hyper",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "claude" => Some(AccountName::Claude),
            "codex" | "openai-codex" => Some(AccountName::Codex),
            "copilot" | "github-copilot" => Some(AccountName::Copilot),
            "hyper" => Some(AccountName::Hyper),
            _ => None,
        }
    }
}

impl fmt::Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginSource {
    ClaudeCredentials,
    PiCodex,
    CodexAuth,
    PiCopilot,
    PiHyper,
}

impl LoginSource {
    pub fn path(self) -> &'static str {
        match self {
            LoginSource::ClaudeCredentials => "~/.claude/.credentials.json",
            LoginSource::PiCodex | LoginSource::PiCopilot | LoginSource::PiHyper => {
                "~/.pi/agent/auth.json"
            }
            LoginSource::CodexAuth => "~/.codex/auth.json",
        }
    }

    fn full_path(self) -> std::path::PathBuf {
        home().join(self.path().trim_start_matches("~/"))
    }

    fn hint(self) -> &'static str {
        match self {
            LoginSource::ClaudeCredentials => "log in again with claude auth login",
            LoginSource::PiCodex | LoginSource::PiCopilot | LoginSource::PiHyper => {
                "log in again with pi"
            }
            LoginSource::CodexAuth => "log in again with codex login",
        }
    }
}

fn default_account(worker: &Worker) -> Option<Account> {
    let account = match worker.harness {
        Harness::Claude => Account {
            name: AccountName::Claude,
            source: LoginSource::ClaudeCredentials,
        },
        Harness::Codex => Account {
            name: AccountName::Codex,
            source: LoginSource::CodexAuth,
        },
        Harness::Pi => {
            let provider = worker.model.split('/').next().unwrap_or_default();
            match provider {
                "openai-codex" => Account {
                    name: AccountName::Codex,
                    source: LoginSource::PiCodex,
                },
                "github-copilot" => Account {
                    name: AccountName::Copilot,
                    source: LoginSource::PiCopilot,
                },
                "hyper" => Account {
                    name: AccountName::Hyper,
                    source: LoginSource::PiHyper,
                },
                _ => return None,
            }
        }
        Harness::Opencode | Harness::Gemini => return None,
    };
    Some(account)
}

/// The login a mapping key names, following the vendor named at its start.
fn source_for_prefix(key: &str) -> Option<LoginSource> {
    let key = key.trim();
    let without_pi = key.strip_prefix("pi:").unwrap_or(key);
    if without_pi.starts_with("openai-codex") {
        Some(LoginSource::PiCodex)
    } else if without_pi.starts_with("github-copilot") {
        Some(LoginSource::PiCopilot)
    } else if without_pi.starts_with("hyper") {
        Some(LoginSource::PiHyper)
    } else if key == "claude" || key.starts_with("claude:") {
        Some(LoginSource::ClaudeCredentials)
    } else if key == "codex" || key.starts_with("codex:") {
        Some(LoginSource::CodexAuth)
    } else {
        None
    }
}

/// The longest `[accounts]` key that prefixes the whole candidate.
fn matching_override<'a>(
    worker: &Worker,
    mapping: &'a BTreeMap<String, String>,
) -> Option<(&'a str, &'a str)> {
    let candidate = worker.to_string();
    mapping
        .iter()
        .filter(|(key, _)| !key.trim().is_empty() && candidate.starts_with(key.trim()))
        .max_by_key(|(key, _)| key.trim().len())
        .map(|(key, value)| (key.trim(), value.as_str()))
}

/// The account a candidate uses, after `[accounts]` overrides.
pub fn account_for(worker: &Worker, mapping: &BTreeMap<String, String>) -> Option<Account> {
    let fallback = default_account(worker);
    if let Some((key, value)) = matching_override(worker, mapping) {
        let Some(name) = AccountName::parse(value) else {
            // An unknown account name means "not checked", never a guess.
            return None;
        };
        let named_source = match name {
            AccountName::Claude => Some(LoginSource::ClaudeCredentials),
            AccountName::Codex => Some(LoginSource::CodexAuth),
            AccountName::Copilot => Some(LoginSource::PiCopilot),
            AccountName::Hyper => Some(LoginSource::PiHyper),
        };
        let source = source_for_prefix(key)
            .or_else(|| fallback.map(|account| account.source))
            .or(named_source)?;
        return Some(Account { name, source });
    }
    fallback
}

/// The accounts used by these candidates, first occurrence order, one per name.
pub fn accounts_in_order(workers: &[Worker], mapping: &BTreeMap<String, String>) -> Vec<Account> {
    let mut accounts: Vec<Account> = Vec::new();
    for worker in workers {
        if let Some(account) = account_for(worker, mapping)
            && !accounts.contains(&account)
        {
            accounts.push(account);
        }
    }
    accounts
}

// ------------------------------------------------------------------ reports

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportStatus {
    Ok,
    Exhausted,
    Unknown,
}

impl ReportStatus {
    fn as_str(self) -> &'static str {
        match self {
            ReportStatus::Ok => "ok",
            ReportStatus::Exhausted => "exhausted",
            ReportStatus::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub name: String,
    pub percent_left: f64,
    pub resets_at: Option<i64>,
    pub seconds: i64,
    /// Model ids/names a scoped window applies to; empty for unscoped windows.
    pub scope: Vec<String>,
    /// The vendor gives no reset for this window. Unlike a reset that could not be
    /// read, this does not make the account unknown.
    pub no_reset: bool,
}

impl Window {
    /// A window the vendor deliberately leaves without a reset.
    fn resetless(&self) -> bool {
        self.no_reset && self.resets_at.is_none()
    }

    /// Whether this window counts for a candidate's model. Unscoped windows do.
    fn applies_to(&self, model: &str) -> bool {
        if self.scope.is_empty() {
            return true;
        }
        let model = model.trim().to_lowercase();
        !model.is_empty()
            && self
                .scope
                .iter()
                .any(|scope| scope.contains(&model) || model.contains(scope))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunwayStatus {
    ExhaustedNow,
    ProjectedExhaustion,
    ThroughReset,
    Unknown,
}

impl RunwayStatus {
    fn as_str(self) -> &'static str {
        match self {
            RunwayStatus::ExhaustedNow => "exhausted_now",
            RunwayStatus::ProjectedExhaustion => "projected_exhaustion",
            RunwayStatus::ThroughReset => "through_reset",
            RunwayStatus::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runway {
    pub status: RunwayStatus,
    /// When it runs out (projected) or the blocking window's reset.
    pub at: Option<i64>,
    /// The reset of the window that limits this runway.
    pub reset: Option<i64>,
    pub seconds_left: i64,
}

impl Runway {
    fn unknown() -> Self {
        Runway {
            status: RunwayStatus::Unknown,
            at: None,
            reset: None,
            seconds_left: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountReport {
    pub account: AccountName,
    pub login: LoginSource,
    pub source: &'static str,
    pub status: ReportStatus,
    pub windows: Vec<Window>,
    pub balance: Option<f64>,
    pub daily_cap: Option<f64>,
    pub runway: Runway,
    pub error: Option<String>,
    /// The login itself failed, so the hint to log in again is useful.
    login_failed: bool,
}

/// One candidate that quota says not to start, and the account line for a summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkipReason {
    pub account: AccountName,
    pub login: LoginSource,
    pub at: i64,
    pub reset: Option<i64>,
    pub reason: String,
    pub summary: String,
}

impl SkipReason {
    /// An explicit `--worker` is checked but never refused.
    pub fn explicit_warning(&self, worker: &Worker) -> String {
        format!(
            "warning: {worker}'s {} account {}; running it because --worker was given",
            self.account, self.reason
        )
    }
}

impl AccountReport {
    fn unknown(account: Account, error: impl Into<QuotaError>) -> Self {
        let error = error.into();
        AccountReport {
            account: account.name,
            login: account.source,
            source: account.source.path(),
            status: ReportStatus::Unknown,
            windows: Vec::new(),
            balance: None,
            daily_cap: None,
            runway: Runway::unknown(),
            error: Some(error.why),
            login_failed: error.login,
        }
    }

    fn from_windows(
        account: Account,
        windows: Vec<Window>,
        balance: Option<f64>,
        daily_cap: Option<f64>,
    ) -> Self {
        let runway = runway_for(&windows);
        let refs = windows.iter().collect::<Vec<_>>();
        let status = status_for_runway(runway, &refs);
        let error = (status == ReportStatus::Unknown).then(|| {
            if windows.iter().any(|window| !window.percent_left.is_finite()) {
                "vendor usage was not a number".to_string()
            } else {
                "a quota reset time could not be read".to_string()
            }
        });
        AccountReport {
            account: account.name,
            login: account.source,
            source: account.source.path(),
            status,
            windows,
            balance,
            daily_cap,
            runway,
            error,
            login_failed: false,
        }
    }

    /// Earliest reset named by any window.
    pub fn reset_at(&self) -> Option<i64> {
        self.windows
            .iter()
            .filter_map(|window| window.resets_at)
            .min()
    }

    /// Why this account cannot take a task of `size`, if it cannot.
    #[cfg(test)]
    pub fn skip_reason(&self, size: TaskSize, required: Duration) -> Option<SkipReason> {
        Self::reason_for_runway(self.account, self.login, self.runway, size, required)
    }

    /// The windows that count for this candidate's model.
    fn windows_for(&self, worker: &Worker) -> impl Iterator<Item = &Window> {
        self.windows
            .iter()
            .filter(|window| window.applies_to(&worker.model))
    }

    fn runway_for(&self, worker: &Worker) -> Runway {
        runway_for_iter(self.windows_for(worker))
    }

    /// A failed login leaves no windows; an unreadable reset still has them.
    fn login_unknown(&self) -> bool {
        self.status == ReportStatus::Unknown && self.windows.is_empty()
    }

    /// Whether quota says this candidate can run; a scoped window only counts for
    /// candidates whose model it names.
    pub fn status_for(&self, worker: &Worker) -> ReportStatus {
        if self.login_unknown() {
            return ReportStatus::Unknown;
        }
        let windows = self.windows_for(worker).collect::<Vec<_>>();
        status_for_runway(runway_for_iter(windows.iter().copied()), &windows)
    }

    /// Why this candidate cannot take a task of `size`, after model scoping.
    pub fn skip_reason_for(
        &self,
        worker: &Worker,
        size: TaskSize,
        required: Duration,
    ) -> Option<SkipReason> {
        if self.login_unknown() {
            return None;
        }
        Self::reason_for_runway(
            self.account,
            self.login,
            self.runway_for(worker),
            size,
            required,
        )
    }

    /// The warning to print when the quota behind this candidate could not be read.
    pub fn unknown_reason(&self) -> &str {
        self.error
            .as_deref()
            .unwrap_or("a quota reset time could not be read")
    }

    fn reason_for_runway(
        account: AccountName,
        login: LoginSource,
        runway: Runway,
        size: TaskSize,
        required: Duration,
    ) -> Option<SkipReason> {
        match runway.status {
            RunwayStatus::ExhaustedNow => {
                let at = runway.at.or(runway.reset);
                let clock = at.map_or_else(|| "its reset".to_string(), format_clock);
                Some(SkipReason {
                    account,
                    login,
                    at: at.unwrap_or(i64::MAX),
                    reset: runway.reset,
                    reason: format!("exhausted until {clock}"),
                    summary: format!("{account}: exhausted until {clock}"),
                })
            }
            // A reset replenishes the quota, so it is never a reason to skip.
            RunwayStatus::ThroughReset | RunwayStatus::Unknown => None,
            RunwayStatus::ProjectedExhaustion => {
                if runway.seconds_left >= i64::try_from(required.as_secs()).unwrap_or(i64::MAX) {
                    return None;
                }
                let at = runway.at.or(runway.reset);
                let clock = at.map_or_else(|| "its reset".to_string(), format_clock);
                let summary = runway.reset.map_or_else(
                    || format!("{account}: runs out at {clock}"),
                    |reset| {
                        format!(
                            "{account}: resets {}, runs out at {clock}",
                            format_clock(reset)
                        )
                    },
                );
                Some(SkipReason {
                    account,
                    login,
                    at: at.unwrap_or(i64::MAX),
                    reset: runway.reset,
                    reason: format!(
                        "runs out at {clock}, a {size} task needs {}",
                        duration_label(required)
                    ),
                    summary,
                })
            }
        }
    }

    /// A window the vendor deliberately leaves without a reset, not one that failed to read.
    fn resetless_only(&self) -> bool {
        !self.windows.is_empty() && self.windows.iter().all(Window::resetless)
    }

    /// Windows, reset and runway columns for a known report.
    fn text_columns(&self) -> (String, String, String) {
        let reset = if self.resetless_only() {
            "no reset given".to_string()
        } else {
            format!(
                "resets {}",
                self.reset_at()
                    .map_or_else(|| "unknown".to_string(), format_clock)
            )
        };
        let runway = format!("runway: {}", self.runway_text());
        let windows = if let (Some(balance), Some(cap)) = (self.balance, self.daily_cap) {
            format!("{}/{} credits", format_number(balance), format_number(cap))
        } else {
            self.windows
                .iter()
                .map(|window| {
                    format!(
                        "{}% left ({})",
                        format_percent(window.percent_left),
                        window.name
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        (windows, reset, runway)
    }

    fn runway_text(&self) -> String {
        match self.runway.status {
            RunwayStatus::ExhaustedNow => format!(
                "exhausted until {}",
                self.runway
                    .at
                    .or_else(|| self.reset_at())
                    .map_or_else(|| "its reset".to_string(), format_clock)
            ),
            RunwayStatus::ProjectedExhaustion => format!(
                "projected {}",
                self.runway
                    .at
                    .map_or_else(|| "unknown".to_string(), format_clock)
            ),
            RunwayStatus::ThroughReset => "through reset".to_string(),
            RunwayStatus::Unknown if self.resetless_only() => "no reset given".to_string(),
            RunwayStatus::Unknown => "unknown".to_string(),
        }
    }

    pub fn json(&self) -> Value {
        let windows = self
            .windows
            .iter()
            .map(|window| {
                json!({
                    "name": window.name,
                    "percentLeft": window.percent_left,
                    "resetsAt": window.resets_at.map(format_rfc3339),
                })
            })
            .collect::<Vec<_>>();
        let mut runway = serde_json::Map::new();
        runway.insert("status".into(), json!(self.runway.status.as_str()));
        if let Some(at) = self.runway.at {
            runway.insert("at".into(), json!(format_rfc3339(at)));
        }
        let mut object = serde_json::Map::new();
        object.insert("account".into(), json!(self.account.as_str()));
        object.insert("status".into(), json!(self.status.as_str()));
        object.insert("windows".into(), Value::Array(windows));
        if let Some(balance) = self.balance {
            object.insert("balance".into(), json!(balance));
        }
        object.insert("runway".into(), Value::Object(runway));
        object.insert("source".into(), json!(self.source));
        if let Some(error) = &self.error {
            object.insert("error".into(), json!(error));
        }
        Value::Object(object)
    }
}

/// `delegate quota` lines with account, windows, reset and runway columns aligned.
///
/// A reset or projection more than a day out carries its date (see [`format_clock`]),
/// so the variable-width columns are padded to keep the later ones readable.
pub fn text_lines(reports: &[AccountReport]) -> Vec<String> {
    let account_width = reports
        .iter()
        .map(|report| report.account.as_str().len())
        .max()
        .unwrap_or(0)
        .max(8);
    let columns = reports
        .iter()
        .map(|report| {
            (report.status != ReportStatus::Unknown).then(|| report.text_columns())
        })
        .collect::<Vec<_>>();
    let width = |select: fn(&(String, String, String)) -> &String| {
        columns
            .iter()
            .flatten()
            .map(|columns| select(columns).len())
            .max()
            .unwrap_or(0)
    };
    let windows_width = width(|(windows, _, _)| windows);
    let reset_width = width(|(_, reset, _)| reset);
    let runway_width = width(|(_, _, runway)| runway);
    reports
        .iter()
        .zip(columns)
        .map(|(report, columns)| match columns {
            None => {
                let why = report.error.as_deref().unwrap_or("quota unreadable");
                let hint = if report.login_failed {
                    format!("; {}", report.login.hint())
                } else {
                    String::new()
                };
                format!(
                    "{:<account_width$} unknown: {why} ({}){hint}",
                    report.account, report.source
                )
            }
            Some((windows, reset, runway)) => format!(
                "{:<account_width$} {windows:<windows_width$}   {reset:<reset_width$}   {runway:<runway_width$}   [{}]",
                report.account, report.source
            ),
        })
        .collect()
}

/// Read every account in parallel; each request has a five-second budget.
pub fn read_accounts(accounts: &[Account], config: &QuotaConfig) -> Vec<AccountReport> {
    std::thread::scope(|scope| {
        let handles = accounts
            .iter()
            .map(|account| scope.spawn(move || read_account(*account, config)))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .zip(accounts)
            .map(|(handle, account)| match handle.join() {
                Ok(report) => report,
                Err(_) => AccountReport::unknown(*account, "quota reader failed"),
            })
            .collect()
    })
}

fn read_account(account: Account, config: &QuotaConfig) -> AccountReport {
    let result = match account.source {
        LoginSource::ClaudeCredentials => read_claude_login()
            .map_err(QuotaError::login)
            .and_then(|login| fetch_claude(account, &login)),
        LoginSource::PiCodex | LoginSource::CodexAuth => read_codex_login(account.source)
            .map_err(QuotaError::login)
            .and_then(|login| fetch_codex(account, &login)),
        LoginSource::PiCopilot => read_copilot_login()
            .map_err(QuotaError::login)
            .and_then(|login| fetch_copilot(account, &login)),
        LoginSource::PiHyper => read_hyper_login()
            .map_err(QuotaError::login)
            .and_then(|login| fetch_hyper(account, &login, config.hyper)),
    };
    result.unwrap_or_else(|error| AccountReport::unknown(account, error))
}

// -------------------------------------------------------------------- logins

enum Login {
    Claude {
        token: String,
    },
    Codex {
        token: String,
        account_id: Option<String>,
    },
    Copilot {
        token: String,
    },
    Hyper {
        access: String,
        refresh: Option<String>,
        expires_ms: i64,
    },
}

/// A failed quota read and whether the login itself is what failed. Only a login
/// failure deserves the hint to log in again.
struct QuotaError {
    why: String,
    login: bool,
}

impl QuotaError {
    fn login(why: impl Into<String>) -> Self {
        QuotaError {
            why: why.into(),
            login: true,
        }
    }
}

impl From<String> for QuotaError {
    fn from(why: String) -> Self {
        QuotaError { why, login: false }
    }
}

impl From<&str> for QuotaError {
    fn from(why: &str) -> Self {
        QuotaError {
            why: why.to_string(),
            login: false,
        }
    }
}

type FetchResult = Result<AccountReport, QuotaError>;

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "login missing".to_string()
        } else {
            "login file cannot be read".to_string()
        }
    })?;
    serde_json::from_str(&text).map_err(|_| "login file is not valid JSON".to_string())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A numeric field as JSON or string, including non-finite values, so readers can
/// tell a missing field from one that is present but unusable.
fn raw_number_field(value: &Value, key: &str) -> Option<f64> {
    match value.get(key)? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().replace('%', "").parse().ok(),
        _ => None,
    }
}

fn number_field(value: &Value, key: &str) -> Option<f64> {
    raw_number_field(value, key).filter(|number| number.is_finite())
}

/// `Ok(None)` when the key names no number (missing, null, or not numeric);
/// `Err(())` when it names a non-finite one, which makes that window unreadable.
fn checked_number_field(value: &Value, key: &str) -> Result<Option<f64>, ()> {
    match raw_number_field(value, key) {
        None => Ok(None),
        Some(number) if number.is_finite() => Ok(Some(number)),
        Some(_) => Err(()),
    }
}

/// The first usable number among alternative keys, e.g. a `null` newer field
/// falling back to the older one as the Omarchy readers do.
fn first_number_field(value: &Value, keys: &[&str]) -> Result<Option<f64>, ()> {
    for key in keys {
        match checked_number_field(value, key) {
            Ok(Some(number)) => return Ok(Some(number)),
            Err(()) => return Err(()),
            Ok(None) => {}
        }
    }
    Ok(None)
}

/// The first readable time among alternative keys, skipping null and empty ones.
///
/// A reset or window time at or before now (epoch 0, negative values, a stale
/// date) says nothing about the next reset, so it is unreadable too: fall back to
/// the next key and leave the window without a reset if none is usable.
fn first_time(value: &Value, keys: &[&str]) -> Option<i64> {
    let now = now_seconds();
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(parse_time)
            .filter(|at| *at > now)
    })
}

fn object_bucket<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|entry| entry.is_object())
}

/// A window whose usage number was present but not finite; it makes the account
/// unknown instead of looking healthy or exhausted.
fn unreadable_window(name: impl Into<String>, resets_at: Option<i64>, seconds: i64) -> Window {
    Window {
        name: name.into(),
        percent_left: f64::NAN,
        resets_at,
        seconds,
        scope: Vec::new(),
        no_reset: false,
    }
}

fn read_claude_login() -> Result<Login, String> {
    let data = read_json(&LoginSource::ClaudeCredentials.full_path())?;
    let login = data
        .get("claudeAiOauth")
        .ok_or_else(|| "no Claude OAuth login".to_string())?;
    let token = string_field(login, "accessToken")
        .ok_or_else(|| "login has no access token".to_string())?;
    let expires_ms = number_field(login, "expiresAt").unwrap_or(0.0);
    if expires_ms > 0.0 && expires_ms as i64 <= now_millis() {
        return Err("login expired".to_string());
    }
    Ok(Login::Claude { token })
}

fn read_codex_login(source: LoginSource) -> Result<Login, String> {
    let data = read_json(&source.full_path())?;
    let (entry, token_key) = match source {
        LoginSource::PiCodex => (
            data.get("openai-codex")
                .ok_or_else(|| "no openai-codex login".to_string())?,
            "access",
        ),
        _ => (data.get("tokens").unwrap_or(&data), "access_token"),
    };
    codex_login_from(entry, token_key)
}

fn codex_login_from(entry: &Value, token_key: &str) -> Result<Login, String> {
    let account_id = string_field(entry, "accountId").or_else(|| string_field(entry, "account_id"));
    if let Some(key) = pi_api_key(entry) {
        return Ok(Login::Codex {
            token: key,
            account_id,
        });
    }
    let token =
        string_field(entry, token_key).ok_or_else(|| "login has no access token".to_string())?;
    Ok(Login::Codex { token, account_id })
}

fn read_copilot_login() -> Result<Login, String> {
    let data = read_json(&LoginSource::PiCopilot.full_path())?;
    let entry = data
        .get("github-copilot")
        .ok_or_else(|| "no github-copilot login".to_string())?;
    copilot_login_from(entry)
}

fn copilot_login_from(entry: &Value) -> Result<Login, String> {
    if let Some(key) = pi_api_key(entry) {
        return Ok(Login::Copilot { token: key });
    }
    let token = string_field(entry, "refresh")
        .or_else(|| string_field(entry, "access"))
        .ok_or_else(|| "login has no GitHub token".to_string())?;
    Ok(Login::Copilot { token })
}

fn read_hyper_login() -> Result<Login, String> {
    let data = read_json(&LoginSource::PiHyper.full_path())?;
    let entry = data
        .get("hyper")
        .ok_or_else(|| "no hyper login".to_string())?;
    Ok(hyper_login_from(entry))
}

/// Pi also stores plain API keys, not only OAuth logins. Such a key is used
/// directly as the bearer; there is nothing to refresh or exchange.
fn pi_api_key(entry: &Value) -> Option<String> {
    (string_field(entry, "type").as_deref() == Some("api_key"))
        .then(|| string_field(entry, "key"))
        .flatten()
}

fn hyper_login_from(entry: &Value) -> Login {
    if let Some(key) = pi_api_key(entry) {
        return Login::Hyper {
            access: key,
            refresh: None,
            expires_ms: i64::MAX,
        };
    }
    let access = string_field(entry, "access").unwrap_or_default();
    let refresh = string_field(entry, "refresh");
    let expires_ms = number_field(entry, "expires").unwrap_or(0.0) as i64;
    Login::Hyper {
        access,
        refresh,
        expires_ms,
    }
}

// ------------------------------------------------------------------ requests

/// A test seam is only usable for a loopback address; anything else keeps the vendor base.
///
/// Accept exactly `http://127.0.0.1:<port>` or `http://localhost:<port>` with a numeric
/// port in 1-65535 and nothing after it but an optional `/`. Prefix matching would let
/// a host such as `127.0.0.1:@evil.example` through.
fn loopback_base(value: &str) -> Option<String> {
    let value = value.trim();
    let rest = value.strip_prefix("http://")?;
    let (host, port) = rest.split_once(':')?;
    if host != "127.0.0.1" && host != "localhost" {
        return None;
    }
    let port = port.strip_suffix('/').unwrap_or(port);
    if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let port: u32 = port.parse().ok()?;
    if !(1..=65_535).contains(&port) {
        return None;
    }
    Some(format!("http://{host}:{port}"))
}

fn base_url(variable: &str, default: &str) -> String {
    if let Some(value) = std::env::var_os(variable) {
        let value = value.to_string_lossy();
        if let Some(base) = loopback_base(&value) {
            return base;
        }
        eprintln!(
            "warning: ignoring {variable}: quota base URLs must start with http://127.0.0.1: or http://localhost:"
        );
    }
    default.trim_end_matches('/').to_string()
}

fn http_get(url: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
    http_get_with(url, headers, REQUEST_TIMEOUT)
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            // A redirect could leave the configured vendor; never follow one.
            .max_redirects(0)
            .http_status_as_error(false)
            .build(),
    )
}

fn http_get_with(
    url: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<(u16, String), String> {
    let agent = http_agent(timeout);
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    response_text(request.call())
}

fn http_post_with(
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let agent = http_agent(timeout);
    let mut request = agent.post(url).header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    response_text(request.send(body))
}

/// The time left in one account's five-second budget; a request never gets a fresh one.
fn budget_left(deadline: Instant) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err("vendor timed out".to_string())
    } else {
        Ok(left)
    }
}

fn response_text(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<(u16, String), String> {
    let mut response = response.map_err(|error| match error {
        ureq::Error::Timeout(_) => "vendor timed out".to_string(),
        ureq::Error::HostNotFound => "vendor unreachable".to_string(),
        ureq::Error::StatusCode(code) => format!("vendor returned HTTP {code}"),
        _ => "vendor request failed".to_string(),
    })?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| "vendor response could not be read".to_string())?;
    Ok((status, text))
}

pub fn base_for(account: AccountName) -> String {
    match account {
        AccountName::Claude => base_url("DELEGATE_QUOTA_BASE_CLAUDE", CLAUDE_BASE),
        AccountName::Codex => base_url("DELEGATE_QUOTA_BASE_CODEX", CODEX_BASE),
        AccountName::Copilot => base_url("DELEGATE_QUOTA_BASE_COPILOT", COPILOT_BASE),
        AccountName::Hyper => base_url("DELEGATE_QUOTA_BASE_HYPER", HYPER_BASE),
    }
}

fn fetch_claude(account: Account, login: &Login) -> FetchResult {
    let Login::Claude { token } = login else {
        return Err("quota reader mismatch".into());
    };
    let url = format!("{}/api/oauth/usage", base_for(AccountName::Claude));
    let (status, body) = http_get(
        &url,
        &[
            ("Authorization", &format!("Bearer {token}")),
            ("anthropic-beta", "oauth-2025-04-20"),
            ("Accept", "application/json"),
        ],
    )?;
    if status == 401 || status == 403 {
        return Err(QuotaError::login("login expired"));
    }
    let payload = json_body(status, &body)?;
    let windows = claude_windows(&payload)?;
    Ok(AccountReport::from_windows(account, windows, None, None))
}

/// Claude's flat buckets plus the model-scoped entries in its `limits` array.
fn claude_windows(payload: &Value) -> Result<Vec<Window>, String> {
    // Only JSON objects are buckets; a null placeholder must not hide the real one.
    let weekly = object_bucket(payload, "seven_day_oauth_apps")
        .or_else(|| object_bucket(payload, "seven_day"));
    let session = object_bucket(payload, "five_hour");
    let raw = [session, weekly]
        .into_iter()
        .flatten()
        .filter_map(|entry| number_field(entry, "utilization"))
        .chain(scoped_entries(payload).filter_map(|entry| number_field(entry, "percent")))
        .collect::<Vec<_>>();
    let percent_scale = raw.iter().any(|value| *value >= 1.0);
    let mut windows = Vec::new();
    for (entry, name, seconds) in [
        (weekly, "weekly".to_string(), SEVEN_DAYS),
        (session, "5-hour".to_string(), FIVE_HOURS),
    ] {
        let Some(entry) = entry else { continue };
        let resets_at = first_time(entry, &["resets_at"]);
        let used = match checked_number_field(entry, "utilization") {
            Ok(Some(used)) => used,
            Ok(None) => continue,
            Err(()) => {
                windows.push(unreadable_window(name, resets_at, seconds));
                continue;
            }
        };
        let used = if percent_scale { used } else { used * 100.0 };
        windows.push(Window {
            name,
            percent_left: (100.0 - used).clamp(0.0, 100.0),
            resets_at,
            seconds,
            scope: Vec::new(),
            no_reset: false,
        });
    }
    windows.extend(claude_scoped_windows(payload, percent_scale));
    if windows.is_empty()
        && let Some(monthly) = claude_monthly_window(payload)
    {
        windows.push(monthly);
    }
    if windows.is_empty() {
        return Err("vendor answer had no quota windows".to_string());
    }
    Ok(windows)
}

/// Enterprise seats have no plan windows, only an organisation-set monthly spend cap.
/// It is read only when `extra_usage` is enabled; a plan window always wins over it.
fn claude_monthly_window(payload: &Value) -> Option<Window> {
    let Some(extra) = object_bucket(payload, "extra_usage") else {
        return claude_spend_window(payload);
    };
    if extra.get("is_enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let absent = |key: &str| extra.get(key).is_none_or(Value::is_null);
    if absent("used_credits") || absent("monthly_limit") {
        return claude_spend_window(payload);
    }
    let limit = match checked_number_field(extra, "monthly_limit") {
        Ok(Some(limit)) if limit > 0.0 => limit,
        Ok(_) => return None,
        Err(()) => return Some(unreadable_window("monthly", None, THIRTY_DAYS)),
    };
    let used = match checked_number_field(extra, "used_credits") {
        Ok(Some(used)) => used,
        Ok(None) => return claude_spend_window(payload),
        Err(()) => return Some(unreadable_window("monthly", None, THIRTY_DAYS)),
    };
    let reached = extra.get("spend_limit_reached").and_then(Value::as_bool) == Some(true);
    let percent_left = if reached {
        0.0
    } else {
        (100.0 - used / limit * 100.0).clamp(0.0, 100.0)
    };
    Some(monthly_window(percent_left))
}

/// The older `spend` figures, used only when `extra_usage` names no amounts.
fn claude_spend_window(payload: &Value) -> Option<Window> {
    let spend = object_bucket(payload, "spend")?;
    let used = checked_number_field(object_bucket(spend, "used")?, "amount_minor");
    let limit = checked_number_field(object_bucket(spend, "limit")?, "amount_minor");
    if used.is_err() || limit.is_err() {
        return Some(unreadable_window("monthly", None, THIRTY_DAYS));
    }
    let (Ok(Some(used)), Ok(Some(limit))) = (used, limit) else {
        return None;
    };
    if limit <= 0.0 {
        return None;
    }
    Some(monthly_window(
        (100.0 - used / limit * 100.0).clamp(0.0, 100.0),
    ))
}

fn monthly_window(percent_left: f64) -> Window {
    Window {
        name: "monthly".to_string(),
        percent_left,
        resets_at: None,
        seconds: THIRTY_DAYS,
        scope: Vec::new(),
        no_reset: true,
    }
}

/// The model-scoped entries in Claude's `limits` array that the flat buckets miss.
fn scoped_entries(payload: &Value) -> impl Iterator<Item = &Value> {
    payload
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.is_object())
        .filter(|entry| {
            entry
                .get("scope")
                .and_then(|scope| scope.get("model"))
                .is_some_and(Value::is_object)
        })
}

fn claude_scoped_windows(payload: &Value, percent_scale: bool) -> Vec<Window> {
    let mut windows = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for entry in scoped_entries(payload) {
        let Some(model) = entry
            .get("scope")
            .and_then(|scope| scope.get("model"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_lowercase)
            .unwrap_or_default();
        let display = model
            .get("display_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or_default()
            .to_string();
        let title = if display.is_empty() {
            id.clone()
        } else {
            display
        };
        let kind = string_field(entry, "kind").unwrap_or_default();
        if title.is_empty() || seen.contains(&(title.clone(), kind.clone())) {
            continue;
        }
        let kind_lower = kind.to_lowercase();
        let (window_name, seconds) = if kind_lower.contains("month") {
            ("monthly", THIRTY_DAYS)
        } else if kind_lower.contains("week") || kind_lower.contains("day") {
            ("weekly", SEVEN_DAYS)
        } else if kind_lower.contains("hour") || kind_lower.contains("session") {
            ("5-hour", FIVE_HOURS)
        } else {
            ("scoped", SEVEN_DAYS)
        };
        let mut scope = vec![title.to_lowercase()];
        if !id.is_empty() {
            scope.push(id);
        }
        seen.push((title.clone(), kind));
        let resets_at = first_time(entry, &["resets_at"]);
        let used = match checked_number_field(entry, "percent") {
            Ok(Some(used)) => used,
            Ok(None) => continue,
            Err(()) => {
                let mut window = unreadable_window(format!("{title} {window_name}"), resets_at, seconds);
                window.scope = scope;
                windows.push(window);
                continue;
            }
        };
        let used = if percent_scale { used } else { used * 100.0 };
        windows.push(Window {
            name: format!("{title} {window_name}"),
            percent_left: (100.0 - used).clamp(0.0, 100.0),
            resets_at,
            seconds,
            scope,
            no_reset: false,
        });
    }
    windows
}

fn fetch_codex(account: Account, login: &Login) -> FetchResult {
    let Login::Codex { token, account_id } = login else {
        return Err("quota reader mismatch".into());
    };
    let url = format!("{}/backend-api/wham/usage", base_for(AccountName::Codex));
    let mut headers = vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    if let Some(id) = account_id {
        headers.push(("ChatGPT-Account-Id".to_string(), id.clone()));
    }
    let borrowed = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let (status, body) = http_get(&url, &borrowed)?;
    if status == 401 || status == 403 {
        return Err(QuotaError::login("login expired"));
    }
    let payload = json_body(status, &body)?;
    let windows = codex_windows(&payload)?;
    Ok(AccountReport::from_windows(account, windows, None, None))
}

/// Codex's rate-limit windows, falling back past null alternative fields.
fn codex_windows(payload: &Value) -> Result<Vec<Window>, String> {
    let container = ["rate_limit", "rateLimits", "rate_limits"]
        .iter()
        .find_map(|key| object_bucket(payload, key))
        .unwrap_or(payload);
    let mut windows = Vec::new();
    for (keys, fallback, default_name) in [
        (&["primary_window", "primary"][..], FIVE_HOURS, "5-hour"),
        (&["secondary_window", "secondary"][..], SEVEN_DAYS, "weekly"),
    ] {
        let entry = keys.iter().find_map(|key| object_bucket(container, key));
        let Some(entry) = entry else { continue };
        let used = first_number_field(entry, &["used_percent", "usedPercent"]);
        let seconds_field = first_number_field(entry, &["limit_window_seconds"]).and_then(
            |seconds| match seconds {
                Some(value) => Ok(Some(value)),
                None => checked_number_field(entry, "windowDurationMins")
                    .map(|minutes| minutes.map(|minutes| minutes * 60.0)),
            },
        );
        let unreadable = used.is_err() || seconds_field.is_err();
        let seconds = match seconds_field {
            Ok(Some(value)) if value > 0.0 => value as i64,
            _ => fallback,
        };
        let name = match seconds {
            FIVE_HOURS => "5-hour",
            SEVEN_DAYS => "weekly",
            _ if default_name == "weekly" => "weekly",
            _ => "window",
        };
        let resets_at = first_time(entry, &["reset_at", "resetsAt"]).or_else(|| {
            // A relative delay has the same rule as an absolute reset: one at or
            // before now is unreadable, not the next window.
            let now = now_seconds();
            number_field(entry, "reset_after_seconds")
                .map(|seconds| now + seconds as i64)
                .filter(|at| *at > now)
        });
        if unreadable {
            windows.push(unreadable_window(name, resets_at, seconds));
            continue;
        }
        let Some(used) = used.ok().flatten() else {
            continue;
        };
        windows.push(Window {
            name: name.to_string(),
            percent_left: (100.0 - used).clamp(0.0, 100.0),
            resets_at,
            seconds,
            scope: Vec::new(),
            no_reset: false,
        });
    }
    if windows.is_empty() {
        return Err("vendor answer had no quota windows".to_string());
    }
    Ok(windows)
}

fn fetch_copilot(account: Account, login: &Login) -> FetchResult {
    let Login::Copilot { token } = login else {
        return Err("quota reader mismatch".into());
    };
    let url = format!("{}/copilot_internal/user", base_for(AccountName::Copilot));
    let (status, body) = http_get(
        &url,
        &[
            ("Authorization", &format!("Bearer {token}")),
            ("Accept", "application/json"),
            ("User-Agent", "GitHubCopilotCLI/1.0"),
        ],
    )?;
    if status == 401 || status == 403 {
        return Err(QuotaError::login("login expired"));
    }
    let payload = json_body(status, &body)?;
    let windows = copilot_windows_from(&payload)?;
    Ok(AccountReport::from_windows(account, windows, None, None))
}

/// Copilot's per-category windows from one `/copilot_internal/user` answer.
fn copilot_windows_from(payload: &Value) -> Result<Vec<Window>, String> {
    let Some(snapshots) = payload.get("quota_snapshots").and_then(Value::as_object) else {
        return Err("vendor answer had no quota windows".to_string());
    };
    // A null newer field falls back to the older one, as the Omarchy reader does.
    let fallback = first_time(
        payload,
        &["quota_reset_date_utc", "quota_reset_date", "quotaResetDate"],
    );
    let windows = copilot_windows(snapshots, fallback);
    if windows.is_empty() {
        return Err("vendor answer had no quota windows".to_string());
    }
    Ok(windows)
}

/// Only metered Copilot categories count; unlimited ones can never run out.
fn copilot_windows(
    snapshots: &serde_json::Map<String, Value>,
    fallback: Option<i64>,
) -> Vec<Window> {
    let mut windows = Vec::new();
    for (id, entry) in snapshots {
        let percent_left = match copilot_quota(entry) {
            CopilotQuota::Unlimited => continue,
            CopilotQuota::Percent(percent) => percent.clamp(0.0, 100.0),
            CopilotQuota::Unreadable => f64::NAN,
        };
        let resets_at = first_time(entry, &["quota_reset_at", "resetDate", "reset_date"])
            .or(fallback);
        let seconds = resets_at.map_or(THIRTY_DAYS, copilot_window_seconds);
        windows.push(Window {
            name: id.replace('_', " "),
            percent_left,
            resets_at,
            seconds,
            scope: Vec::new(),
            no_reset: false,
        });
    }
    windows
}

/// What one Copilot category says about its metered quota.
enum CopilotQuota {
    /// Unlimited, or not a metered category: it never restricts the account.
    Unlimited,
    /// Percent remaining in the category.
    Percent(f64),
    /// A usage field was present but not a finite number.
    Unreadable,
}

/// Percent left in one Copilot category, from either the classic or the SDK shape.
fn copilot_quota(entry: &Value) -> CopilotQuota {
    if !entry.is_object() || entry.get("unlimited").and_then(Value::as_bool) == Some(true) {
        return CopilotQuota::Unlimited;
    }
    // The SDK shape wins when any of its fields is present, as in the Omarchy reader.
    if entry.get("entitlementRequests").is_some()
        || entry.get("usedRequests").is_some()
        || entry.get("remainingPercentage").is_some()
    {
        let entitlement = match checked_number_field(entry, "entitlementRequests") {
            Ok(value) => value.unwrap_or(0.0),
            Err(()) => return CopilotQuota::Unreadable,
        };
        if entitlement < 0.0 {
            return CopilotQuota::Unlimited;
        }
        match checked_number_field(entry, "remainingPercentage") {
            Err(()) => return CopilotQuota::Unreadable,
            Ok(Some(percent)) => return CopilotQuota::Percent(percent),
            Ok(None) => {}
        }
        if entitlement > 0.0 {
            let used = match checked_number_field(entry, "usedRequests") {
                Ok(value) => value.unwrap_or(0.0),
                Err(()) => return CopilotQuota::Unreadable,
            };
            return CopilotQuota::Percent((entitlement - used) / entitlement * 100.0);
        }
        return CopilotQuota::Unlimited;
    }
    let entitlement = match checked_number_field(entry, "entitlement") {
        Ok(Some(entitlement)) => entitlement,
        Ok(None) => return CopilotQuota::Unlimited,
        Err(()) => return CopilotQuota::Unreadable,
    };
    if entitlement <= 0.0 {
        return CopilotQuota::Unlimited;
    }
    match checked_number_field(entry, "percent_remaining") {
        Err(()) => return CopilotQuota::Unreadable,
        Ok(Some(percent)) => return CopilotQuota::Percent(percent),
        Ok(None) => {}
    }
    let remaining = match checked_number_field(entry, "remaining") {
        Err(()) => return CopilotQuota::Unreadable,
        Ok(Some(remaining)) => Some(remaining),
        Ok(None) => match checked_number_field(entry, "quota_remaining") {
            Err(()) => return CopilotQuota::Unreadable,
            Ok(remaining) => remaining,
        },
    };
    CopilotQuota::Percent(remaining.unwrap_or(0.0) / entitlement * 100.0)
}

/// Copilot's window runs from the first of the month before the reset to the reset.
fn copilot_window_seconds(reset: i64) -> i64 {
    let (year, month, _, _, _, _) = civil_parts(reset);
    let (previous_year, previous_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    let start = days_from_civil(previous_year, previous_month, 1) * ONE_DAY;
    let seconds = reset - start;
    if seconds > 0 { seconds } else { THIRTY_DAYS }
}

fn fetch_hyper(account: Account, login: &Login, config: HyperConfig) -> FetchResult {
    let Login::Hyper {
        access,
        refresh,
        expires_ms,
    } = login
    else {
        return Err("quota reader mismatch".into());
    };
    // The token exchange and the credits request share one five-second budget.
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let token = if !access.is_empty() && expires_ms - HYPER_EXPIRY_BUFFER_MS > now_millis() {
        access.clone()
    } else if let Some(refresh) = refresh {
        exchange_hyper_token(refresh, deadline)?
    } else {
        return Err(QuotaError::login("login expired"));
    };
    let url = format!("{}/v1/credits", base_for(AccountName::Hyper));
    let (status, body) = http_get_with(
        &url,
        &[
            ("Authorization", &format!("Bearer {token}")),
            ("Accept", "application/json"),
        ],
        budget_left(deadline)?,
    )?;
    if status == 401 || status == 403 {
        return Err(QuotaError::login("login expired"));
    }
    let payload = json_body(status, &body)?;
    let balance = match first_number_field(&payload, &["balance", "balance_usd"]) {
        Ok(Some(balance)) => balance,
        Ok(None) => return Err("vendor answer had no credit balance".into()),
        Err(()) => return Err("vendor credit balance was not a number".into()),
    };
    let cap = f64::from(config.daily_cap);
    let percent_left = (balance / cap * 100.0).clamp(0.0, 100.0);
    let now = now_seconds();
    let resets_at = Some(next_daily_reset(now, config.resets_at));
    let windows = vec![Window {
        name: "daily".to_string(),
        percent_left,
        resets_at,
        seconds: ONE_DAY,
        scope: Vec::new(),
        no_reset: false,
    }];
    Ok(AccountReport::from_windows(
        account,
        windows,
        Some(balance),
        Some(cap),
    ))
}

fn exchange_hyper_token(refresh: &str, deadline: Instant) -> Result<String, QuotaError> {
    let url = format!("{}/token/exchange", base_for(AccountName::Hyper));
    let body = json!({ "refresh_token": refresh }).to_string();
    let (status, text) = http_post_with(
        &url,
        &[("Accept", "application/json")],
        &body,
        budget_left(deadline)?,
    )?;
    if status == 401 || status == 403 {
        return Err(QuotaError::login("login expired"));
    }
    let payload = json_body(status, &text)?;
    string_field(&payload, "access_token")
        .ok_or_else(|| "vendor answer had no access token".to_string())
        .map_err(QuotaError::from)
}

fn json_body(status: u16, text: &str) -> Result<Value, String> {
    if status != 200 {
        return Err(format!("vendor returned HTTP {status}"));
    }
    serde_json::from_str(text).map_err(|_| "vendor answered with something unusable".to_string())
}

// ------------------------------------------------------------------- runway

/// The report status for a runway. A window the vendor deliberately leaves without a
/// reset still leaves the runway unknown, but the account is ok and not an error.
fn status_for_runway(runway: Runway, windows: &[&Window]) -> ReportStatus {
    match runway.status {
        RunwayStatus::ExhaustedNow => ReportStatus::Exhausted,
        RunwayStatus::Unknown
            if !windows.is_empty() && windows.iter().all(|window| window.resetless()) =>
        {
            ReportStatus::Ok
        }
        RunwayStatus::Unknown => ReportStatus::Unknown,
        RunwayStatus::ProjectedExhaustion | RunwayStatus::ThroughReset => ReportStatus::Ok,
    }
}

/// The tightest runway across every window, not just the shortest-duration one.
/// An unreadable reset makes the account unknown unless some window is exhausted.
fn runway_for(windows: &[Window]) -> Runway {
    runway_for_iter(windows.iter())
}

fn runway_for_iter<'a>(windows: impl IntoIterator<Item = &'a Window>) -> Runway {
    let now = now_seconds();
    let mut best: Option<Runway> = None;
    let mut unknown = false;
    for window in windows {
        // A non-finite usage number says nothing about the account; never read it
        // as healthy or exhausted.
        if !window.percent_left.is_finite() {
            return Runway::unknown();
        }
        let Some(runway) = window_runway(window, now) else {
            // A vendor-given absence of a reset is not an unreadable reset.
            if !window.resetless() {
                unknown = true;
            }
            continue;
        };
        best = Some(best.map_or(runway, |current| tighter(current, runway)));
    }
    match best {
        Some(runway) if runway.status == RunwayStatus::ExhaustedNow => runway,
        _ if unknown => Runway::unknown(),
        Some(runway) => runway,
        None => Runway::unknown(),
    }
}

/// Exhaustion matters most, then the earliest projected exhaustion, then the reset that
/// replenishes the quota. A reset is never worse than running out.
fn tighter(current: Runway, other: Runway) -> Runway {
    let rank = |runway: Runway| match runway.status {
        RunwayStatus::ExhaustedNow => 0,
        RunwayStatus::ProjectedExhaustion => 1,
        RunwayStatus::ThroughReset => 2,
        RunwayStatus::Unknown => 3,
    };
    if rank(other) < rank(current)
        || (rank(other) == rank(current) && other.seconds_left < current.seconds_left)
    {
        other
    } else {
        current
    }
}

fn window_runway(window: &Window, now: i64) -> Option<Runway> {
    let window_seconds = window.seconds;
    if window_seconds <= 0 || !window.percent_left.is_finite() {
        return None;
    }
    let used = 1.0 - (window.percent_left / 100.0);
    if used >= 1.0 {
        // Exhaustion is known even when the reset time cannot be read.
        return Some(Runway {
            status: RunwayStatus::ExhaustedNow,
            at: window.resets_at,
            reset: window.resets_at,
            seconds_left: 0,
        });
    }
    let reset = window.resets_at?;
    if used <= 0.0 || reset <= now {
        return Some(Runway {
            status: RunwayStatus::ThroughReset,
            at: Some(reset),
            reset: Some(reset),
            seconds_left: (reset - now).max(0),
        });
    }
    let elapsed = (window_seconds - (reset - now)) as f64;
    let elapsed_fraction = (elapsed / window_seconds as f64).clamp(0.0, 1.0);
    let to_exhaustion = (1.0 - used) * window_seconds as f64 * elapsed_fraction / used;
    let projected = now + to_exhaustion as i64;
    if projected > reset {
        Some(Runway {
            status: RunwayStatus::ThroughReset,
            at: Some(reset),
            reset: Some(reset),
            seconds_left: (reset - now).max(0),
        })
    } else {
        Some(Runway {
            status: RunwayStatus::ProjectedExhaustion,
            at: Some(projected),
            reset: Some(reset),
            seconds_left: (projected - now).max(0),
        })
    }
}

fn next_daily_reset(now: i64, reset: LocalTime) -> i64 {
    let offset = offset_seconds();
    let local = now + i64::from(offset.0);
    let day_start = local - local.rem_euclid(ONE_DAY);
    let mut at = day_start + i64::from(reset.hour) * 3_600 + i64::from(reset.minute) * 60;
    if at <= local {
        at += ONE_DAY;
    }
    at - i64::from(offset.0)
}

// -------------------------------------------------------------------- times

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs().cast_signed())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// Local UTC offset from `date +%z`, once per process; UTC if that fails.
fn offset_seconds() -> (i32, bool) {
    static OFFSET: OnceLock<(i32, bool)> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let output = std::process::Command::new("date").arg("+%z").output();
        let Ok(output) = output else {
            return (0, false);
        };
        if !output.status.success() {
            return (0, false);
        }
        parse_offset(&String::from_utf8_lossy(&output.stdout))
            .map_or((0, false), |seconds| (seconds, true))
    })
}

fn parse_offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let (sign, digits) = match text.split_at_checked(1)? {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => return None,
    };
    let digits = digits.replace(':', "");
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Local clock time; when it is more than a day away, its date comes too.
pub fn format_clock(epoch: i64) -> String {
    format_clock_at(epoch, now_seconds(), offset_seconds())
}

fn format_clock_at(epoch: i64, now: i64, offset: (i32, bool)) -> String {
    let (_, month, day, hour, minute, _) = civil_parts(epoch + i64::from(offset.0));
    let suffix = if offset.1 { "" } else { " UTC" };
    if epoch.saturating_sub(now) > ONE_DAY {
        let month = MONTHS
            .get((month as usize).saturating_sub(1))
            .copied()
            .unwrap_or("???");
        format!("{month} {day} {hour:02}:{minute:02}{suffix}")
    } else {
        format!("{hour:02}:{minute:02}{suffix}")
    }
}

pub fn format_rfc3339(epoch: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_parts(epoch);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_parts(epoch: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = epoch.div_euclid(ONE_DAY);
    let seconds = epoch.rem_euclid(ONE_DAY);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (seconds / 3_600) as u32,
        ((seconds % 3_600) / 60) as u32,
        (seconds % 60) as u32,
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let month = i64::from(month);
    let day = i64::from(day);
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn valid_date(year: i64, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day)
}

fn parse_date_only(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !valid_date(year, month, day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * ONE_DAY)
}

fn parse_time(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().map(|value| {
            if value > 10_000_000_000 {
                value / 1_000
            } else {
                value
            }
        }),
        Value::String(text) => parse_rfc3339(text),
        _ => None,
    }
}

fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(value) = text.parse::<i64>() {
        return Some(if value > 10_000_000_000 {
            value / 1_000
        } else {
            value
        });
    }
    // Copilot sometimes sends a date only; that means the start of that day, UTC.
    if !text.contains('T')
        && let Some(day) = parse_date_only(text)
    {
        return Some(day);
    }
    let (date, rest) = text.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !valid_date(year, month, day) {
        return None;
    }
    let (clock, offset) = if let Some(clock) = rest.strip_suffix('Z') {
        (clock, 0)
    } else if let Some(index) = rest.find(['+', '-']) {
        let clock = &rest[..index];
        let offset = parse_offset(&rest[index..])?;
        (clock, offset)
    } else {
        (rest, 0)
    };
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second_part = clock_parts.next().unwrap_or("0");
    let second: i64 = second_part.split('.').next()?.parse().ok()?;
    if clock_parts.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * ONE_DAY + hour * 3_600 + minute * 60 + second - i64::from(offset))
}

// ---------------------------------------------------------------- rendering

fn format_percent(value: f64) -> String {
    format!("{value:.0}")
}

fn format_number(value: f64) -> String {
    format_percent(value)
}

fn duration_label(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunwayConfig;

    fn worker(text: &str) -> Worker {
        text.parse().unwrap()
    }

    fn window(percent_left: f64, reset_in: i64, seconds: i64) -> Window {
        Window {
            name: "test".into(),
            percent_left,
            resets_at: Some(now_seconds() + reset_in),
            seconds,
            scope: Vec::new(),
            no_reset: false,
        }
    }

    #[test]
    fn pi_api_key_entries_use_the_key_without_exchange() {
        let key = json!({ "type": "api_key", "key": "never-print-this" });
        match hyper_login_from(&key) {
            Login::Hyper {
                access,
                refresh,
                expires_ms,
            } => {
                assert_eq!(access, "never-print-this");
                assert!(refresh.is_none());
                assert!(expires_ms > now_millis());
            }
            _ => panic!("expected a Hyper login"),
        }
        match codex_login_from(&key, "access").unwrap() {
            Login::Codex { token, .. } => assert_eq!(token, "never-print-this"),
            _ => panic!("expected a Codex login"),
        }
        match copilot_login_from(&key).unwrap() {
            Login::Copilot { token } => assert_eq!(token, "never-print-this"),
            _ => panic!("expected a Copilot login"),
        }
        // The OAuth shape still works.
        let oauth = json!({ "access": "access", "refresh": "refresh", "expires": now_millis() + 60_000 });
        match hyper_login_from(&oauth) {
            Login::Hyper {
                access, refresh, ..
            } => {
                assert_eq!(access, "access");
                assert_eq!(refresh.as_deref(), Some("refresh"));
            }
            _ => panic!("expected a Hyper login"),
        }
    }

    #[test]
    fn account_mapping_follows_the_table_and_overrides() {
        let none = BTreeMap::new();
        assert_eq!(
            account_for(&worker("claude:sonnet@high"), &none),
            Some(Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials
            })
        );
        assert_eq!(
            account_for(&worker("pi:openai-codex/gpt-5@high"), &none),
            Some(Account {
                name: AccountName::Codex,
                source: LoginSource::PiCodex
            })
        );
        assert_eq!(
            account_for(&worker("codex:gpt-5@high"), &none),
            Some(Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth
            })
        );
        assert_eq!(
            account_for(&worker("pi:github-copilot/gpt-4.1@high"), &none),
            Some(Account {
                name: AccountName::Copilot,
                source: LoginSource::PiCopilot
            })
        );
        assert_eq!(
            account_for(&worker("pi:hyper/sonnet@high"), &none),
            Some(Account {
                name: AccountName::Hyper,
                source: LoginSource::PiHyper
            })
        );
        assert_eq!(account_for(&worker("pi:gpt-good@high"), &none), None);
        assert_eq!(account_for(&worker("gemini:gemini-4@high"), &none), None);

        let mapping = BTreeMap::from([
            ("pi:openai-codex/".to_string(), "copilot".to_string()),
            ("claude:".to_string(), "none".to_string()),
        ]);
        assert_eq!(
            account_for(&worker("pi:openai-codex/gpt-5@high"), &mapping),
            Some(Account {
                name: AccountName::Copilot,
                source: LoginSource::PiCodex
            })
        );
        assert_eq!(account_for(&worker("claude:sonnet"), &mapping), None);
    }

    #[test]
    fn runway_reports_every_status() {
        assert_eq!(
            runway_for(&[window(0.0, 3_600, FIVE_HOURS)]).status,
            RunwayStatus::ExhaustedNow
        );
        assert_eq!(
            runway_for(&[window(100.0, 3_600, FIVE_HOURS)]).status,
            RunwayStatus::ThroughReset
        );
        let projected = runway_for(&[window(50.0, 4 * 3_600, FIVE_HOURS)]);
        assert_eq!(projected.status, RunwayStatus::ProjectedExhaustion);
        let left = projected.seconds_left;
        // Half used after one hour of five: a linear pace lasts about one more.
        assert!((3_500..=3_700).contains(&left), "{left}");
        let through = runway_for(&[window(10.0, 60, FIVE_HOURS)]);
        assert_eq!(through.status, RunwayStatus::ThroughReset);
        assert_eq!(runway_for(&[]).status, RunwayStatus::Unknown);
    }

    #[test]
    fn runway_uses_the_shortest_window() {
        let windows = [
            window(90.0, 60, SEVEN_DAYS),
            window(50.0, 4 * 3_600, FIVE_HOURS),
        ];
        // The 5-hour window's projection (~3h) wins over the weekly one's reset.
        assert_eq!(
            runway_for(&windows).status,
            RunwayStatus::ProjectedExhaustion
        );
    }

    #[test]
    fn hyper_uses_its_day_window_and_local_reset() {
        let now = now_seconds();
        let at = next_daily_reset(now, LocalTime { hour: 0, minute: 8 });
        assert!(at > now && at - now <= ONE_DAY);
        let windows = [
            Window {
                name: "daily".into(),
                percent_left: 91.2,
                resets_at: Some(at),
                seconds: ONE_DAY,
                scope: Vec::new(),
                no_reset: false,
            },
            window(50.0, 60, FIVE_HOURS),
        ];
        // The day window is longer than the 5-hour one, so the shorter window counts.
        assert_ne!(runway_for(&windows).status, RunwayStatus::Unknown);
        assert_ne!(runway_for(&windows[..1]).status, RunwayStatus::Unknown);
    }

    #[test]
    fn skip_reasons_name_the_size_and_required_runway() {
        let skipped = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            vec![window(50.0, 3 * 3_600, FIVE_HOURS)],
            None,
            None,
        );
        let reason = skipped
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .expect("2h of runway is not enough for a long task");
        assert!(reason.reason.contains("long"), "{reason:?}");
        assert!(reason.reason.contains("3h"), "{reason:?}");
        assert!(
            skipped
                .skip_reason(TaskSize::Small, RunwayConfig::default().small)
                .is_none()
        );
    }

    #[test]
    fn base_url_seam_only_accepts_loopback() {
        assert_eq!(
            loopback_base("http://127.0.0.1:8123").as_deref(),
            Some("http://127.0.0.1:8123")
        );
        assert_eq!(
            loopback_base("http://localhost:8123/").as_deref(),
            Some("http://localhost:8123")
        );
        for refused in [
            "https://api.anthropic.com",
            "http://127.0.0.1.evil:80",
            "http://localhost.evil:80",
            "http://10.0.0.1:80",
            "http://127.0.0.1",
            "ftp://127.0.0.1:80",
            "http://127.0.0.1:@evil.example",
            "http://127.0.0.1:80#fragment",
            "http://127.0.0.1:80?query",
            "http://127.0.0.1:80\\path",
            "http://127.0.0.1:80/extra",
            "HTTP://127.0.0.1:80",
            "http://[::1]:80",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:80:90",
        ] {
            assert_eq!(loopback_base(refused), None, "{refused}");
        }
        assert_eq!(
            base_url("DELEGATE_QUOTA_BASE_TEST_UNSET", CLAUDE_BASE),
            CLAUDE_BASE
        );
    }

    #[test]
    fn quota_lines_align_columns_and_date_far_times() {
        let now = now_seconds();
        let claude = AccountReport::from_windows(
            Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            },
            vec![window(13.0, 2 * 3_600, FIVE_HOURS)],
            None,
            None,
        );
        let copilot = AccountReport::from_windows(
            Account {
                name: AccountName::Copilot,
                source: LoginSource::PiCopilot,
            },
            vec![window(10.0, 40 * ONE_DAY, THIRTY_DAYS)],
            None,
            None,
        );
        let hyper = AccountReport::from_windows(
            Account {
                name: AccountName::Hyper,
                source: LoginSource::PiHyper,
            },
            vec![window(91.2, 3_600, ONE_DAY)],
            Some(228.0),
            Some(250.0),
        );
        let lines = text_lines(&[claude, copilot, hyper]);
        assert_eq!(lines[0].find("resets"), lines[1].find("resets"), "{lines:?}");
        assert_eq!(lines[0].find("runway:"), lines[1].find("runway:"), "{lines:?}");
        assert_eq!(lines[1].find("runway:"), lines[2].find("runway:"), "{lines:?}");
        assert_eq!(lines[0].find("13% left"), lines[1].find("10% left"));
        assert!(lines[2].contains("228/250 credits"));
        assert!(
            lines[1].contains(&format!("resets {}", format_clock(now + 40 * ONE_DAY))),
            "{}",
            lines[1]
        );
        let expected = format_clock_at(now + 40 * ONE_DAY, now, (0, true));
        assert!(expected.contains(' '), "a far reset carries its date: {expected}");
        assert_eq!(
            format_clock_at(1_793_491_200 + 7 * 3_600, 1_767_225_600, (0, true)),
            "Nov 1 07:00"
        );
        assert_eq!(
            format_clock_at(1_767_229_200, 1_767_225_600, (0, true)),
            "01:00"
        );
        assert_eq!(
            format_clock_at(1_793_491_200, 1_767_225_600, (0, false)),
            "Nov 1 00:00 UTC"
        );
    }

    #[test]
    fn rfc3339_round_trips() {
        let epoch = 1_767_225_600;
        assert_eq!(format_rfc3339(epoch), "2026-01-01T00:00:00Z");
        assert_eq!(parse_rfc3339("2026-01-01T00:00:00Z"), Some(epoch));
        assert_eq!(parse_rfc3339("2026-01-01T01:00:00+01:00"), Some(epoch));
        assert_eq!(parse_rfc3339("2026-01-01T00:00:00.500Z"), Some(epoch));
        assert_eq!(parse_rfc3339("1767225600"), Some(epoch));
        assert_eq!(parse_rfc3339("1767225600000"), Some(epoch));
        assert_eq!(parse_rfc3339("not a time"), None);
    }

    #[test]
    fn durations_parse_compactly_and_reject_bad_values() {
        assert_eq!(duration_label(Duration::from_mins(30)), "30m");
        assert_eq!(duration_label(Duration::from_hours(3)), "3h");
        assert_eq!(
            RunwayConfig::default().for_size(TaskSize::Long).as_secs(),
            10_800
        );
    }

    #[test]
    fn exhausted_longer_window_beats_a_healthy_short_one() {
        let windows = [
            window(100.0, 4 * 3_600, FIVE_HOURS),
            window(0.0, 86_400, SEVEN_DAYS),
        ];
        let runway = runway_for(&windows);
        assert_eq!(runway.status, RunwayStatus::ExhaustedNow);
    }

    #[test]
    fn through_reset_is_never_a_reason_to_skip() {
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            vec![window(100.0, 300, FIVE_HOURS)],
            None,
            None,
        );
        assert_eq!(report.runway.status, RunwayStatus::ThroughReset);
        assert!(
            report
                .skip_reason(TaskSize::Long, RunwayConfig::default().long)
                .is_none(),
            "a reset in five minutes is fine for any size"
        );
    }

    #[test]
    fn reports_keep_the_mapped_account_name() {
        let mapping = BTreeMap::from([("pi:openai-codex/".to_string(), "copilot".to_string())]);
        let account = account_for(&worker("pi:openai-codex/gpt-5"), &mapping).unwrap();
        assert_eq!(account.name, AccountName::Copilot);
        let report =
            AccountReport::from_windows(account, vec![window(90.0, 3_600, FIVE_HOURS)], None, None);
        assert_eq!(report.account, AccountName::Copilot);
        assert_eq!(report.json()["account"], "copilot");
    }

    #[test]
    fn each_harness_login_is_a_separate_account() {
        let workers = [worker("pi:openai-codex/gpt-5"), worker("codex:gpt-5")];
        let accounts = accounts_in_order(&workers, &BTreeMap::new());
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].name, accounts[1].name);
        assert_ne!(accounts[0].source, accounts[1].source);
    }

    #[test]
    fn unlimited_copilot_categories_are_ignored() {
        let payload = json!({
            "chat": {"unlimited": true, "entitlement": -1, "percent_remaining": 0},
            "premium_interactions": {"entitlement": 300, "percent_remaining": 90},
        });
        let windows = copilot_windows(payload.as_object().unwrap(), None);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].name, "premium interactions");
        assert!((windows[0].percent_left - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn copilot_date_only_resets_parse_and_exhaustion_is_known() {
        let day = parse_time(&json!("2026-11-01"));
        assert_eq!(day, Some(1_793_491_200));
        let exhausted = Window {
            name: "premium interactions".into(),
            percent_left: 0.0,
            resets_at: day,
            seconds: THIRTY_DAYS,
            scope: Vec::new(),
            no_reset: false,
        };
        assert_eq!(
            runway_for(std::slice::from_ref(&exhausted)).status,
            RunwayStatus::ExhaustedNow
        );
        let no_reset = Window {
            resets_at: None,
            ..exhausted
        };
        assert_eq!(
            runway_for(&[no_reset]).status,
            RunwayStatus::ExhaustedNow,
            "exhaustion is known even without a reset"
        );
    }

    #[test]
    fn review_codex_nonfuture_relative_resets_are_unreadable() {
        for delay in [0, -3_600] {
            let payload = json!({
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 10,
                        "limit_window_seconds": FIVE_HOURS,
                        "reset_at": 0,
                        "reset_after_seconds": delay,
                    },
                },
            });
            let windows = codex_windows(&payload).unwrap();
            assert_eq!(
                runway_for(&windows).status,
                RunwayStatus::Unknown,
                "nonfuture reset_after_seconds={delay} must not report healthy quota: {windows:?}"
            );
            assert_eq!(windows[0].resets_at, None);
        }
    }

    #[test]
    fn copilot_unusable_category_resets_fall_back_to_the_account_date() {
        let day = parse_time(&json!("2026-11-01"));
        for bad in [json!(0), json!(""), json!(null), json!(-1)] {
            let payload = json!({
                "quota_snapshots": {
                    "premium_interactions": {
                        "entitlement": 300,
                        "percent_remaining": 90,
                        "quota_reset_at": bad,
                    },
                },
                "quota_reset_date_utc": "2026-11-01T00:00:00Z",
            });
            let windows = copilot_windows_from(&payload).unwrap();
            assert_eq!(windows.len(), 1);
            assert_eq!(windows[0].resets_at, day, "category reset {bad}");
        }
    }

    #[test]
    fn copilot_all_unusable_resets_leave_the_window_unknown() {
        let payload = json!({
            "quota_snapshots": {
                "premium_interactions": {
                    "entitlement": 300,
                    "percent_remaining": 90,
                    "quota_reset_at": 0,
                },
            },
            "quota_reset_date_utc": "1970-01-01T00:00:00Z",
            "quota_reset_date": "",
        });
        let windows = copilot_windows_from(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].resets_at, None);
        assert_eq!(runway_for(&windows).status, RunwayStatus::Unknown);
    }

    #[test]
    fn hyper_exchange_and_credits_share_one_budget() {
        assert!(budget_left(Instant::now()).is_err());
        let left = budget_left(Instant::now() + REQUEST_TIMEOUT).unwrap();
        assert!(left <= REQUEST_TIMEOUT);
    }

    #[test]
    fn claude_buckets_must_be_objects() {
        let payload = json!({
            "five_hour": {"utilization": 0},
            "seven_day_oauth_apps": null,
            "seven_day": {"utilization": 100},
        });
        assert!(object_bucket(&payload, "seven_day_oauth_apps").is_none());
        assert_eq!(
            object_bucket(&payload, "seven_day")
                .and_then(|entry| number_field(entry, "utilization")),
            Some(100.0)
        );
        assert!(object_bucket(&json!({"bucket": "text"}), "bucket").is_none());
    }

    #[test]
    fn copilot_remaining_counts_are_read_and_zero_is_exhaustion() {
        let payload = json!({
            "premium_interactions": {"entitlement": 300, "remaining": 0},
            "chat": {"entitlement": 100, "remaining": 25},
        });
        let windows = copilot_windows(payload.as_object().unwrap(), None);
        assert_eq!(windows.len(), 2);
        let chat = windows.iter().find(|window| window.name == "chat").unwrap();
        assert!((chat.percent_left - 25.0).abs() < f64::EPSILON);
        let premium = windows
            .iter()
            .find(|window| window.name == "premium interactions")
            .unwrap();
        assert!((premium.percent_left - 0.0).abs() < f64::EPSILON);
        assert_eq!(
            runway_for(std::slice::from_ref(premium)).status,
            RunwayStatus::ExhaustedNow
        );
    }

    #[test]
    fn copilot_window_is_the_calendar_month_before_its_reset() {
        let reset = parse_time(&json!("2026-11-01")).unwrap();
        assert_eq!(copilot_window_seconds(reset), 31 * ONE_DAY);
        let january = parse_time(&json!("2027-01-01")).unwrap();
        assert_eq!(copilot_window_seconds(january), 31 * ONE_DAY);
    }

    #[test]
    fn account_override_is_a_longest_candidate_prefix() {
        let mapping = BTreeMap::from([
            ("pi:openai-codex/".to_string(), "copilot".to_string()),
            ("pi:openai-codex/gpt-".to_string(), "claude".to_string()),
        ]);
        let account = account_for(&worker("pi:openai-codex/gpt-5@high"), &mapping).unwrap();
        assert_eq!(account.name, AccountName::Claude);
        assert_eq!(account.source, LoginSource::PiCodex);

        let unmapped = BTreeMap::from([("pi:local/".to_string(), "hyper".to_string())]);
        let account = account_for(&worker("pi:local/model"), &unmapped).unwrap();
        assert_eq!(account.name, AccountName::Hyper);
    }

    #[test]
    fn unreadable_reset_is_unknown_unless_exhausted() {
        let mut unknown = window(90.0, 3_600, FIVE_HOURS);
        unknown.resets_at = None;
        assert_eq!(runway_for(&[unknown]).status, RunwayStatus::Unknown);
        let mut exhausted = window(0.0, 3_600, FIVE_HOURS);
        exhausted.resets_at = None;
        assert_eq!(runway_for(&[exhausted]).status, RunwayStatus::ExhaustedNow);
    }

    #[test]
    fn claude_scoped_limits_count_only_for_matching_models() {
        let payload = json!({
            "limits": [{
                "kind": "weekly_scoped",
                "percent": 100,
                "resets_at": now_seconds() + SEVEN_DAYS,
                "scope": {"model": {"id": "claude-sonnet-4-5", "display_name": "Sonnet"}},
            }],
        });
        let scoped = claude_scoped_windows(&payload, true);
        assert_eq!(scoped.len(), 1);
        assert!(scoped[0].applies_to("sonnet"));
        assert!(!scoped[0].applies_to("opus"));
        let mut windows = vec![
            window(100.0, 3_600, FIVE_HOURS),
            window(100.0, SEVEN_DAYS, SEVEN_DAYS),
        ];
        windows.extend(scoped);
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            },
            windows,
            None,
            None,
        );
        assert_eq!(
            report.status_for(&worker("claude:sonnet@high")),
            ReportStatus::Exhausted
        );
        assert_eq!(
            report.status_for(&worker("claude:opus@high")),
            ReportStatus::Ok
        );
        assert!(
            report
                .skip_reason_for(
                    &worker("claude:sonnet@high"),
                    TaskSize::Long,
                    RunwayConfig::default().long,
                )
                .is_some()
        );
        assert!(
            report
                .skip_reason_for(
                    &worker("claude:opus@high"),
                    TaskSize::Long,
                    RunwayConfig::default().long,
                )
                .is_none()
        );
    }

    #[test]
    fn claude_enterprise_monthly_cap_is_a_monthly_window() {
        let windows = claude_windows(&enterprise_payload(22_550.0, 50_000.0, false)).unwrap();
        assert_eq!(windows.len(), 1);
        let monthly = &windows[0];
        assert_eq!(monthly.name, "monthly");
        assert!((monthly.percent_left - 54.9).abs() < 1e-9);
        assert!(monthly.resets_at.is_none());
        assert_eq!(monthly.scope, Vec::<String>::new());
        assert_eq!(monthly.seconds, THIRTY_DAYS);
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            },
            windows,
            None,
            None,
        );
        assert_eq!(report.status, ReportStatus::Ok);
        assert_eq!(report.runway.status, RunwayStatus::Unknown);
        assert!(report.error.is_none());
    }

    #[test]
    fn claude_enterprise_monthly_cap_used_up_is_zero_left() {
        let windows = claude_windows(&enterprise_payload(50_000.0, 50_000.0, true)).unwrap();
        assert_eq!(windows.len(), 1);
        assert!(windows[0].percent_left.abs() < f64::EPSILON);
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Claude,
                source: LoginSource::ClaudeCredentials,
            },
            windows,
            None,
            None,
        );
        assert_eq!(report.status, ReportStatus::Exhausted);
    }

    #[test]
    fn claude_plan_windows_hide_enabled_extra_usage() {
        let now = now_seconds();
        let payload = json!({
            "five_hour": {"utilization": 30, "resets_at": now + FIVE_HOURS},
            "seven_day": {"utilization": 20, "resets_at": now + SEVEN_DAYS},
            "limits": [],
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 50_000.0,
                "used_credits": 45_000.0,
                "spend_limit_reached": false,
            },
            "spend": {
                "used": {"amount_minor": 45_000},
                "limit": {"amount_minor": 50_000},
            },
        });
        let windows = claude_windows(&payload).unwrap();
        assert_eq!(windows.len(), 2);
        assert!(
            windows.iter().all(|window| window.name != "monthly"),
            "{windows:?}"
        );
    }

    fn enterprise_payload(used: f64, limit: f64, reached: bool) -> Value {
        json!({
            "five_hour": null,
            "seven_day": null,
            "seven_day_oauth_apps": null,
            "limits": [],
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": limit,
                "used_credits": used,
                "spend_limit_reached": reached,
            },
            "spend": {
                "used": {"amount_minor": used},
                "limit": {"amount_minor": limit},
            },
        })
    }

    #[test]
    fn copilot_reads_classic_and_sdk_snapshot_shapes() {
        let payload = json!({
            "classic": {"entitlement": 100, "quota_remaining": 0},
            "sdk_percent": {"entitlementRequests": 300, "remainingPercentage": 50},
            "sdk_counts": {"entitlementRequests": 300, "usedRequests": 300},
            "unlimited": {"unlimited": true, "entitlement": 100, "quota_remaining": 0},
            "unlimited_sdk": {"entitlementRequests": -1, "remainingPercentage": 0},
        });
        let windows = copilot_windows(payload.as_object().unwrap(), None);
        assert_eq!(windows.len(), 3);
        let by_name = |name: &str| windows.iter().find(|window| window.name == name).unwrap();
        assert_eq!(by_name("classic").percent_left, 0.0);
        assert_eq!(by_name("sdk percent").percent_left, 50.0);
        assert_eq!(by_name("sdk counts").percent_left, 0.0);
        assert_eq!(
            runway_for(std::slice::from_ref(by_name("classic"))).status,
            RunwayStatus::ExhaustedNow
        );
    }

    #[test]
    fn explicit_worker_warning_names_the_account_and_reason() {
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            vec![window(50.0, 3 * 3_600, FIVE_HOURS)],
            None,
            None,
        );
        let reason = report
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .unwrap();
        let warning = reason.explicit_warning(&worker("codex:gpt-5"));
        assert_eq!(
            warning,
            format!(
                "warning: codex:gpt-5's codex account {}; running it because --worker was given",
                reason.reason
            )
        );
    }

    #[test]
    fn skip_summary_names_the_reset_and_any_early_exhaustion() {
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            vec![window(50.0, 3 * 3_600, FIVE_HOURS)],
            None,
            None,
        );
        let reason = report
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .unwrap();
        assert!(reason.summary.contains("resets"), "{reason:?}");
        assert!(reason.summary.contains("runs out at"), "{reason:?}");
        assert!(reason.reset.is_some());
        let exhausted = AccountReport::from_windows(
            Account {
                name: AccountName::Copilot,
                source: LoginSource::PiCopilot,
            },
            vec![window(0.0, 3_600, FIVE_HOURS)],
            None,
            None,
        );
        let reason = exhausted
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .unwrap();
        assert!(reason.summary.contains("until"), "{reason:?}");
    }

    #[test]
    fn quota_requests_never_follow_redirects() {
        assert_eq!(http_agent(REQUEST_TIMEOUT).config().max_redirects(), 0);
    }

    #[test]
    fn null_alternatives_fall_back_to_the_older_field() {
        // Copilot: a null UTC reset must not hide the date-only one.
        let payload = json!({
            "quota_reset_date_utc": null,
            "quota_reset_date": "2026-11-01",
            "quota_snapshots": {"premium_interactions": {
                "entitlement": 300, "percent_remaining": 0}},
        });
        let windows = copilot_windows_from(&payload).unwrap();
        assert_eq!(windows[0].resets_at, parse_date_only("2026-11-01"));

        // Codex: a null container or reset falls back to the next key.
        let now = now_seconds();
        let payload = json!({
            "rate_limit": null,
            "rateLimits": {"primary": {
                "usedPercent": 10.0,
                "reset_at": null,
                "resetsAt": now + 3_600,
                "windowDurationMins": 300}},
        });
        let windows = codex_windows(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].resets_at, Some(now + 3_600));
        assert_eq!(windows[0].seconds, FIVE_HOURS);
        assert_eq!(windows[0].percent_left, 90.0);
    }

    #[test]
    fn non_finite_usage_makes_the_account_unknown() {
        let payload = json!({
            "rate_limit": {"primary_window": {
                "used_percent": "NaN",
                "reset_at": now_seconds() + 3_600,
                "limit_window_seconds": FIVE_HOURS}},
        });
        let windows = codex_windows(&payload).unwrap();
        assert_eq!(runway_for(&windows).status, RunwayStatus::Unknown);

        for bad in ["inf", "-inf", "NaN"] {
            let payload = json!({
                "quota_reset_date": "2026-11-01",
                "quota_snapshots": {"premium_interactions": {
                    "entitlement": 300, "percent_remaining": bad}},
            });
            let windows = copilot_windows_from(&payload).unwrap();
            assert_eq!(runway_for(&windows).status, RunwayStatus::Unknown, "{bad}");
        }
        assert_eq!(
            checked_number_field(&json!({"balance": "NaN"}), "balance"),
            Err(())
        );

        // An unreadable usage number never becomes exhaustion, even beside one.
        let mut bad = window(50.0, 3_600, FIVE_HOURS);
        bad.percent_left = f64::INFINITY;
        let exhausted = [window(0.0, 3_600, FIVE_HOURS), bad];
        assert_eq!(runway_for(&exhausted).status, RunwayStatus::Unknown);
    }

    #[test]
    fn rfc3339_fields_are_validated_not_normalized() {
        for bad in [
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-02-30T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:61Z",
            "2026-01-01T00:00:00+24:00",
            "2026-13-01",
            "2026-02-30",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
        assert!(parse_rfc3339("2024-02-29T00:00:00Z").is_some());
        assert_eq!(parse_rfc3339("2025-02-29T00:00:00Z"), None);
        assert!(parse_rfc3339("2026-01-01T23:59:60Z").is_some());
        assert!(parse_rfc3339("2026-01-01T00:00:00+23:59").is_some());
        assert!(parse_date_only("2024-02-29").is_some());
        assert_eq!(parse_date_only("2026-02-29"), None);
    }

    #[test]
    fn the_blocking_window_supplies_the_reset_for_order_and_summary() {
        let now = now_seconds();
        let window_at = |percent_left, reset_in, seconds| Window {
            name: "test".into(),
            percent_left,
            resets_at: Some(now + reset_in),
            seconds,
            scope: Vec::new(),
            no_reset: false,
        };
        let exhausted = [
            window_at(50.0, 3_600, FIVE_HOURS),
            window_at(0.0, 12 * 3_600, SEVEN_DAYS),
        ];
        let runway = runway_for(&exhausted);
        assert_eq!(runway.status, RunwayStatus::ExhaustedNow);
        assert_eq!(runway.reset, Some(now + 12 * 3_600));
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            exhausted.to_vec(),
            None,
            None,
        );
        let reason = report
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .unwrap();
        assert_eq!(reason.reset, Some(now + 12 * 3_600));
        assert!(
            reason.summary.contains(&format_clock(now + 12 * 3_600)),
            "{reason:?}"
        );

        // A projected exhaustion also reports the window that limits it.
        let projected = [
            window_at(100.0, 3_600, FIVE_HOURS),
            window_at(10.0, 4 * 3_600, FIVE_HOURS),
        ];
        let runway = runway_for(&projected);
        assert_eq!(runway.status, RunwayStatus::ProjectedExhaustion);
        assert_eq!(runway.reset, Some(now + 4 * 3_600));
        let report = AccountReport::from_windows(
            Account {
                name: AccountName::Codex,
                source: LoginSource::CodexAuth,
            },
            projected.to_vec(),
            None,
            None,
        );
        let reason = report
            .skip_reason(TaskSize::Long, RunwayConfig::default().long)
            .unwrap();
        assert_eq!(reason.reset, Some(now + 4 * 3_600));
        assert!(
            reason.summary.contains(&format_clock(now + 4 * 3_600)),
            "{reason:?}"
        );
        assert!(reason.summary.contains("runs out at"), "{reason:?}");
    }
}
