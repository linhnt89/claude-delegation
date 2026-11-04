//! The Herdr pane's lead conversation, captured once at `delegate new` (decision 038).
//!
//! Capture is observation only: it never fails `new`, never records the identity value in
//! the note or a log, and never guesses a pane, checkout, focus or model name instead.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::task::{Origin, RefKind};

/// One bounded `pane.get` exchange: connect, request and the first response line.
const DEADLINE: Duration = Duration::from_secs(3);
/// Longest accepted response line, including its newline.
const MAX_LINE: usize = 2 * 1024 * 1024;
/// Longest accepted identity string, in characters.
const MAX_FIELD_CHARS: usize = 4096;

/// The lead-side facts capture depends on, snapshotted once so one launch sees one view.
#[derive(Debug, Default)]
pub(crate) struct Env {
    pub(crate) sandboxed: bool,
    pub(crate) socket: Option<PathBuf>,
    pub(crate) pane_id: Option<OsString>,
    pub(crate) claude_session: Option<String>,
}

impl Env {
    pub(crate) fn from_process() -> Self {
        Self {
            sandboxed: std::env::var_os("DELEGATE_SANDBOXED").is_some(),
            socket: std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from),
            pane_id: std::env::var_os("HERDR_PANE_ID"),
            claude_session: std::env::var("CLAUDE_CODE_SESSION_ID")
                .ok()
                .filter(|session| !session.is_empty()),
        }
    }

    /// Whether this launch came from a Herdr pane at all, valid or not.
    pub(crate) fn herdr_present(&self) -> bool {
        self.socket.is_some() || self.pane_id.is_some()
    }
}

/// What capture found. A skip is a reason for the one-line note, never an error.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Recorded(Origin),
    Skipped(&'static str),
}

impl Outcome {
    pub(crate) fn origin(&self) -> Option<Origin> {
        match self {
            Outcome::Recorded(origin) => Some(origin.clone()),
            Outcome::Skipped(_) => None,
        }
    }

    pub(crate) fn reason(&self) -> Option<&'static str> {
        match self {
            Outcome::Recorded(_) => None,
            Outcome::Skipped(reason) => Some(reason),
        }
    }
}

/// The stderr line `new` prints when a Herdr launch recorded nothing.
pub(crate) fn note(id: &str, reason: &str) -> String {
    format!("note: no lead origin recorded for {id} ({reason}); the task runs normally")
}

/// Read the launching pane's identity. Failures are bounded and never guessed.
pub(crate) fn capture(env: &Env) -> Outcome {
    if env.sandboxed {
        return Outcome::Skipped("sandboxed");
    }
    let Some(socket) = env.socket.as_deref() else {
        return Outcome::Skipped("no Herdr socket");
    };
    if !socket.is_absolute() {
        return Outcome::Skipped("Herdr socket path is not absolute");
    }
    let Ok(metadata) = std::fs::metadata(socket) else {
        return Outcome::Skipped("Herdr socket is unavailable");
    };
    if !metadata.file_type().is_socket() {
        return Outcome::Skipped("Herdr socket is not a UNIX socket");
    }
    let Some(pane) = env.pane_id.as_deref().and_then(OsStr::to_str) else {
        return Outcome::Skipped("no Herdr pane id");
    };
    if pane.is_empty() {
        return Outcome::Skipped("no Herdr pane id");
    }
    if pane.chars().any(forbidden_category_c) {
        return Outcome::Skipped("invalid Herdr pane id");
    }

    let deadline = Instant::now() + DEADLINE;
    let Ok(mut stream) = connect(socket, deadline) else {
        return Outcome::Skipped("Herdr socket did not connect in time");
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = json!({
        "id": &request_id,
        "method": "pane.get",
        "params": {"pane_id": pane},
    })
    .to_string()
        + "\n";
    if let Err(reason) = send(&mut stream, request.as_bytes(), deadline) {
        return Outcome::Skipped(reason);
    }
    let line = match receive(&mut stream, deadline) {
        Ok(line) => line,
        Err(reason) => return Outcome::Skipped(reason),
    };
    parse(&line, &request_id, pane, env.claude_session.as_deref())
}

/// `UnixStream::connect` has no timeout; bound it with the same total deadline.
fn connect(socket: &Path, deadline: Instant) -> Result<UnixStream, &'static str> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = socket.to_path_buf();
    let handle = std::thread::Builder::new()
        .spawn(move || {
            let _ = sender.send(UnixStream::connect(path));
        })
        .map_err(|_| "Herdr socket failed")?;
    let left = deadline.saturating_duration_since(Instant::now());
    let connected = receiver.recv_timeout(left);
    drop(handle);
    match connected {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(_)) | Err(_) => Err("Herdr socket did not connect"),
    }
}

fn send(stream: &mut UnixStream, request: &[u8], deadline: Instant) -> Result<(), &'static str> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err("Herdr request timed out");
    }
    if stream.set_write_timeout(Some(left)).is_err() {
        return Err("Herdr socket failed");
    }
    stream
        .write_all(request)
        .map_err(|_| "Herdr request failed")
}

fn receive(stream: &mut UnixStream, deadline: Instant) -> Result<Vec<u8>, &'static str> {
    let mut line = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("Herdr response timed out");
        }
        if stream.set_read_timeout(Some(left)).is_err() {
            return Err("Herdr socket failed");
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err("Herdr closed without a response"),
            Ok(read) => {
                line.extend_from_slice(&chunk[..read]);
                if line.len() > MAX_LINE {
                    return Err("Herdr response was too large");
                }
                if line.last() == Some(&b'\n') {
                    return Ok(line);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err("Herdr response timed out");
            }
            Err(_) => return Err("Herdr response failed"),
        }
    }
}

fn parse(line: &[u8], request_id: &str, pane: &str, claude: Option<&str>) -> Outcome {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return Outcome::Skipped("Herdr response was not JSON");
    };
    if value.get("id").and_then(Value::as_str) != Some(request_id) {
        return Outcome::Skipped("Herdr response had another request id");
    }
    if value.get("error").is_some() {
        return Outcome::Skipped("Herdr returned an error");
    }
    let result = &value["result"];
    if result.get("type").and_then(Value::as_str) != Some("pane_info") {
        return Outcome::Skipped("Herdr did not return pane_info");
    }
    let info = &result["pane"];
    if info.get("pane_id").and_then(Value::as_str) != Some(pane) {
        return Outcome::Skipped("Herdr returned another pane");
    }
    let Some(identity) = info.get("agent_session").and_then(Value::as_object) else {
        return Outcome::Skipped("the pane has no agent session");
    };
    let (Some(source), Some(agent), Some(kind), Some(value)) = (
        identity.get("source").and_then(Value::as_str),
        identity.get("agent").and_then(Value::as_str),
        identity.get("kind").and_then(Value::as_str),
        identity.get("value").and_then(Value::as_str),
    ) else {
        return Outcome::Skipped("the pane's identity is incomplete");
    };
    if [source, agent, value]
        .into_iter()
        .any(|field| !valid_text(field))
    {
        return Outcome::Skipped("the pane's identity is unsafe");
    }
    let kind = match kind {
        "id" => RefKind::Id,
        "path" => RefKind::Path,
        _ => return Outcome::Skipped("the pane's identity kind is unknown"),
    };
    if kind == RefKind::Path && !Path::new(value).is_absolute() {
        return Outcome::Skipped("the pane's identity path is not absolute");
    }
    let origin = match (source, agent) {
        ("herdr:pi", "pi") | ("herdr:claude", "claude") => Origin::Supported {
            source: source.into(),
            agent: agent.into(),
            kind,
            value: value.into(),
        },
        ("herdr:codex", "codex") => Origin::Unsupported {
            source: source.into(),
            agent: agent.into(),
        },
        _ => return Outcome::Skipped("the pane's agent is not supported"),
    };
    if let Some(session) = claude {
        let matches = matches!(
            &origin,
            Origin::Supported { source, agent, kind: RefKind::Id, value }
                if source == "herdr:claude" && agent == "claude" && value == session
        );
        if !matches {
            return Outcome::Skipped("the pane's identity differs from CLAUDE_CODE_SESSION_ID");
        }
    }
    Outcome::Recorded(origin)
}

/// Whether a stored origin still meets the capture rules. `task::load` drops it otherwise.
pub(crate) fn valid(origin: &Origin) -> bool {
    match origin {
        Origin::Supported {
            source,
            agent,
            kind,
            value,
        } => {
            valid_text(source)
                && valid_text(agent)
                && valid_text(value)
                && matches!(
                    (source.as_str(), agent.as_str()),
                    ("herdr:pi", "pi") | ("herdr:claude", "claude")
                )
                && (*kind != RefKind::Path || Path::new(value).is_absolute())
        }
        Origin::Unsupported { source, agent } => {
            valid_text(source)
                && valid_text(agent)
                && source == "herdr:codex"
                && agent == "codex"
        }
    }
}

/// An identity string is nonempty, bounded and free of category C, Zl and Zp characters.
fn valid_text(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_FIELD_CHARS
        && !text.chars().any(forbidden_identity_char)
}

fn forbidden_identity_char(c: char) -> bool {
    forbidden_category_c(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Unicode category C without a dependency: controls, format, private use and the
/// unassigned ranges we can name (planes 4-13 and noncharacters). Scattered unassigned
/// codepoints stay undetected; no dependency is allowed to carry their table.
fn forbidden_category_c(c: char) -> bool {
    c.is_control() || is_format(c) || is_private_use(c) || is_unassigned(c)
}

fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}' | '\u{061c}' | '\u{06dd}' | '\u{070f}' | '\u{08e2}' | '\u{180e}' | '\u{feff}'
    ) || ('\u{0600}'..='\u{0605}').contains(&c)
        || ('\u{0890}'..='\u{0891}').contains(&c)
        || ('\u{200b}'..='\u{200f}').contains(&c)
        || ('\u{202a}'..='\u{202e}').contains(&c)
        || ('\u{2060}'..='\u{2064}').contains(&c)
        || ('\u{2066}'..='\u{206f}').contains(&c)
        || ('\u{fff9}'..='\u{fffb}').contains(&c)
        || matches!(c, '\u{110bd}' | '\u{110cd}')
        || ('\u{13430}'..='\u{1343f}').contains(&c)
        || ('\u{1bca0}'..='\u{1bca3}').contains(&c)
        || ('\u{1d173}'..='\u{1d17a}').contains(&c)
        || matches!(c, '\u{e0001}')
        || ('\u{e0020}'..='\u{e007f}').contains(&c)
}

fn is_private_use(c: char) -> bool {
    ('\u{e000}'..='\u{f8ff}').contains(&c)
        || ('\u{f0000}'..='\u{ffffd}').contains(&c)
        || ('\u{100000}'..='\u{10fffd}').contains(&c)
}

fn is_unassigned(c: char) -> bool {
    let code = u32::from(c);
    ('\u{40000}'..='\u{dffff}').contains(&c) // planes 4-13 are unassigned
        || ('\u{fdd0}'..='\u{fdef}').contains(&c) // noncharacters
        || (code & 0xffff) == 0xfffe
        || (code & 0xffff) == 0xffff
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::thread;

    /// A one-connection Herdr stand-in in its own temp dir.
    struct Fixture {
        dir: PathBuf,
        path: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn serve(delay: Duration, reply: impl FnOnce(Value) -> Vec<u8> + Send + 'static) -> Fixture {
        let dir = temp_dir();
        let path = dir.join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        thread::spawn(move || {
            let Ok((conn, _)) = listener.accept() else {
                return;
            };
            let request = {
                let mut reader = BufReader::new(&conn);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    return;
                }
                serde_json::from_str::<Value>(&line).ok()
            };
            let Some(request) = request else { return };
            thread::sleep(delay);
            let body = reply(request);
            let mut out = &conn;
            let _ = out.write_all(&body);
        });
        Fixture { dir, path }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("delegate-origin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn line(value: &Value) -> Vec<u8> {
        (value.to_string() + "\n").into_bytes()
    }

    fn identity(source: &str, agent: &str, kind: &str, value: &str) -> Value {
        json!({"source": source, "agent": agent, "kind": kind, "value": value})
    }

    fn reply(request: &Value, agent_session: Option<Value>) -> Vec<u8> {
        let id = request["id"].clone();
        let pane = request["params"]["pane_id"].clone();
        let mut info = json!({"pane_id": pane});
        if let Some(agent_session) = agent_session {
            info["agent_session"] = agent_session;
        }
        line(&json!({"id": id, "result": {"type": "pane_info", "pane": info}}))
    }

    fn env(fixture: &Fixture) -> Env {
        Env {
            sandboxed: false,
            socket: Some(fixture.path.clone()),
            pane_id: Some(OsString::from("w1:p1")),
            claude_session: None,
        }
    }

    fn skipped(outcome: &Outcome) -> bool {
        matches!(outcome, Outcome::Skipped(_))
    }

    #[test]
    fn pi_path_and_claude_id_are_recorded() {
        let pi = serve(Duration::ZERO, |request| {
            reply(
                &request,
                Some(identity("herdr:pi", "pi", "path", "/sessions/lead.jsonl")),
            )
        });
        assert_eq!(
            capture(&env(&pi)),
            Outcome::Recorded(Origin::Supported {
                source: "herdr:pi".into(),
                agent: "pi".into(),
                kind: RefKind::Path,
                value: "/sessions/lead.jsonl".into(),
            })
        );

        let claude = serve(Duration::ZERO, |request| {
            reply(
                &request,
                Some(identity("herdr:claude", "claude", "id", "session-a")),
            )
        });
        let mut env = env(&claude);
        env.claude_session = Some("session-a".into());
        assert_eq!(
            capture(&env),
            Outcome::Recorded(Origin::Supported {
                source: "herdr:claude".into(),
                agent: "claude".into(),
                kind: RefKind::Id,
                value: "session-a".into(),
            })
        );
    }

    #[test]
    fn codex_is_unsupported_without_its_value() {
        let codex = serve(Duration::ZERO, |request| {
            reply(
                &request,
                Some(identity("herdr:codex", "codex", "id", "thread-secret")),
            )
        });
        let Outcome::Recorded(origin) = capture(&env(&codex)) else {
            panic!("codex should be recorded as unsupported");
        };
        assert_eq!(
            origin,
            Origin::Unsupported {
                source: "herdr:codex".into(),
                agent: "codex".into(),
            }
        );
        assert_eq!(
            serde_json::to_value(&origin).unwrap(),
            json!({"support": "unsupported", "source": "herdr:codex", "agent": "codex"})
        );
        assert!(!serde_json::to_string(&origin).unwrap().contains("thread-secret"));
    }

    #[test]
    fn unknown_pair_records_nothing() {
        for (source, agent) in [
            ("herdr:gemini", "gemini"),
            ("herdr:pi", "claude"),
            ("herdr:codex", "pi"),
            ("elsewhere:claude", "claude"),
        ] {
            let fixture = serve(Duration::ZERO, move |request| {
                reply(&request, Some(identity(source, agent, "id", "value-1")))
            });
            let outcome = capture(&env(&fixture));
            assert!(skipped(&outcome), "{source}/{agent} was recorded");
        }
    }

    #[test]
    fn protocol_failures_record_nothing() {
        let error = serve(Duration::ZERO, |request| {
            line(&json!({
                "id": request["id"],
                "error": {"code": "internal", "message": "stand-in error"},
            }))
        });
        assert!(skipped(&capture(&env(&error))));

        let wrong_id = serve(Duration::ZERO, |request| {
            let mut reply = reply(&request, Some(identity("herdr:pi", "pi", "id", "pane-a")));
            let mut value: Value = serde_json::from_slice(&reply).unwrap();
            value["id"] = "other-request".into();
            reply = line(&value);
            reply
        });
        assert!(skipped(&capture(&env(&wrong_id))));

        let wrong_pane = serve(Duration::ZERO, |request| {
            let mut value: Value =
                serde_json::from_slice(&reply(&request, Some(identity("herdr:pi", "pi", "id", "pane-a"))))
                    .unwrap();
            value["result"]["pane"]["pane_id"] = "w9:p9".into();
            line(&value)
        });
        assert!(skipped(&capture(&env(&wrong_pane))));

        let no_session = serve(Duration::ZERO, |request| reply(&request, None));
        assert!(skipped(&capture(&env(&no_session))));

        let garbage = serve(Duration::ZERO, |_request| b"not json\n".to_vec());
        assert!(skipped(&capture(&env(&garbage))));

        let no_newline = serve(Duration::ZERO, |request| {
            let mut reply = reply(&request, Some(identity("herdr:pi", "pi", "id", "pane-a")));
            reply.pop();
            reply
        });
        assert!(skipped(&capture(&env(&no_newline))));

        let oversized = serve(Duration::ZERO, |_request| {
            let mut reply = vec![b'x'; MAX_LINE + 1];
            reply.push(b'\n');
            reply
        });
        assert!(skipped(&capture(&env(&oversized))));
    }

    #[test]
    fn slow_response_stops_at_the_deadline() {
        let slow = serve(Duration::from_secs(6), |request| {
            reply(
                &request,
                Some(identity("herdr:pi", "pi", "path", "/sessions/lead.jsonl")),
            )
        });
        let start = Instant::now();
        let outcome = capture(&env(&slow));
        assert!(skipped(&outcome));
        assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    }

    #[test]
    fn unsafe_identity_fields_record_nothing() {
        for field in ["source", "agent", "kind", "value"] {
            for bad in ['\u{1b}', '\u{202e}', '\u{200b}', '\u{2028}', '\u{2029}'] {
                let fixture = serve(Duration::ZERO, move |request| {
                    let mut agent_session = identity("herdr:pi", "pi", "id", "pane-a");
                    let text = agent_session[field].as_str().unwrap().to_string();
                    agent_session[field] = format!("{text}{bad}").into();
                    reply(&request, Some(agent_session))
                });
                let outcome = capture(&env(&fixture));
                assert!(
                    skipped(&outcome),
                    "field {field} accepted {:?}",
                    bad.escape_debug()
                );
            }
        }
    }

    #[test]
    fn relative_path_records_nothing() {
        let fixture = serve(Duration::ZERO, |request| {
            reply(
                &request,
                Some(identity("herdr:pi", "pi", "path", "sessions/lead.jsonl")),
            )
        });
        assert!(skipped(&capture(&env(&fixture))));
    }

    #[test]
    fn claude_session_must_match() {
        let claude = serve(Duration::ZERO, |request| {
            reply(
                &request,
                Some(identity("herdr:claude", "claude", "id", "session-a")),
            )
        });
        let mut mismatched = env(&claude);
        mismatched.claude_session = Some("session-b".into());
        assert!(skipped(&capture(&mismatched)));

        for (source, agent, kind) in [
            ("herdr:pi", "pi", "path"),
            ("herdr:codex", "codex", "id"),
            ("herdr:claude", "claude", "path"),
        ] {
            let fixture = serve(Duration::ZERO, move |request| {
                reply(
                    &request,
                    Some(identity(source, agent, kind, "/sessions/lead.jsonl")),
                )
            });
            let mut paired = env(&fixture);
            paired.claude_session = Some("session-a".into());
            assert!(skipped(&capture(&paired)), "{source} ignored the Claude session");
        }
    }

    #[test]
    fn sandbox_and_bad_endpoints_skip_before_connecting() {
        let dir = temp_dir();
        let path = dir.join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let offline = Env {
            sandboxed: true,
            socket: Some(path.clone()),
            pane_id: Some(OsString::from("w1:p1")),
            claude_session: None,
        };
        assert_eq!(capture(&offline), Outcome::Skipped("sandboxed"));
        assert!(listener.accept().is_err(), "a sandboxed new asked Herdr");

        let relative = Env {
            socket: Some(PathBuf::from("herdr.sock")),
            pane_id: Some(OsString::from("w1:p1")),
            ..Env::default()
        };
        assert_eq!(
            capture(&relative),
            Outcome::Skipped("Herdr socket path is not absolute")
        );

        let no_socket = Env {
            pane_id: Some(OsString::from("w1:p1")),
            ..Env::default()
        };
        assert_eq!(capture(&no_socket), Outcome::Skipped("no Herdr socket"));

        let missing = Env {
            socket: Some(dir.join("missing.sock")),
            pane_id: Some(OsString::from("w1:p1")),
            ..Env::default()
        };
        assert_eq!(
            capture(&missing),
            Outcome::Skipped("Herdr socket is unavailable")
        );

        let regular = dir.join("regular");
        std::fs::write(&regular, "").unwrap();
        let plain_file = Env {
            socket: Some(regular),
            pane_id: Some(OsString::from("w1:p1")),
            ..Env::default()
        };
        assert_eq!(
            capture(&plain_file),
            Outcome::Skipped("Herdr socket is not a UNIX socket")
        );

        for pane in [None, Some(""), Some("bad\u{1b}pane")] {
            let env = Env {
                socket: Some(path.clone()),
                pane_id: pane.map(OsString::from),
                ..Env::default()
            };
            assert!(skipped(&capture(&env)), "pane {pane:?} asked Herdr");
        }
        assert!(listener.accept().is_err(), "an invalid pane id asked Herdr");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn loaded_origins_are_validated() {
        let pi = Origin::Supported {
            source: "herdr:pi".into(),
            agent: "pi".into(),
            kind: RefKind::Path,
            value: "/sessions/lead.jsonl".into(),
        };
        assert!(valid(&pi));
        assert!(valid(&Origin::Supported {
            source: "herdr:claude".into(),
            agent: "claude".into(),
            kind: RefKind::Id,
            value: "session-a".into(),
        }));
        assert!(valid(&Origin::Unsupported {
            source: "herdr:codex".into(),
            agent: "codex".into(),
        }));

        let supported = |source: &str, agent: &str, kind: RefKind, value: String| Origin::Supported {
            source: source.into(),
            agent: agent.into(),
            kind,
            value,
        };
        for bad in [
            supported("", "pi", RefKind::Path, "/sessions/lead.jsonl".into()),
            supported("herdr:pi", "", RefKind::Path, "/sessions/lead.jsonl".into()),
            supported("herdr:pi", "pi", RefKind::Path, String::new()),
            supported(
                "herdr:pi",
                "pi",
                RefKind::Path,
                "relative/lead.jsonl".into(),
            ),
            supported(
                "herdr:pi",
                "pi",
                RefKind::Id,
                "a".repeat(MAX_FIELD_CHARS + 1),
            ),
            supported(
                "herdr:pi\u{1b}",
                "pi",
                RefKind::Id,
                "pane-a".into(),
            ),
            supported("herdr:gemini", "gemini", RefKind::Id, "g-1".into()),
            Origin::Unsupported {
                source: "herdr:pi".into(),
                agent: "pi".into(),
            },
            Origin::Unsupported {
                source: "herdr:codex".into(),
                agent: "codex\u{202e}".into(),
            },
        ] {
            assert!(!valid(&bad), "{bad:?} should not load as a valid origin");
        }
    }

    #[test]
    fn note_names_only_the_task_and_reason() {
        assert_eq!(
            note("t001", "Herdr response timed out"),
            "note: no lead origin recorded for t001 (Herdr response timed out); the task runs normally"
        );
    }
}
