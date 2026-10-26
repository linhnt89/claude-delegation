//! Best-effort translation of worker streams into append-only, attempt-local steps.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{Value, json};

use crate::config::Harness;

// Bound parsing memory, but always drain and preserve the entire raw pipe.
const MAX_LINE: usize = 256 * 1024;
const LOG_PREVIEW: usize = 4096;

/// Disable a failed output, reporting its first error only. Observability must
/// never interrupt the worker or stop draining its stdout.
#[derive(Debug)]
struct Output {
    file: Option<File>,
    name: &'static str,
    error: Option<String>,
}

impl Output {
    fn open(path: &Path, name: &'static str, append: bool) -> Self {
        let opened = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path);
        let (file, error) = match opened {
            Ok(file) => (Some(file), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self { file, name, error }
    }

    fn write(&mut self, bytes: &[u8]) {
        if let Some(file) = &mut self.file
            && let Err(error) = file.write_all(bytes).and_then(|()| file.flush())
        {
            self.error = Some(error.to_string());
            self.file = None;
        }
    }

    fn report(&mut self, log: &mut Self) {
        if let Some(error) = self.error.take() {
            log.write(
                format!(
                    "warning: writing {}: {error}; check disk space and permissions\n",
                    self.name
                )
                .as_bytes(),
            );
        }
    }
}

/// Clear stale steps even when preflight fails before the first worker attempt.
pub(crate) fn reset(dir: &Path) {
    let mut log = Output::open(&dir.join("run.log"), "run.log", true);
    for name in ["events.jsonl", "stream.jsonl"] {
        Output::open(&dir.join(name), name, false).report(&mut log);
    }
}

#[derive(Debug)]
pub(crate) struct Stream {
    raw: Output,
    events: Output,
    adapter: Adapter,
}

impl Stream {
    pub(crate) fn new(dir: &Path, harness: Harness, cwd: &Path) -> Self {
        Self {
            raw: Output::open(&dir.join("stream.jsonl"), "stream.jsonl", false),
            events: Output::open(&dir.join("events.jsonl"), "events.jsonl", false),
            adapter: Adapter::new(harness, cwd),
        }
    }

    #[cfg(test)]
    pub(crate) fn drain(&mut self, pipe: impl Read, log: &Path) -> Result<()> {
        self.drain_until(pipe, log, || Ok(false))
    }

    pub(crate) fn drain_until(
        &mut self,
        mut pipe: impl Read,
        log: &Path,
        mut stopped: impl FnMut() -> Result<bool>,
    ) -> Result<()> {
        // Harness call IDs may restart on a resumed turn; step IDs do not.
        self.adapter.calls.clear();
        let mut log = Output::open(log, "run.log", true);
        self.raw.report(&mut log);
        self.events.report(&mut log);
        let mut chunk = [0; 8192];
        let mut line = Vec::new();
        let mut oversized = false;
        loop {
            let count = if stopped()? {
                0
            } else {
                match pipe.read(&mut chunk) {
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::Interrupted
                                | std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    other => other?,
                }
            };
            if count == 0 {
                // An unterminated record may be truncated: never interpret it.
                if !oversized {
                    log.write(&line);
                }
                return Ok(());
            }
            self.raw.write(&chunk[..count]);
            self.raw.report(&mut log);
            for part in chunk[..count].split_inclusive(|b| *b == b'\n') {
                if !oversized && line.len() + part.len() > MAX_LINE {
                    // A JSON-shaped oversized record belongs only in the raw
                    // stream. Plain output gets at most a small preview.
                    if line.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{') {
                        log.write(&line[..line.len().min(LOG_PREVIEW)]);
                        log.write(b" [oversized line truncated]\n");
                    }
                    oversized = true;
                    line.clear();
                } else if !oversized {
                    line.extend_from_slice(part);
                }
                if part.last() == Some(&b'\n') {
                    if !oversized {
                        if let Ok(value) = serde_json::from_slice::<Value>(&line)
                            && value.is_object()
                        {
                            for event in self.adapter.feed(&value) {
                                let mut bytes = event.to_string().into_bytes();
                                bytes.push(b'\n');
                                self.events.write(&bytes);
                                self.events.report(&mut log);
                            }
                        } else {
                            log.write(&line);
                        }
                    }
                    line.clear();
                    oversized = false;
                }
            }
        }
    }
}

#[derive(Debug)]
struct Adapter {
    harness: Harness,
    cwd: String,
    source_cwd: Option<String>,
    // All calls are deduplicated; only run steps receive result patches.
    calls: HashMap<String, Option<u64>>,
    next: u64,
}

impl Adapter {
    fn new(harness: Harness, cwd: &Path) -> Self {
        Self {
            harness,
            cwd: cwd.to_string_lossy().into_owned(),
            source_cwd: None,
            calls: HashMap::new(),
            next: 0,
        }
    }

    fn step(&mut self, kind: &str, target: &str, result: Option<&str>) -> Value {
        self.next += 1;
        json!({"id": self.next, "at": timestamp(), "kind": kind, "target": target, "result": result})
    }

    fn path(&self, path: &str) -> String {
        for root in [Some(self.cwd.as_str()), self.source_cwd.as_deref()]
            .into_iter()
            .flatten()
        {
            if let Ok(relative) = Path::new(path).strip_prefix(root) {
                return relative.to_string_lossy().into_owned();
            }
        }
        path.to_owned()
    }

    fn tool(&mut self, id: &str, name: &str, input: &Value) -> Option<Value> {
        if id.is_empty() || self.calls.contains_key(id) {
            return None;
        }
        let (kind, keys): (&str, &[&str]) = match name.to_ascii_lowercase().as_str() {
            "read" | "read_file" => ("read", &["file_path", "path", "filePath"]),
            "edit" | "multiedit" | "write" | "write_file" | "notebookedit" => {
                ("edit", &["file_path", "path", "filePath", "notebook_path"])
            }
            "grep" | "glob" | "find" | "search" | "code_search" | "docs_search" => {
                ("read", &["pattern", "query", "path"])
            }
            "ls" | "list" | "list_directory" | "list_files" => ("read", &["path", "directory"]),
            "webfetch" | "web_fetch" | "fetch" => ("read", &["url"]),
            "websearch" | "web_search" => ("read", &["query", "search_term"]),
            "bash" | "shell" | "command_execution" => ("run", &["command"]),
            _ => ("run", &[]),
        };
        if name.is_empty() || !input.is_object() {
            return None;
        }
        let argument = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| input[*key].as_str().filter(|s| !s.is_empty()))
        };
        let target = if keys.is_empty() {
            let main = argument(&[
                "command",
                "pattern",
                "query",
                "file_path",
                "path",
                "filePath",
                "url",
                "name",
                "prompt",
            ])
            .or_else(|| input.as_object()?.values().find_map(Value::as_str));
            main.map_or_else(|| name.to_owned(), |arg| format!("{name} {arg}"))
        } else {
            let target = argument(keys)?;
            if name == "command_execution" {
                command_target(target)
            } else if kind == "run" {
                target.to_owned()
            } else {
                self.path(target)
            }
        };
        let step = self.step(kind, &target, None);
        self.calls
            .insert(id.to_owned(), (kind == "run").then_some(self.next));
        Some(step)
    }

    fn complete(&self, call: &str, failed: bool) -> Option<Value> {
        self.calls
            .get(call)
            .copied()
            .flatten()
            .map(|id| json!({"id": id, "result": if failed { "failed" } else { "ok" }}))
    }

    fn content_step(&mut self, block: &Value) -> Option<Value> {
        let (kind, text) = match block["type"].as_str() {
            Some("text") => ("message", block["text"].as_str()),
            Some("thinking") => ("think", block["thinking"].as_str()),
            _ => return None,
        };
        text.filter(|s| !s.trim().is_empty())
            .map(|text| self.step(kind, text, None))
    }

    fn content(&mut self, content: &Value) -> Vec<Value> {
        content
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| self.content_step(block))
            .collect()
    }

    fn feed(&mut self, value: &Value) -> Vec<Value> {
        match self.harness {
            Harness::Claude => self.claude(value),
            Harness::Codex => self.codex(value),
            Harness::Opencode => self.opencode(value),
            Harness::Pi => self.pi(value),
            Harness::Gemini => vec![],
        }
    }

    fn claude(&mut self, value: &Value) -> Vec<Value> {
        match value["type"].as_str() {
            Some("system") if value["subtype"] == "init" => {
                self.source_cwd = value["cwd"].as_str().map(str::to_owned);
                vec![]
            }
            Some("assistant") => {
                let mut steps = Vec::new();
                for block in value["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_use" {
                        if let (Some(id), Some(name)) =
                            (block["id"].as_str(), block["name"].as_str())
                            && let Some(step) = self.tool(id, name, &block["input"])
                        {
                            steps.push(step);
                        }
                    } else if let Some(step) = self.content_step(block) {
                        steps.push(step);
                    }
                }
                steps
            }
            Some("user") => value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_result")
                .filter_map(|block| {
                    self.complete(
                        block["tool_use_id"].as_str()?,
                        block["is_error"].as_bool().unwrap_or(false),
                    )
                })
                .collect(),
            Some("result") if value["is_error"] == true => value["result"]
                .as_str()
                .map(|text| vec![self.step("error", text, None)])
                .unwrap_or_default(),
            _ => vec![],
        }
    }

    fn codex(&mut self, value: &Value) -> Vec<Value> {
        let event = value["type"].as_str().unwrap_or("");
        let item = &value["item"];
        let mut steps = Vec::new();
        if !matches!(event, "item.started" | "item.completed") {
            if event == "error"
                && let Some(text) = value["message"].as_str()
            {
                steps.push(self.step("error", text, None));
            }
            return steps;
        }
        let Some(id) = item["id"].as_str() else {
            return steps;
        };
        match item["type"].as_str() {
            Some("command_execution") => {
                if let Some(step) = self.tool(id, "command_execution", item) {
                    steps.push(step);
                }
                if event == "item.completed"
                    && let Some(code) = item["exit_code"].as_i64()
                    && let Some(result) = self.complete(id, code != 0)
                {
                    steps.push(result);
                }
            }
            Some("file_change") if !self.calls.contains_key(id) => {
                let paths: Vec<_> = item["changes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|change| change["path"].as_str())
                    .map(|path| self.path(path))
                    .collect();
                if !paths.is_empty() {
                    steps.push(self.step("edit", &paths.join(", "), None));
                    self.calls.insert(id.to_owned(), None);
                }
            }
            Some("agent_message" | "reasoning") if event == "item.completed" => {
                if let Some(text) = item["text"].as_str().filter(|s| !s.trim().is_empty()) {
                    steps.push(self.step(
                        if item["type"] == "reasoning" {
                            "think"
                        } else {
                            "message"
                        },
                        text,
                        None,
                    ));
                }
            }
            _ => {}
        }
        steps
    }

    fn opencode(&mut self, value: &Value) -> Vec<Value> {
        let part = &value["part"];
        match value["type"].as_str() {
            Some("text" | "reasoning") => part["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|text| {
                    vec![self.step(
                        if value["type"] == "reasoning" {
                            "think"
                        } else {
                            "message"
                        },
                        text,
                        None,
                    )]
                })
                .unwrap_or_default(),
            Some("tool_use") => {
                let Some(id) = part["callID"].as_str().or_else(|| part["id"].as_str()) else {
                    return vec![];
                };
                let Some(name) = part["tool"].as_str() else {
                    return vec![];
                };
                let state = &part["state"];
                let mut steps: Vec<_> = self.tool(id, name, &state["input"]).into_iter().collect();
                let code = state["metadata"]["exit"]
                    .as_i64()
                    .or_else(|| state["metadata"]["metadata"]["exit"].as_i64());
                if let Some(failed) = code
                    .map(|code| code != 0)
                    .or_else(|| (state["status"] == "error").then_some(true))
                    && let Some(result) = self.complete(id, failed)
                {
                    steps.push(result);
                }
                steps
            }
            Some("error") => value["error"]["data"]["message"]
                .as_str()
                .map(|text| vec![self.step("error", text, None)])
                .unwrap_or_default(),
            _ => vec![],
        }
    }

    fn pi(&mut self, value: &Value) -> Vec<Value> {
        match value["type"].as_str() {
            Some("session") => {
                self.source_cwd = value["cwd"].as_str().map(str::to_owned);
                vec![]
            }
            Some("tool_execution_start") => {
                let (Some(id), Some(name)) =
                    (value["toolCallId"].as_str(), value["toolName"].as_str())
                else {
                    return vec![];
                };
                self.tool(id, name, &value["args"]).into_iter().collect()
            }
            Some("tool_execution_end") => {
                let code = value["result"]["structuredContent"]["exit_code"].as_i64();
                value["toolCallId"]
                    .as_str()
                    .and_then(|id| {
                        self.complete(
                            id,
                            code.map_or_else(|| value["isError"] == true, |c| c != 0),
                        )
                    })
                    .into_iter()
                    .collect()
            }
            Some("message_end") if value["message"]["role"] == "assistant" => {
                self.content(&value["message"]["content"])
            }
            _ => vec![],
        }
    }
}

/// Decode only the single argument in Codex's login-shell wrapper. Never
/// execute or expand a command to obtain its display.
fn command_target(command: &str) -> String {
    let decode = || {
        let (shell, argument) = command.split_once(" -lc ")?;
        if !matches!(shell.rsplit('/').next(), Some("bash" | "sh" | "zsh")) {
            return None;
        }
        let mut quote = None;
        let mut chars = argument.chars();
        let mut target = String::new();
        while let Some(ch) = chars.next() {
            match (quote, ch) {
                (None, '\'' | '"') => quote = Some(ch),
                (Some(q), ch) if ch == q => quote = None,
                (None, ch) if ch.is_whitespace() => return None,
                (None, '\\') => target.push(chars.next()?),
                (Some('"'), '\\') => {
                    let next = chars.next()?;
                    if !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                        target.push('\\');
                    }
                    if next != '\n' {
                        target.push(next);
                    }
                }
                _ => target.push(ch),
            }
        }
        quote.is_none().then_some(target)
    };
    decode().unwrap_or_else(|| command.to_owned())
}

/// Fold append-only result patches, ignoring incomplete/corrupt records during a write.
pub(crate) fn collect(path: &Path) -> Vec<Value> {
    let Ok(bytes) = std::fs::read(path) else {
        return vec![];
    };
    let mut steps = Vec::<Value>::new();
    let mut indices = HashMap::<u64, usize>::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.last() != Some(&b'\n') {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let Some(id) = value["id"].as_u64() else {
            continue;
        };
        if let Some(&index) = indices.get(&id) {
            if let Some(step) = steps.get_mut(index) {
                step["result"] = value["result"].clone();
            }
        } else if value["kind"].is_string()
            && value["at"].is_string()
            && value["target"].is_string()
        {
            indices.insert(id, steps.len());
            steps.push(value);
        }
    }
    steps
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = i64::try_from(seconds / 86400).unwrap_or(i64::MAX / 2) + 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDINGS: &[(Harness, &str, &str)] = &[
        (
            Harness::Claude,
            include_str!("../../evals/lib/streams/claude.jsonl"),
            include_str!("../../evals/lib/streams/claude.steps"),
        ),
        (
            Harness::Claude,
            include_str!("../../evals/lib/streams/claude-fail.jsonl"),
            include_str!("../../evals/lib/streams/claude-fail.steps"),
        ),
        (
            Harness::Codex,
            include_str!("../../evals/lib/streams/codex.jsonl"),
            include_str!("../../evals/lib/streams/codex.steps"),
        ),
        (
            Harness::Codex,
            include_str!("../../evals/lib/streams/codex-fail.jsonl"),
            include_str!("../../evals/lib/streams/codex-fail.steps"),
        ),
        (
            Harness::Opencode,
            include_str!("../../evals/lib/streams/opencode.jsonl"),
            include_str!("../../evals/lib/streams/opencode.steps"),
        ),
        (
            Harness::Opencode,
            include_str!("../../evals/lib/streams/opencode-fail.jsonl"),
            include_str!("../../evals/lib/streams/opencode-fail.steps"),
        ),
        (
            Harness::Pi,
            include_str!("../../evals/lib/streams/pi.jsonl"),
            include_str!("../../evals/lib/streams/pi.steps"),
        ),
        (
            Harness::Pi,
            include_str!("../../evals/lib/streams/pi-fail.jsonl"),
            include_str!("../../evals/lib/streams/pi-fail.steps"),
        ),
    ];

    fn temp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("delegate-events-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_recording_matches_independently_authored_steps() {
        for &(harness, raw, expected) in RECORDINGS {
            let dir = temp();
            let mut stream = Stream::new(&dir, harness, Path::new("/work/shop"));
            stream.drain(raw.as_bytes(), &dir.join("run.log")).unwrap();
            let steps = collect(&dir.join("events.jsonl"));
            let expected: Vec<_> = expected
                .lines()
                .map(|line| line.split('\t').collect::<Vec<_>>())
                .collect();
            assert_eq!(steps.len(), expected.len(), "{harness:?}: {steps:?}");
            for (index, (step, expected)) in steps.iter().zip(expected).enumerate() {
                assert_eq!(step["id"], index + 1);
                assert_eq!(step["kind"], expected[1], "{harness:?}");
                assert!(
                    step["target"].as_str().unwrap().contains(expected[2]),
                    "{harness:?}: {step:?}"
                );
                assert_eq!(
                    step["result"].as_str().unwrap_or(""),
                    *expected.get(3).unwrap_or(&""),
                    "{harness:?}: {step:?}"
                );
                if matches!(expected[1], "read" | "edit") {
                    assert!(step["result"].is_null(), "{harness:?}: {step:?}");
                }
                if harness == Harness::Codex && expected[1] == "run" {
                    assert!(
                        !step["target"]
                            .as_str()
                            .unwrap()
                            .starts_with("/usr/bin/bash -lc")
                    );
                }
                assert!(step["at"].as_str().unwrap().ends_with('Z'));
            }
            assert_eq!(
                std::fs::read(dir.join("stream.jsonl")).unwrap(),
                raw.as_bytes()
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn same_command_interleaved_results_use_call_ids_for_every_harness() {
        let cases = [
            (
                Harness::Claude,
                vec![
                    json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"test"}}]}}),
                    json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"b","name":"Bash","input":{"command":"test"}}]}}),
                    json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b","is_error":true}]}}),
                    json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","is_error":false}]}}),
                ],
            ),
            (
                Harness::Codex,
                vec![
                    json!({"type":"item.started","item":{"id":"a","type":"command_execution","command":"test"}}),
                    json!({"type":"item.started","item":{"id":"b","type":"command_execution","command":"test"}}),
                    json!({"type":"item.completed","item":{"id":"b","type":"command_execution","command":"test","exit_code":1}}),
                    json!({"type":"item.completed","item":{"id":"a","type":"command_execution","command":"test","exit_code":0}}),
                ],
            ),
            (
                Harness::Opencode,
                vec![
                    json!({"type":"tool_use","part":{"callID":"a","tool":"bash","state":{"status":"running","input":{"command":"test"}}}}),
                    json!({"type":"tool_use","part":{"callID":"b","tool":"bash","state":{"status":"running","input":{"command":"test"}}}}),
                    json!({"type":"tool_use","part":{"callID":"b","tool":"bash","state":{"status":"completed","input":{"command":"test"},"metadata":{"exit":1}}}}),
                    json!({"type":"tool_use","part":{"callID":"a","tool":"bash","state":{"status":"completed","input":{"command":"test"},"metadata":{"exit":0}}}}),
                ],
            ),
            (
                Harness::Pi,
                vec![
                    json!({"type":"tool_execution_start","toolCallId":"a","toolName":"bash","args":{"command":"test"}}),
                    json!({"type":"tool_execution_start","toolCallId":"b","toolName":"bash","args":{"command":"test"}}),
                    json!({"type":"tool_execution_end","toolCallId":"b","isError":true}),
                    json!({"type":"tool_execution_end","toolCallId":"a","isError":false}),
                ],
            ),
        ];
        for (harness, lines) in cases {
            let mut adapter = Adapter::new(harness, Path::new("/work"));
            let events: Vec<_> = lines.iter().flat_map(|line| adapter.feed(line)).collect();
            assert_eq!(events.len(), 4, "{harness:?}: {events:?}");
            assert_eq!(events[0]["id"], 1);
            assert_eq!(events[1]["id"], 2);
            assert_eq!(events[2], json!({"id":2,"result":"failed"}));
            assert_eq!(events[3], json!({"id":1,"result":"ok"}));
        }
    }

    #[test]
    fn broken_input_is_preserved_and_does_not_prevent_later_steps() {
        let dir = temp();
        let mut bytes = b"plain output\n{broken\n\xff\xfe\n".to_vec();
        bytes.extend_from_slice(b"{\"type\":\"message\",\"text\":\"");
        bytes.extend(vec![b'x'; 1_000_000]);
        bytes.extend_from_slice(b"\"}\n{\"type\":\"unknown\"}\n");
        let good = b"{\"type\":\"tool_execution_start\",\"toolCallId\":\"a\",\"toolName\":\"read\",\"args\":{\"path\":\"file\"}}\n";
        bytes.extend_from_slice(good);
        // Even a valid object without its newline is considered truncated.
        bytes.extend_from_slice(&good[..good.len() - 1]);
        let mut stream = Stream::new(&dir, Harness::Pi, Path::new("/work"));
        stream
            .drain(bytes.as_slice(), &dir.join("run.log"))
            .unwrap();
        assert_eq!(std::fs::read(dir.join("stream.jsonl")).unwrap(), bytes);
        let steps = collect(&dir.join("events.jsonl"));
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0]["target"], "file");
        assert!(
            std::fs::read(dir.join("run.log"))
                .unwrap()
                .starts_with(b"plain output\n{broken\n")
        );
        drop(stream);
        let _fresh = Stream::new(&dir, Harness::Gemini, Path::new("/work"));
        assert_eq!(collect(&dir.join("events.jsonl")), Vec::<Value>::new());
        assert_eq!(
            std::fs::read(dir.join("stream.jsonl")).unwrap(),
            Vec::<u8>::new()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_lines_have_bounded_plain_previews_and_no_json_preview() {
        for prefix in ["x", "{", "  {"] {
            for newline in ["", "\n"] {
                let dir = temp();
                let bytes = format!("{prefix}{}{newline}", "x".repeat(MAX_LINE * 4));
                let mut stream = Stream::new(&dir, Harness::Pi, Path::new("/work"));
                stream
                    .drain(bytes.as_bytes(), &dir.join("run.log"))
                    .unwrap();
                let log = std::fs::read(dir.join("run.log")).unwrap();
                if prefix.contains('{') {
                    assert_eq!(log, Vec::<u8>::new());
                } else {
                    assert!(log.len() <= LOG_PREVIEW + 100);
                    assert!(log.starts_with(b"xxx"));
                }
                assert_eq!(
                    std::fs::read(dir.join("stream.jsonl")).unwrap(),
                    bytes.as_bytes()
                );
                assert_eq!(collect(&dir.join("events.jsonl")), Vec::<Value>::new());
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }

    #[test]
    fn resumed_turn_reuses_call_ids_but_not_step_ids() {
        let dir = temp();
        let mut stream = Stream::new(&dir, Harness::Codex, Path::new("/work"));
        let line = b"{\"type\":\"item.started\",\"item\":{\"id\":\"item_0\",\"type\":\"command_execution\",\"command\":\"test\"}}\n";
        stream.drain(line.as_slice(), &dir.join("run.log")).unwrap();
        stream.drain(line.as_slice(), &dir.join("run.log")).unwrap();
        let steps = collect(&dir.join("events.jsonl"));
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["id"], 1);
        assert_eq!(steps[1]["id"], 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn steps_drive_status_activity_and_full_or_recent_views() {
        use crate::status::{self, Depth, Operation};
        use crate::task::{Ctx, Task};

        let root = temp();
        std::fs::create_dir_all(root.join("tasks/t001/worktree/.task")).unwrap();
        let ctx = Ctx {
            root: root.clone(),
            dhome: root.join("tasks"),
            gitdir: root.join(".git"),
        };
        let task = Task::load(&ctx, "t001").unwrap();
        std::fs::write(&task.spec, "# Task t001\n").unwrap();
        let mut operation = Operation::acquire(&task, "run").unwrap();
        operation
            .start("implement", Some("pi:test"), "fast", "1/1")
            .unwrap();
        let mut live = status::read_live(&task).unwrap().unwrap();
        drop(operation);
        live.started_at = "2000-01-01T00:00:00Z".into();
        std::fs::write(
            task.dir.join("status.json"),
            serde_json::to_vec(&live).unwrap(),
        )
        .unwrap();
        assert!(status::collect(&ctx, Some("t001"), Depth::Phase).unwrap()[0].stalled);
        let mut stream = Stream::new(&task.dir, Harness::Pi, &task.wt);
        for index in 0..7 {
            let event = json!({"type":"tool_execution_start","toolCallId":index.to_string(),"toolName":"read","args":{"path":format!("file{index}")}});
            stream
                .drain(format!("{event}\n").as_bytes(), &task.dir.join("run.log"))
                .unwrap();
        }
        let views = status::collect(&ctx, Some("t001"), Depth::Phase).unwrap();
        assert!(!views[0].stalled);
        assert!(views[0].steps_available);
        assert_eq!(views[0].steps.len(), 7);
        assert_eq!(
            views[0].last_activity_at.as_deref(),
            views[0].steps.last().unwrap()["at"].as_str()
        );
        let recent = status::collect(&ctx, None, Depth::Phase).unwrap();
        assert_eq!(recent[0].steps.len(), 5);
        assert_eq!(recent[0].steps[0]["id"], 3);
        let rows = status::render_rows(&recent, false);
        assert!(rows.contains("t001"));
        assert!(
            rows.lines()
                .any(|line| line.trim_start().starts_with("└ read  file6"))
        );
        assert!(status::render_detail(&views[0], false).contains("file0"));
        assert!(!status::render_detail(&views[0], false).contains("aren't available"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn logging_failures_never_stop_draining_and_are_reported_once() {
        use std::os::unix::fs::symlink;

        for name in ["stream.jsonl", "events.jsonl", "run.log"] {
            for fail_open in [false, true] {
                let dir = temp();
                let path = dir.join(name);
                if fail_open {
                    std::fs::create_dir(&path).unwrap();
                } else {
                    symlink("/dev/full", &path).unwrap();
                }
                let mut stream = Stream::new(&dir, Harness::Pi, &dir);
                let bytes = b"plain output\n{\"type\":\"tool_execution_start\",\"toolCallId\":\"a\",\"toolName\":\"bash\",\"args\":{\"command\":\"test\"}}\n{\"type\":\"tool_execution_end\",\"toolCallId\":\"a\",\"isError\":false}\n";
                for _ in 0..2 {
                    let mut pipe = std::io::Cursor::new(bytes);
                    stream.drain(&mut pipe, &dir.join("run.log")).unwrap();
                    assert_eq!(pipe.position(), bytes.len() as u64);
                }
                if name != "run.log" {
                    let log = std::fs::read_to_string(dir.join("run.log")).unwrap();
                    assert_eq!(
                        log.matches(&format!("warning: writing {name}:")).count(),
                        1,
                        "{log}"
                    );
                    assert_eq!(log.matches("plain output").count(), 2);
                }
                if name != "events.jsonl" {
                    let steps = collect(&dir.join("events.jsonl"));
                    assert_eq!(steps.len(), 2);
                    assert!(steps.iter().all(|step| step["result"] == "ok"));
                }
                if name != "stream.jsonl" {
                    assert_eq!(
                        std::fs::read(dir.join("stream.jsonl")).unwrap(),
                        bytes.repeat(2)
                    );
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }

    #[test]
    fn tool_classification_includes_search_multiedit_and_unknown_tools() {
        let mut adapter = Adapter::new(Harness::Claude, Path::new("/work"));
        for (index, (name, input, kind, target)) in [
            (
                "Grep",
                json!({"pattern":"needle","path":"/work/src"}),
                "read",
                "needle",
            ),
            ("Glob", json!({"pattern":"**/*.rs"}), "read", "**/*.rs"),
            ("LS", json!({"path":"/work/src"}), "read", "src"),
            ("find", json!({"path":"/work/src"}), "read", "src"),
            (
                "web_fetch",
                json!({"url":"https://example.test"}),
                "read",
                "https://example.test",
            ),
            (
                "WebFetch",
                json!({"url":"https://example.test"}),
                "read",
                "https://example.test",
            ),
            ("WebSearch", json!({"query":"rust"}), "read", "rust"),
            (
                "MultiEdit",
                json!({"file_path":"/work/src/main.rs","edits":[]}),
                "edit",
                "src/main.rs",
            ),
            (
                "Deploy",
                json!({"command":"preview"}),
                "run",
                "Deploy preview",
            ),
            (
                "Custom",
                json!({"topic":"something"}),
                "run",
                "Custom something",
            ),
            ("Custom", json!({}), "run", "Custom"),
        ]
        .into_iter()
        .enumerate()
        {
            let id = index.to_string();
            let step = adapter.tool(&id, name, &input).unwrap();
            assert_eq!(step["kind"], kind);
            assert_eq!(step["target"], target);
            assert!(step["result"].is_null());
            assert!(adapter.tool(&id, name, &input).is_none());
            assert_eq!(
                adapter.complete(&id, false),
                (kind == "run").then(|| json!({"id":index + 1,"result":"ok"}))
            );
        }
    }

    #[test]
    fn codex_command_targets_remove_only_the_shell_wrapper() {
        for (command, expected) in [
            (
                "/usr/bin/bash -lc 'python3 -m unittest -q'",
                "python3 -m unittest -q",
            ),
            (
                "/usr/bin/bash -lc \"pwd && rg --files -g '*pricing*'\"",
                "pwd && rg --files -g '*pricing*'",
            ),
            ("bash -lc 'echo '\\''quoted'\\'''", "echo 'quoted'"),
            ("bash -lc \"echo \\\"quoted\\\"\"", "echo \"quoted\""),
            ("bash -lc 'echo \"$HOME\"'", "echo \"$HOME\""),
            ("bash -lc \"printf '\\n'\"", "printf '\\n'"),
            ("bash -lc 'truncated", "bash -lc 'truncated"),
            ("bash -lc 'one' 'two'", "bash -lc 'one' 'two'"),
            ("other -lc 'command'", "other -lc 'command'"),
            ("echo plain", "echo plain"),
        ] {
            assert_eq!(command_target(command), expected);
        }
    }

    /// Only JSON objects are events; scalars and arrays remain plain output.
    #[test]
    fn plain_lines_that_happen_to_parse_as_json_scalars_still_reach_run_log() {
        let dir = temp();
        let mut stream = Stream::new(&dir, Harness::Gemini, &dir);
        let bytes = b"42\ntrue\n\"done\"\nnull\n[]\n{}\n";
        stream
            .drain(bytes.as_slice(), &dir.join("run.log"))
            .unwrap();
        assert_eq!(
            std::fs::read(dir.join("run.log")).unwrap(),
            &bytes[..bytes.len() - 3]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_result_patch_does_not_hide_existing_steps() {
        let dir = temp();
        let path = dir.join("events.jsonl");
        std::fs::write(&path, "{\"id\":1,\"at\":\"2026-10-25T00:00:00Z\",\"kind\":\"run\",\"target\":\"test\",\"result\":null}\n{\"id\":1,\"result\":\"ok\"}\n{\"id\":1,").unwrap();
        assert_eq!(collect(&path)[0]["result"], "ok");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
