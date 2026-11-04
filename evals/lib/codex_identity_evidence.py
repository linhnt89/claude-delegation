"""Pure, fail-closed evidence readers for the S146 Codex attribution check.

This module validates in-memory snapshots only. The live driver owns trusted
collection, process control, filesystem access and the verdict oracle. There is no
filesystem, process, socket or network access here and no import-time side effect.

Every public reader raises :class:`EvidenceError` with a stable, redacted ``code``
for invalid or unsupported input. A stable code never contains caller data. Pending
rollout evidence is reported with ``None`` only where the contract says a writer may
still complete the record.
"""

from __future__ import annotations

import functools
import json
import math
import posixpath
import re
from datetime import datetime

__all__ = [
    "EvidenceError",
    "attest_process",
    "bind_launch",
    "parse_pid_record",
    "parse_proc_stat",
    "parse_rollout",
]

CODEX_VERSION = "0.166.0"
MODEL_PROVIDER = "local"

DAEMON_ARGS = (
    "app-server",
    "--listen",
    "unix://",
    "--analytics-default-enabled",
    "--managed-daemon",
)
UPDATER_ARGS = ("app-server", "daemon", "pid-update-loop")

_PID_MAX = (1 << 31) - 1
_TICKS_MAX = (1 << 64) - 1
_DELETED_SUFFIX = " (deleted)"

_MISSING = object()
_INT_RE = re.compile(r"^[+-]?\d+$")
_UINT_RE = re.compile(r"^\d+$")

# Closed supported timestamp grammar: datetime is only ever asked to validate
# calendar/timezone values inside this pinned driver subset. Full-string matching
# prevents fromisoformat from accepting or silently truncating extensions such as
# offset seconds or fractions longer than microseconds.
_TIMESTAMP_RE = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}"
    r"(?:\.[0-9]{1,6})?(?:Z|[+-][0-9]{2}:[0-9]{2})"
)

# Linux x86_64 /proc/<pid>/stat numeric field bounds (proc(5) fields 4..52).
# Field 3 is the state character. int is 32-bit; long, unsigned long and long long
# are 64-bit. Field 22 (starttime) is identity evidence and is also checked > 0.
_INT32_BOUNDS = (-(1 << 31), (1 << 31) - 1)
_UINT32_BOUNDS = (0, (1 << 32) - 1)
_INT64_BOUNDS = (-(1 << 63), (1 << 63) - 1)
_UINT64_BOUNDS = (0, (1 << 64) - 1)
_STAT_FIELD_BOUNDS = {
    4: (0, _INT32_BOUNDS[1]),  # ppid: signed pid_t, but negative is impossible
    5: _INT32_BOUNDS,  # pgrp
    6: _INT32_BOUNDS,  # session
    7: _INT32_BOUNDS,  # tty_nr
    8: _INT32_BOUNDS,  # tpgid; -1 when there is no foreground group
    9: _UINT32_BOUNDS,  # flags
    10: _UINT64_BOUNDS,  # minflt
    11: _UINT64_BOUNDS,  # cminflt
    12: _UINT64_BOUNDS,  # majflt
    13: _UINT64_BOUNDS,  # cmajflt
    14: _UINT64_BOUNDS,  # utime
    15: _UINT64_BOUNDS,  # stime
    16: _INT64_BOUNDS,  # cutime
    17: _INT64_BOUNDS,  # cstime
    18: _INT64_BOUNDS,  # priority
    19: _INT64_BOUNDS,  # nice
    20: _INT64_BOUNDS,  # num_threads
    21: _INT64_BOUNDS,  # itrealvalue
    22: _UINT64_BOUNDS,  # starttime
    23: _UINT64_BOUNDS,  # vsize
    24: _INT64_BOUNDS,  # rss
    25: _UINT64_BOUNDS,  # rsslim
    26: _UINT64_BOUNDS,  # startcode
    27: _UINT64_BOUNDS,  # endcode
    28: _UINT64_BOUNDS,  # startstack
    29: _UINT64_BOUNDS,  # kstkesp
    30: _UINT64_BOUNDS,  # kstkeip
    31: _UINT64_BOUNDS,  # signal
    32: _UINT64_BOUNDS,  # blocked
    33: _UINT64_BOUNDS,  # sigignore
    34: _UINT64_BOUNDS,  # sigcatch
    35: _UINT64_BOUNDS,  # wchan
    36: _UINT64_BOUNDS,  # nswap
    37: _UINT64_BOUNDS,  # cnswap
    38: _INT32_BOUNDS,  # exit_signal
    39: _INT32_BOUNDS,  # processor
    40: _UINT32_BOUNDS,  # rt_priority
    41: _UINT32_BOUNDS,  # policy
    42: _UINT64_BOUNDS,  # delayacct_blkio_ticks
    43: _UINT64_BOUNDS,  # guest_time
    44: _INT64_BOUNDS,  # cguest_time
    45: _UINT64_BOUNDS,  # start_data
    46: _UINT64_BOUNDS,  # end_data
    47: _UINT64_BOUNDS,  # start_brk
    48: _UINT64_BOUNDS,  # arg_start
    49: _UINT64_BOUNDS,  # arg_end
    50: _UINT64_BOUNDS,  # env_start
    51: _UINT64_BOUNDS,  # env_end
    52: _INT32_BOUNDS,  # exit_code
}

_TOLERATED_RECORD_TYPES = frozenset({"world_state", "token_usage_record"})
_SESSION_FIELDS = ("thread_id", "session_id")
_TURN_FIELDS = ("turn_id", "root_turn_id")
_BINDING_CONTAINERS = (
    "turn_attribution",
    "item",
    "message",
    "thread_settings",
    "internal_chat_message_metadata_passthrough",
)
_TOLERATED_EVENT_TYPES = frozenset(
    {
        "thread_settings_applied",
        "item_completed",
        "token_count",
        "user_message",
        "agent_message",
    }
)
_LIVE_STATES = frozenset("RSDTtKWP I".replace(" ", ""))
_DEAD_STATES = frozenset("ZXx")


class EvidenceError(Exception):
    """Redacted evidence failure; ``code`` is stable and never contains input."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


def _fail(code: str):
    raise EvidenceError(code)


def _guard(function):
    """Map unexpected parser failures to a redacted EvidenceError."""

    @functools.wraps(function)
    def wrapper(*args, **kwargs):
        try:
            return function(*args, **kwargs)
        except EvidenceError:
            raise
        except Exception:
            raise EvidenceError("invalid_input") from None

    return wrapper


def _mapping(value, code: str) -> dict:
    if not isinstance(value, dict):
        _fail(code)
    return value


def _string(value, code: str, *, nonempty: bool = False) -> str:
    if not isinstance(value, str):
        _fail(code)
    if nonempty and not value:
        _fail(code)
    return value


def _positive_int(value, code: str, *, maximum: int | None = None) -> int:
    # bool is an int subclass; it is never accepted as evidence.
    if type(value) is not int or value <= 0:
        _fail(code)
    if maximum is not None and value > maximum:
        _fail(code)
    return value


def _timestamp(value, code: str) -> datetime:
    text = _string(value, code, nonempty=True)
    if _TIMESTAMP_RE.fullmatch(text) is None:
        _fail(code)
    candidate = text[:-1] + "+00:00" if text.endswith("Z") else text
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError:
        raise EvidenceError(code) from None
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        _fail(code)
    return parsed


def _json_object(data: bytes) -> dict:
    if not isinstance(data, bytes):
        _fail("invalid_input")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        raise EvidenceError("invalid_utf8") from None

    try:
        parsed = json.loads(
            text, object_pairs_hook=_json_pairs, parse_constant=_reject_constant
        )
    except EvidenceError:
        raise
    except (json.JSONDecodeError, ValueError):
        raise EvidenceError("malformed_json") from None
    _check_finite(parsed)
    return _mapping(parsed, "malformed_json")


def _complete_records(data: bytes) -> list[dict]:
    """Return parsed complete JSONL records, ignoring the final unterminated line."""
    if not isinstance(data, bytes):
        _fail("invalid_input")
    if b"\n" not in data:
        return []
    complete, _, _tail = data.rpartition(b"\n")
    records = []
    for raw in complete.split(b"\n"):
        if not raw:
            _fail("malformed_jsonl")
        try:
            line = raw.decode("utf-8")
        except UnicodeDecodeError:
            raise EvidenceError("invalid_utf8") from None
        try:
            record = json.loads(
                line, object_pairs_hook=_json_pairs, parse_constant=_reject_constant
            )
        except EvidenceError:
            raise
        except (json.JSONDecodeError, ValueError):
            raise EvidenceError("malformed_jsonl") from None
        _check_finite(record)
        records.append(_mapping(record, "malformed_jsonl"))
    return records


def _reject_constant(value: str):
    """Reject NaN/Infinity/-Infinity wherever they appear, ignored fields included."""
    raise EvidenceError("non_finite_number") from None


def _check_finite(value) -> None:
    if isinstance(value, float):
        if not math.isfinite(value):
            _fail("non_finite_number")
        return
    if isinstance(value, dict):
        for item in value.values():
            _check_finite(item)
    elif isinstance(value, list):
        for item in value:
            _check_finite(item)


def _json_pairs(values):
    result = {}
    for key, value in values:
        if key in result:
            _fail("duplicate_json_key")
        result[key] = value
    return result


def _record_parts(record: dict) -> tuple[str, dict]:
    record_type = record.get("type", _MISSING)
    if record_type is _MISSING:
        _fail("unsupported_record")
    record_type = _string(record_type, "unsupported_record", nonempty=True)
    payload = _mapping(record.get("payload", _MISSING), "unsupported_record")
    timestamp = record.get("timestamp", _MISSING)
    if timestamp is not _MISSING:
        _string(timestamp, "invalid_field_type")
    return record_type, payload


def _session_identity(payload: dict) -> str:
    raw_id = payload.get("id", _MISSING)
    raw_session = payload.get("session_id", _MISSING)
    values = []
    for raw in (raw_id, raw_session):
        if raw is not _MISSING:
            values.append(_string(raw, "invalid_session_metadata", nonempty=True))
    if not values:
        _fail("invalid_session_metadata")
    if len(values) == 2 and values[0] != values[1]:
        _fail("conflicting_metadata")
    return values[0]


def _session_meta(payload: dict) -> dict:
    identifier = _session_identity(payload)
    timestamp = _string(
        payload.get("timestamp", _MISSING), "invalid_session_metadata", nonempty=True
    )
    return {
        "id": identifier,
        "cwd": _string(payload.get("cwd", _MISSING), "invalid_session_metadata", nonempty=True),
        "cli_version": _string(
            payload.get("cli_version", _MISSING), "invalid_session_metadata", nonempty=True
        ),
        "model_provider": _string(
            payload.get("model_provider", _MISSING),
            "invalid_session_metadata",
            nonempty=True,
        ),
        "session_timestamp": timestamp,
        "_timestamp": _timestamp(timestamp, "invalid_session_metadata"),
    }


def _turn_identity(payload: dict, code: str) -> tuple[str, str]:
    turn_id = _string(payload.get("turn_id", _MISSING), code, nonempty=True)
    root_turn_id = _string(payload.get("root_turn_id", _MISSING), code, nonempty=True)
    return turn_id, root_turn_id


def _check_turn_attribution(payload: dict, turn_id: str, root_turn_id: str) -> None:
    attribution = payload.get("turn_attribution", _MISSING)
    if attribution is _MISSING:
        return
    attribution = _mapping(attribution, "invalid_turn")
    if "turn_id" in attribution:
        nested = _string(attribution["turn_id"], "invalid_turn", nonempty=True)
        if nested != turn_id:
            _fail("conflicting_metadata")
    if "root_turn_id" in attribution:
        nested = _string(attribution["root_turn_id"], "invalid_turn", nonempty=True)
        if nested != root_turn_id:
            _fail("conflicting_metadata")


def _task_started(payload: dict) -> dict:
    turn_id, root_turn_id = _turn_identity(payload, "invalid_turn")
    _check_turn_attribution(payload, turn_id, root_turn_id)
    return {"turn_id": turn_id, "root_turn_id": root_turn_id}


def _task_complete(payload: dict) -> dict:
    turn_id, root_turn_id = _turn_identity(payload, "invalid_turn")
    _check_turn_attribution(payload, turn_id, root_turn_id)
    last_message = _string(
        payload.get("last_agent_message", _MISSING), "invalid_completion"
    )
    return {
        "turn_id": turn_id,
        "root_turn_id": root_turn_id,
        "last_agent_message": last_message,
    }


def _message_role(payload: dict) -> str:
    if payload.get("type", _MISSING) != "message":
        _fail("unsupported_record")
    return _string(payload.get("role", _MISSING), "invalid_message", nonempty=True)


def _message_texts(payload: dict, role: str) -> list[tuple[str, str]]:
    """Return every content item text for a selected user/assistant message.

    Unknown item types or shapes are never discarded before comparison: a refusal,
    image or text item without ``text`` fails closed instead of being skipped.
    """
    content = payload.get("content", _MISSING)
    if not isinstance(content, list):
        _fail("invalid_message")
    expected_type = "input_text" if role == "user" else "output_text"
    texts = []
    for item in content:
        item = _mapping(item, "invalid_message")
        item_type = _string(item.get("type", _MISSING), "invalid_message", nonempty=True)
        if item_type != expected_type:
            _fail("unsupported_message_content")
        text = _string(item.get("text", _MISSING), "invalid_message")
        texts.append((item_type, text))
    return texts


def _turn_fields(source: dict, code: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for key in _TURN_FIELDS:
        if key in source:
            fields[key] = _string(source[key], code, nonempty=True)
    return fields


def _merge_turn_fields(target: dict[str, str], source: dict, code: str) -> None:
    for key, value in _turn_fields(source, code).items():
        if key in target and target[key] != value:
            _fail("conflicting_metadata")
        target[key] = value


def _merge_binding_fields(target: dict[str, str], source: dict, code: str) -> None:
    _merge_turn_fields(target, source, code)
    for key in _SESSION_FIELDS:
        if key in source:
            value = _string(source[key], code, nonempty=True)
            if key in target and target[key] != value:
                _fail("conflicting_metadata")
            target[key] = value


def _check_binding_fields(fields: dict[str, str], meta: dict, check_turn) -> None:
    for key in _SESSION_FIELDS:
        if key in fields and fields[key] != meta["id"]:
            _fail("conflicting_metadata")
    turns = {key: fields[key] for key in _TURN_FIELDS if key in fields}
    check_turn(turns)


def _check_payload_binding(payload: dict, meta: dict, check_turn) -> None:
    """Validate every present thread/session/turn binding on a tolerated payload."""
    fields: dict[str, str] = {}
    _merge_binding_fields(fields, payload, "invalid_binding")
    for name in _BINDING_CONTAINERS:
        nested = payload.get(name, _MISSING)
        if nested is _MISSING:
            continue
        if not isinstance(nested, dict):
            if name in {"item", "message"}:
                # Message/item ids and opaque content are not conversation identity.
                continue
            _fail("invalid_binding")
        _merge_binding_fields(fields, nested, "invalid_binding")
    _check_binding_fields(fields, meta, check_turn)


def _message_attribution(record: dict, payload: dict, role: str) -> dict[str, str]:
    """Collect and cross-check every present binding attached to a message record."""
    fields: dict[str, str] = {}
    internal = payload.get("internal_chat_message_metadata_passthrough", _MISSING)
    if internal is not _MISSING:
        internal = _mapping(internal, "invalid_message")
        _merge_binding_fields(fields, internal, "invalid_message")
    metadata = record.get("metadata", _MISSING)
    if metadata is not _MISSING:
        metadata = _mapping(metadata, "invalid_message")
        retained = metadata.get("retained_source", _MISSING)
        if retained is not _MISSING:
            retained = _mapping(retained, "invalid_message")
            source_id = retained.get("id", _MISSING)
            if source_id is not _MISSING:
                source_id = _mapping(source_id, "invalid_message")
                source_role = source_id.get("role", _MISSING)
                if source_role is not _MISSING:
                    source_role = _string(
                        source_role, "invalid_message", nonempty=True
                    )
                    if source_role != role:
                        _fail("conflicting_metadata")
                _merge_binding_fields(fields, source_id, "invalid_message")
    return fields


def _require_turn_match(fields: dict[str, str], started: dict) -> None:
    for key in _TURN_FIELDS:
        if key in fields and fields[key] != started[key]:
            _fail("conflicting_metadata")


def _text_matches(texts: list[tuple[str, str]], item_type: str, expected: str) -> bool:
    """Require the complete textual output to equal the expected text."""
    if not texts or any(kind != item_type for kind, _ in texts):
        return False
    return "".join(text for _, text in texts) == expected


def _rollout_result(meta: dict, turn_id: str, prompt: str, reply: str) -> dict:
    return {
        "id": meta["id"],
        "cwd": meta["cwd"],
        "cli_version": meta["cli_version"],
        "model_provider": meta["model_provider"],
        "session_timestamp": meta["session_timestamp"],
        "turn_id": turn_id,
        "prompt": prompt,
        "reply": reply,
    }


@_guard
def parse_rollout(data: bytes, *, prompt: str, reply: str) -> dict | None:
    """Parse one completed Codex rollout turn, or return ``None`` while pending."""
    _string(prompt, "invalid_input")
    _string(reply, "invalid_input")
    records = _complete_records(data)
    if not records:
        return None
    first_type, _ = _record_parts(records[0])
    if first_type != "session_meta":
        _fail("invalid_ordering")

    meta = None
    started = None
    completed = None
    prompt_seen = False
    assistant_seen = False
    pending_turns: list[dict[str, str]] = []

    def check_turn(fields: dict[str, str]) -> None:
        if started is None:
            pending_turns.append(fields)
        else:
            _require_turn_match(fields, started)

    for record in records:
        record_type, payload = _record_parts(record)

        if record_type == "session_meta":
            if meta is not None:
                _fail("conflicting_metadata")
            if started is not None or completed is not None:
                _fail("invalid_ordering")
            meta = _session_meta(payload)
            _check_payload_binding(payload, meta, check_turn)
            continue

        if meta is None:
            _fail("invalid_ordering")
        _check_payload_binding(payload, meta, check_turn)

        if record_type == "turn_context":
            continue

        if record_type in _TOLERATED_RECORD_TYPES:
            continue

        if record_type == "response_item":
            payload_type = payload.get("type", _MISSING)
            if payload_type == "reasoning":
                continue
            if payload_type != "message":
                _fail("unsupported_record")
            role = _message_role(payload)
            _check_binding_fields(
                _message_attribution(record, payload, role), meta, check_turn
            )
            if role == "developer":
                # Developer context is tolerated without constraining its content.
                continue
            if role not in {"user", "assistant"}:
                _fail("unsupported_record")
            texts = _message_texts(payload, role)
            if started is None:
                _fail("invalid_ordering")
            if role == "user":
                if completed is not None or prompt_seen:
                    _fail("ambiguous_rollout")
                if _text_matches(texts, "input_text", prompt):
                    prompt_seen = True
                continue
            if not prompt_seen:
                _fail("invalid_ordering")
            if completed is not None or assistant_seen:
                _fail("ambiguous_rollout")
            assistant_seen = True
            if not _text_matches(texts, "output_text", reply):
                _fail("wrong_reply")
            continue

        if record_type == "event_msg":
            event_type = _string(
                payload.get("type", _MISSING), "unsupported_record", nonempty=True
            )
            if event_type == "task_started":
                if started is not None or completed is not None:
                    _fail("ambiguous_rollout")
                if meta is None:
                    _fail("invalid_ordering")
                started = _task_started(payload)
                for fields in pending_turns:
                    _require_turn_match(fields, started)
                pending_turns.clear()
                continue
            if event_type == "task_complete":
                if started is None:
                    _fail("invalid_ordering")
                if completed is not None:
                    _fail("ambiguous_rollout")
                candidate = _task_complete(payload)
                if (
                    candidate["turn_id"] != started["turn_id"]
                    or candidate["root_turn_id"] != started["root_turn_id"]
                ):
                    _fail("conflicting_turn")
                if not prompt_seen or not assistant_seen:
                    _fail("missing_expected_messages")
                if candidate["last_agent_message"] != reply:
                    _fail("wrong_reply")
                completed = candidate
                continue
            if event_type in _TOLERATED_EVENT_TYPES:
                continue
            _fail("unsupported_record")

        _fail("unsupported_record")

    if meta is None:
        _fail("invalid_session_metadata")
    if completed is not None:
        return _rollout_result(meta, started["turn_id"], prompt, reply)
    return None


def _candidate_metadata(data: bytes) -> dict | None:
    records = _complete_records(data)
    if not records:
        return None
    record_type, payload = _record_parts(records[0])
    if record_type != "session_meta":
        _fail("invalid_session_metadata")
    meta = _session_meta(payload)
    for record in records[1:]:
        later_type, _ = _record_parts(record)
        if later_type == "session_meta":
            _fail("conflicting_metadata")
    return meta


@_guard
def bind_launch(
    rollouts: dict[str, bytes],
    before_paths: frozenset[str],
    *,
    cwd: str,
    launched_at: str,
    observed_at: str,
    prompt: str,
    reply: str,
) -> dict | None:
    """Bind exactly one new session rollout to a launch, or return pending ``None``."""
    if not isinstance(rollouts, dict):
        _fail("invalid_input")
    if not isinstance(before_paths, (set, frozenset)):
        _fail("invalid_input")
    cwd = _string(cwd, "invalid_input", nonempty=True)
    prompt = _string(prompt, "invalid_input")
    reply = _string(reply, "invalid_input")
    launched = _timestamp(launched_at, "invalid_timestamp")
    observed = _timestamp(observed_at, "invalid_timestamp")
    if launched > observed:
        _fail("time_reversed")

    for path, data in rollouts.items():
        _string(path, "invalid_input")
        if not isinstance(data, bytes):
            _fail("invalid_input")
    for path in before_paths:
        _string(path, "invalid_input")
        if path not in rollouts:
            _fail("disappeared_inventory")

    new_paths = [path for path in rollouts if path not in before_paths]
    if not new_paths:
        return None
    if len(new_paths) != 1:
        _fail("ambiguous_candidates")

    data = rollouts[new_paths[0]]
    if not isinstance(data, bytes):
        _fail("invalid_input")
    meta = _candidate_metadata(data)
    if meta is None:
        return None
    if meta["cwd"] != cwd:
        _fail("metadata_mismatch")
    if meta["cli_version"] != CODEX_VERSION:
        _fail("metadata_mismatch")
    if meta["model_provider"] != MODEL_PROVIDER:
        _fail("metadata_mismatch")
    timestamp = meta["_timestamp"]
    if timestamp < launched or timestamp > observed:
        _fail("candidate_out_of_window")
    return parse_rollout(data, prompt=prompt, reply=reply)


def _validate_digest(value) -> None:
    if not isinstance(value, list) or len(value) != 32:
        _fail("invalid_digest")
    for item in value:
        if type(item) is not int or item < 0 or item > 255:
            _fail("invalid_digest")


@_guard
def parse_pid_record(data: bytes, *, kind: str) -> dict:
    """Parse the actual daemon/updater PID JSON into a normalized identity."""
    if kind not in {"daemon", "updater"}:
        _fail("invalid_input")
    record = _json_object(data)
    pid = _positive_int(record.get("pid", _MISSING), "invalid_pid_record")
    process_start_time = _string(
        record.get("processStartTime", _MISSING),
        "invalid_pid_record",
        nonempty=True,
    )
    if not process_start_time:
        _fail("invalid_pid_record")
    identity = _mapping(
        record.get("processIdentity", _MISSING), "invalid_pid_record"
    )
    boot_id = _string(
        identity.get("bootId", _MISSING), "invalid_pid_record", nonempty=True
    )
    start_ticks = _positive_int(
        identity.get("startTicks", _MISSING),
        "invalid_pid_record",
        maximum=_TICKS_MAX,
    )
    executable_identity = record.get("executableIdentity", _MISSING)
    if executable_identity is not _MISSING:
        executable_identity = _mapping(
            executable_identity, "invalid_executable_identity"
        )
        for key in ("digest", "path_digest"):
            if key in executable_identity:
                _validate_digest(executable_identity[key])
    return {"pid": pid, "boot_id": boot_id, "start_ticks": start_ticks}


@_guard
def parse_proc_stat(data: str) -> dict:
    """Parse Linux /proc/PID/stat without a fixed offset over the whole line."""
    if not isinstance(data, str):
        _fail("invalid_input")
    text = data[:-1] if data.endswith("\n") else data
    open_at = text.find("(")
    if open_at <= 0:
        _fail("invalid_proc_stat")
    prefix = text[:open_at]
    if not prefix.endswith(" "):
        _fail("invalid_proc_stat")
    pid_text = prefix[:-1]
    if not _UINT_RE.fullmatch(pid_text):
        _fail("invalid_proc_stat")
    pid = int(pid_text)
    if pid <= 0 or pid > _PID_MAX:
        _fail("invalid_proc_stat")
    close_at = text.rfind(")")
    if close_at <= open_at or close_at + 1 >= len(text):
        _fail("invalid_proc_stat")
    if text[close_at + 1] != " ":
        _fail("invalid_proc_stat")
    fields = text[close_at + 2 :].split()
    if len(fields) < 20:
        _fail("invalid_proc_stat")
    state = fields[0]
    if len(state) != 1 or (
        state not in _LIVE_STATES and state not in _DEAD_STATES
    ):
        _fail("invalid_proc_stat")
    if state in _DEAD_STATES:
        _fail("process_not_alive")
    for offset, token in enumerate(fields[1:], start=4):
        if not _INT_RE.fullmatch(token):
            _fail("invalid_proc_stat")
        value = int(token)
        bounds = _STAT_FIELD_BOUNDS.get(offset)
        if bounds is not None and not bounds[0] <= value <= bounds[1]:
            _fail("invalid_proc_stat")
    start_ticks = int(fields[19])
    if start_ticks <= 0:
        _fail("invalid_proc_stat")
    return {"pid": pid, "start_ticks": start_ticks}


def _canonical_absolute(value, code: str) -> str:
    text = _string(value, code, nonempty=True)
    if "\0" in text or not posixpath.isabs(text) or text.startswith("//"):
        _fail(code)
    if text.endswith(_DELETED_SUFFIX):
        _fail(code)
    if posixpath.normpath(text) != text:
        _fail(code)
    return text


def _strictly_beneath(path: str, root: str) -> bool:
    try:
        return path != root and posixpath.commonpath((path, root)) == root
    except ValueError:
        return False


def _nul_arguments(data: bytes, code: str) -> list[str]:
    if not isinstance(data, bytes) or not data:
        _fail(code)
    parts = data.split(b"\0")
    if parts and parts[-1] == b"":
        parts.pop()
    if not parts or any(part == b"" for part in parts):
        _fail(code)
    arguments = []
    for part in parts:
        try:
            arguments.append(part.decode("utf-8"))
        except UnicodeDecodeError:
            raise EvidenceError(code) from None
    return arguments


def _private_codex_home(data: bytes, codex_home: str) -> None:
    if not isinstance(data, bytes) or not data:
        _fail("invalid_environment")
    entries = data.split(b"\0")
    if entries and entries[-1] == b"":
        entries.pop()
    if not entries:
        _fail("invalid_environment")
    matches = 0
    for entry in entries:
        if not entry or b"=" not in entry:
            _fail("invalid_environment")
        key, value = entry.split(b"=", 1)
        if not key:
            _fail("invalid_environment")
        if key != b"CODEX_HOME":
            continue
        matches += 1
        try:
            decoded = value.decode("utf-8")
        except UnicodeDecodeError:
            raise EvidenceError("invalid_environment") from None
        if decoded != codex_home:
            _fail("invalid_environment")
    if matches != 1:
        _fail("invalid_environment")


@_guard
def attest_process(
    record: dict,
    snapshot: dict,
    *,
    executable: str,
    codex_home: str,
    kind: str,
    rollout: str | None = None,
) -> tuple[str, int, int]:
    """Validate an in-memory process snapshot; never proves safe acquisition."""
    if kind not in {"daemon", "updater"}:
        _fail("invalid_input")
    record = _mapping(record, "invalid_pid_record")
    pid = _positive_int(record.get("pid", _MISSING), "invalid_pid_record")
    boot_id = _string(
        record.get("boot_id", _MISSING), "invalid_pid_record", nonempty=True
    )
    start_ticks = _positive_int(
        record.get("start_ticks", _MISSING),
        "invalid_pid_record",
        maximum=_TICKS_MAX,
    )

    snapshot = _mapping(snapshot, "invalid_snapshot")
    stat = _string(snapshot.get("stat", _MISSING), "invalid_snapshot")
    snapshot_boot = _string(
        snapshot.get("boot_id", _MISSING), "invalid_snapshot", nonempty=True
    )
    exe = _string(snapshot.get("exe", _MISSING), "invalid_snapshot", nonempty=True)
    cmdline = snapshot.get("cmdline", _MISSING)
    environ = snapshot.get("environ", _MISSING)
    fd_targets = snapshot.get("fd_targets", _MISSING)
    if not isinstance(cmdline, bytes) or not isinstance(environ, bytes):
        _fail("invalid_snapshot")
    if not isinstance(fd_targets, list):
        _fail("invalid_snapshot")
    for target in fd_targets:
        target = _string(target, "invalid_snapshot")
        if "\0" in target or target.endswith(_DELETED_SUFFIX):
            _fail("invalid_snapshot")

    parsed = parse_proc_stat(stat)
    if parsed["pid"] != pid or parsed["start_ticks"] != start_ticks:
        _fail("identity_mismatch")
    if snapshot_boot != boot_id:
        _fail("identity_mismatch")

    executable = _canonical_absolute(executable, "invalid_executable")
    codex_home = _canonical_absolute(codex_home, "invalid_codex_home")
    packages = posixpath.join(codex_home, "packages")
    if not _strictly_beneath(executable, packages):
        _fail("executable_containment")
    if exe.endswith(_DELETED_SUFFIX) or exe != executable:
        _fail("executable_mismatch")

    arguments = _nul_arguments(cmdline, "invalid_cmdline")
    expected_args = DAEMON_ARGS if kind == "daemon" else UPDATER_ARGS
    if arguments[0] != executable:
        _fail("cmdline_mismatch")
    if tuple(arguments[1:]) != expected_args:
        _fail("cmdline_mismatch")

    _private_codex_home(environ, codex_home)

    if rollout is not None:
        if kind != "daemon":
            _fail("invalid_rollout")
        rollout = _canonical_absolute(rollout, "invalid_rollout")
        sessions = posixpath.join(codex_home, "sessions")
        if not _strictly_beneath(rollout, sessions):
            _fail("rollout_containment")
        if rollout not in fd_targets:
            _fail("rollout_not_open")

    return boot_id, pid, start_ticks
