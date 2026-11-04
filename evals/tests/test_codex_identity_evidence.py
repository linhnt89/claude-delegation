"""Adversarial tests for the pure S146 rollout/PID/proc evidence readers.

These tests exercise in-memory snapshots and lead-owned fixtures only. They do not
start processes, open sockets, provision Codex or claim a live S146 result. A passing
suite proves parser behavior, not trustworthy acquisition or serving continuity.
"""

from __future__ import annotations

import copy
import json
import os
import pathlib
import sys
import unittest
from unittest import mock

sys.path.insert(
    0,
    os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"),
)

import codex_identity_evidence as evidence  # noqa: E402

FIXTURES = (
    pathlib.Path(__file__).resolve().parents[1]
    / "lib"
    / "fixtures"
    / "codex-identity"
)
PROMPT = "Reply with LOCAL_STUB_OK. Do not use tools."
REPLY = "LOCAL_STUB_OK"
SESSION_TIMESTAMP = "2026-11-04T05:59:39.708Z"
LAUNCHED = "2026-11-04T05:59:00+00:00"
OBSERVED = "2026-11-04T06:00:00+00:00"
CWD = "/RUN/checkout"
CODEX_HOME = "/RUN/home/.codex"
EXECUTABLE = (
    CODEX_HOME
    + "/packages/app-server-daemon/releases/0.166.0-x86_64-unknown-linux-musl/bin/codex"
)
ROLLOUT = CODEX_HOME + "/sessions/2026/11/04/rollout-thread-C.jsonl"


def fixture_bytes(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def fixture_json(name: str) -> dict:
    return json.loads(fixture_bytes(name))


def fixture_records(name: str) -> list[dict]:
    return [
        json.loads(line)
        for line in fixture_bytes(name).splitlines()
        if line.strip()
    ]


def record(record_type: str, payload: dict) -> dict:
    return {
        "timestamp": "2026-11-04T05:59:39.728Z",
        "type": record_type,
        "payload": payload,
    }


def session(**overrides) -> dict:
    payload = {
        "session_id": "thread-A",
        "id": "thread-A",
        "timestamp": SESSION_TIMESTAMP,
        "cwd": CWD,
        "cli_version": "0.166.0",
        "model_provider": "local",
    }
    payload.update(overrides)
    return record("session_meta", payload)


def started(turn: str = "turn-A", root: str = "turn-A") -> dict:
    return record(
        "event_msg",
        {
            "type": "task_started",
            "turn_id": turn,
            "root_turn_id": root,
            "turn_attribution": {"turn_id": turn, "root_turn_id": root},
        },
    )


def user(text: str = PROMPT, *, item_type: str = "input_text") -> dict:
    return record(
        "response_item",
        {"type": "message", "role": "user", "content": [{"type": item_type, "text": text}]},
    )


def assistant(text: str = REPLY) -> dict:
    return record(
        "response_item",
        {
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": text}],
        },
    )


def developer(text: str = "context") -> dict:
    return record(
        "response_item",
        {
            "type": "message",
            "role": "developer",
            "content": [{"type": "input_text", "text": text}],
        },
    )


def reasoning() -> dict:
    return record("response_item", {"type": "reasoning", "summary": []})


def event(event_type: str, **payload) -> dict:
    body = {"type": event_type}
    body.update(payload)
    return record("event_msg", body)


def complete(
    turn: str = "turn-A", root: str = "turn-A", last: str = REPLY
) -> dict:
    return event(
        "task_complete",
        turn_id=turn,
        root_turn_id=root,
        last_agent_message=last,
    )


def lines(records: list[dict]) -> bytes:
    return b"".join(
        json.dumps(item, separators=(",", ":")).encode("utf-8") + b"\n"
        for item in records
    )


def minimal() -> list[dict]:
    return [session(), started(), user(), assistant(), complete()]


def pid_fixture(name: str) -> tuple[dict, dict]:
    entry = fixture_json("processes.json")[name]
    return entry["record"], entry["proc"]


def stat_with_field(field: int, value: str) -> str:
    """Return the real daemon stat line with one proc(5) field replaced (4..52)."""
    _, proc = pid_fixture("daemon.pid")
    prefix, suffix = proc["stat"].rsplit(") ", 1)
    fields = suffix.split()
    fields[field - 3] = value
    return prefix + ") " + " ".join(fields) + "\n"


def parsed_pid(name: str, kind: str) -> dict:
    record_value, _ = pid_fixture(name)
    return evidence.parse_pid_record(
        json.dumps(record_value).encode("utf-8"), kind=kind
    )


def snapshot(name: str, *, fd_targets=None, environ=None, **overrides) -> dict:
    _, proc = pid_fixture(name)
    if environ is None:
        environ = ("CODEX_HOME=" + proc["codex_home"] + "\0PATH=/usr/bin\0").encode()
    if fd_targets is None:
        fd_targets = []
    result = {
        "stat": proc["stat"],
        "boot_id": proc["boot_id"],
        "exe": proc["exe"],
        "cmdline": proc["cmdline"].encode("utf-8"),
        "environ": environ,
        "fd_targets": list(fd_targets),
    }
    result.update(overrides)
    return result


class RolloutTests(unittest.TestCase):
    def test_projected_a_rollout(self):
        result = evidence.parse_rollout(
            fixture_bytes("rollout-a.jsonl"), prompt=PROMPT, reply=REPLY
        )
        self.assertEqual(
            result,
            {
                "id": "thread-A",
                "cwd": CWD,
                "cli_version": "0.166.0",
                "model_provider": "local",
                "session_timestamp": SESSION_TIMESTAMP,
                "turn_id": "turn-A",
                "prompt": PROMPT,
                "reply": REPLY,
            },
        )

    def test_projected_b_rollout_is_distinct(self):
        result = evidence.parse_rollout(
            fixture_bytes("rollout-b.jsonl"), prompt=PROMPT, reply=REPLY
        )
        self.assertEqual(result["id"], "thread-B")
        self.assertEqual(result["turn_id"], "turn-B")

    def test_full_real_record_order_and_tolerated_duplicates(self):
        result = evidence.parse_rollout(
            fixture_bytes("rollout-a-full.jsonl"), prompt=PROMPT, reply=REPLY
        )
        self.assertEqual(result["id"], "thread-A")
        self.assertEqual(result["turn_id"], "turn-A")

    def test_context_users_before_prompt_are_permitted(self):
        records = minimal()
        records[2:2] = [user("context one"), user("context two")]
        self.assertIsNotNone(evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY))

    def test_unrelated_tolerated_records_do_not_count_as_messages(self):
        records = minimal()
        records[1:1] = [
            record("world_state", {"full": True}),
            record("turn_context", {"turn_id": "turn-A"}),
            record("token_usage_record", {"usage": {"input_tokens": 1}}),
            event("thread_settings_applied", thread_id="thread-A"),
            event(
                "user_message",
                message="not the prompt",
                text="duplicate event user",
            ),
            event(
                "agent_message",
                message="not the reply",
                text="duplicate event assistant",
            ),
            event("item_completed", item={"type": "UserMessage", "content": []}),
            event("token_count", info={}),
            reasoning(),
            developer("developer context"),
        ]
        self.assertIsNotNone(evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY))

    def test_extra_unrelated_record_fields_are_tolerated(self):
        records = minimal()
        records[0]["extra"] = {"anything": [1, True, None]}
        records[1]["payload"]["extra_payload_field"] = 3
        records[2]["payload"]["content"][0]["extra"] = "ignored"
        self.assertIsNotNone(evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY))

    def test_completed_turn_followed_by_incomplete_tail(self):
        data = lines(minimal()) + b'{"timestamp":"2026-11-04T05:59:40.000Z","type":"event_'
        self.assertIsNotNone(evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY))

    def test_completed_turn_followed_by_complete_unrelated_record(self):
        data = lines(minimal() + [record("world_state", {"full": False})])
        self.assertIsNotNone(evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY))

    def test_empty_and_unterminated_evidence_is_pending(self):
        for data in (b"", b'{"type":', lines(minimal())[:-1]):
            with self.subTest(data=data):
                self.assertIsNone(evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY))

    def test_session_meta_only_and_started_only_are_pending(self):
        for records in ([session()], [session(), started()], [session(), started(), user()]):
            with self.subTest(records=len(records)):
                self.assertIsNone(
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
                )

    def test_prompt_and_reply_without_completion_are_pending(self):
        self.assertIsNone(
            evidence.parse_rollout(
                lines([session(), started(), user(), assistant()]),
                prompt=PROMPT,
                reply=REPLY,
            )
        )

    def test_complete_invalid_json_line_raises(self):
        data = lines([session()]) + b"not json\n"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY)

    def test_complete_invalid_utf8_line_raises_and_tail_is_ignored(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines([session()]) + b"\xff\n", prompt=PROMPT, reply=REPLY)
        self.assertIsNotNone(
            evidence.parse_rollout(
                lines(minimal()) + b"\xff", prompt=PROMPT, reply=REPLY
            )
        )

    def test_blank_line_is_malformed(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines([session()]) + b"\n", prompt=PROMPT, reply=REPLY)

    def test_top_level_record_must_be_object(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(b"[]\n", prompt=PROMPT, reply=REPLY)

    def test_duplicate_json_keys_raise(self):
        data = (
            b'{"type":"session_meta","type":"session_meta","payload":{}}\n'
        )
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY)

    def test_missing_session_meta_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([started(), user(), assistant(), complete()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_first_complete_record_must_be_session_meta(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([record("world_state", {"full": True}), *minimal()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_duplicate_session_meta_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([session(), session(), started(), user(), assistant(), complete()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_session_id_and_id_must_agree(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([session(session_id="thread-other"), *minimal()[1:]]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_session_id_only_is_supported(self):
        meta = session()
        meta["payload"].pop("id")
        self.assertEqual(
            evidence.parse_rollout(
                lines([meta, *minimal()[1:]]), prompt=PROMPT, reply=REPLY
            )["id"],
            "thread-A",
        )

    def test_session_metadata_requires_an_id_and_other_fields(self):
        for field in ("timestamp", "cwd", "cli_version", "model_provider"):
            with self.subTest(field=field):
                meta = session()
                meta["payload"].pop(field)
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(
                        lines([meta, *minimal()[1:]]),
                        prompt=PROMPT,
                        reply=REPLY,
                    )
        meta = session()
        meta["payload"].pop("id")
        meta["payload"].pop("session_id")
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([meta, *minimal()[1:]]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_naive_or_nonstring_session_timestamp_raises(self):
        for value in ("2026-11-04T05:59:39", 123):
            with self.subTest(value=value):
                meta = session(timestamp=value)
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(
                        lines([meta, *minimal()[1:]]),
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_session_meta_after_task_started_is_invalid_ordering(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([started(), session(), user(), assistant(), complete()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_additional_user_after_prompt_raises(self):
        for extra_index in (3, 5):
            with self.subTest(extra_index=extra_index):
                records = minimal()
                records.insert(extra_index, user("second user input"))
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_repeated_prompt_raises(self):
        records = [session(), started(), user("context"), user(PROMPT), user(PROMPT), assistant(), complete()]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_additional_assistant_message_raises(self):
        records = [session(), started(), user(), assistant(), assistant(), complete()]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_second_started_turn_raises(self):
        for index in (2, 5):
            with self.subTest(index=index):
                records = minimal()
                records.insert(index, started("turn-B", "turn-B"))
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_second_completion_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines(minimal() + [complete()]), prompt=PROMPT, reply=REPLY
            )

    def test_assistant_before_prompt_raises(self):
        records = [session(), started(), assistant(), user(), complete()]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_completed_turn_without_started_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([session(), user(), assistant(), complete()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_wrong_assistant_reply_is_not_success_or_pending(self):
        records = [session(), started(), user(), assistant("WRONG"), complete()]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_wrong_completion_reply_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([session(), started(), user(), assistant(), complete(last="WRONG")]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_mismatched_turn_and_root_turn_raise(self):
        for pair in (("turn-B", "turn-A"), ("turn-A", "turn-B")):
            with self.subTest(pair=pair):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(
                        lines(
                            [
                                session(),
                                started(),
                                user(),
                                assistant(),
                                complete(*pair),
                            ]
                        ),
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_nested_turn_attribution_must_agree(self):
        records = minimal()
        records[1]["payload"]["turn_attribution"]["root_turn_id"] = "turn-B"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_completion_before_expected_messages_raises(self):
        records = [session(), started(), user(), complete(), assistant()]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_invalid_turn_field_types_raise(self):
        for field in ("turn_id", "root_turn_id"):
            with self.subTest(field=field):
                records = minimal()
                records[1]["payload"][field] = 7
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_invalid_message_content_raises(self):
        records = minimal()
        records[2]["payload"]["content"] = "not a list"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_unknown_record_and_event_types_raise(self):
        for extra in (
            record("future_record", {"type": "x"}),
            event("turn_complete", turn_id="turn-A"),
        ):
            with self.subTest(extra=extra):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(
                        lines([session(), extra, started(), user(), assistant(), complete()]),
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_unknown_response_item_type_raises(self):
        extra = record("response_item", {"type": "function_call", "name": "x"})
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(
                lines([session(), extra, started(), user(), assistant(), complete()]),
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_non_bytes_and_non_string_arguments_raise(self):
        for data, prompt, reply in (
            ("text", PROMPT, REPLY),
            (lines(minimal()), 1, REPLY),
            (lines(minimal()), PROMPT, None),
        ):
            with self.subTest(data=type(data)):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(data, prompt=prompt, reply=reply)

    def test_parser_does_not_mutate_its_inputs(self):
        data = lines(minimal())
        before = bytes(data)
        first = evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY)
        second = evidence.parse_rollout(data, prompt=PROMPT, reply=REPLY)
        self.assertEqual(first, second)
        self.assertEqual(data, before)


class BindLaunchTests(unittest.TestCase):
    def bind(self, rollouts, before=frozenset({"old.jsonl"})):
        return evidence.bind_launch(
            rollouts,
            before,
            cwd=CWD,
            launched_at=LAUNCHED,
            observed_at=OBSERVED,
            prompt=PROMPT,
            reply=REPLY,
        )

    def test_one_new_candidate_binds(self):
        result = self.bind(
            {"old.jsonl": b"", "new.jsonl": fixture_bytes("rollout-a.jsonl")}
        )
        self.assertEqual(result["id"], "thread-A")

    def test_no_new_candidate_is_pending(self):
        self.assertIsNone(self.bind({"old.jsonl": b""}))

    def test_disappeared_inventory_entry_is_an_error(self):
        with self.assertRaises(evidence.EvidenceError):
            self.bind({"new.jsonl": fixture_bytes("rollout-a.jsonl")})

    def test_multiple_new_candidates_are_ambiguous(self):
        for second in (
            b"",
            b'{"timestamp":',
            fixture_bytes("rollout-b.jsonl"),
            lines([session(cwd="/other")]),
        ):
            with self.subTest(second=second[:20]):
                with self.assertRaises(evidence.EvidenceError):
                    self.bind(
                        {
                            "old.jsonl": b"",
                            "a.jsonl": fixture_bytes("rollout-a.jsonl"),
                            "b.jsonl": second,
                        }
                    )

    def test_metadata_mismatches_raise(self):
        cases = (
            {"cwd": "/other"},
            {"cli_version": "0.165.0"},
            {"model_provider": "openai"},
        )
        for overrides in cases:
            with self.subTest(overrides=overrides):
                data = lines([session(**overrides), *minimal()[1:]])
                with self.assertRaises(evidence.EvidenceError):
                    self.bind({"old.jsonl": b"", "new.jsonl": data})

    def test_metadata_timestamp_outside_window_raises(self):
        for timestamp in ("2026-11-04T05:58:59Z", "2026-11-04T06:00:01Z"):
            with self.subTest(timestamp=timestamp):
                data = lines([session(timestamp=timestamp), *minimal()[1:]])
                with self.assertRaises(evidence.EvidenceError):
                    self.bind({"old.jsonl": b"", "new.jsonl": data})

    def test_metadata_window_is_inclusive(self):
        for launched, observed in (
            (SESSION_TIMESTAMP, SESSION_TIMESTAMP),
            ("2026-11-04T05:59:39.708+00:00", "2026-11-04T05:59:39.708Z"),
        ):
            with self.subTest(launched=launched):
                data = fixture_bytes("rollout-a.jsonl")
                self.assertEqual(
                    evidence.bind_launch(
                        {"old.jsonl": b"", "new.jsonl": data},
                        frozenset({"old.jsonl"}),
                        cwd=CWD,
                        launched_at=launched,
                        observed_at=observed,
                        prompt=PROMPT,
                        reply=REPLY,
                    )["id"],
                    "thread-A",
                )

    def test_launch_after_observation_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.bind_launch(
                {"old.jsonl": b"", "new.jsonl": fixture_bytes("rollout-a.jsonl")},
                frozenset({"old.jsonl"}),
                cwd=CWD,
                launched_at=OBSERVED,
                observed_at=LAUNCHED,
                prompt=PROMPT,
                reply=REPLY,
            )

    def test_naive_or_invalid_timestamps_raise(self):
        for launched, observed in (
            ("2026-11-04T05:59:00", OBSERVED),
            (LAUNCHED, "2026-11-04T06:00:00"),
            ("not-a-time", OBSERVED),
        ):
            with self.subTest(launched=launched):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        {"old.jsonl": b"", "new.jsonl": fixture_bytes("rollout-a.jsonl")},
                        frozenset({"old.jsonl"}),
                        cwd=CWD,
                        launched_at=launched,
                        observed_at=observed,
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_partial_first_line_is_pending(self):
        self.assertIsNone(
            self.bind(
                {
                    "old.jsonl": b"",
                    "new.jsonl": b'{"timestamp":"2026-11-04T05:59:39.708Z","type":"session_',
                }
            )
        )

    def test_valid_metadata_without_complete_turn_is_pending(self):
        for data in (
            lines([session()]),
            lines([session(), started()]),
            lines([session(), started(), user()]),
        ):
            with self.subTest(records=data.count(b"\n")):
                self.assertIsNone(
                    self.bind({"old.jsonl": b"", "new.jsonl": data})
                )

    def test_completed_turn_with_incomplete_tail_binds(self):
        data = fixture_bytes("rollout-a.jsonl") + b'{"timestamp":"2026-11-04T06:00:00Z"'
        self.assertEqual(
            self.bind({"old.jsonl": b"", "new.jsonl": data})["id"], "thread-A"
        )

    def test_complete_malformed_line_after_metadata_raises(self):
        data = lines([session()]) + b"not-json\n"
        with self.assertRaises(evidence.EvidenceError):
            self.bind({"old.jsonl": b"", "new.jsonl": data})

    def test_missing_metadata_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            self.bind(
                {
                    "old.jsonl": b"",
                    "new.jsonl": lines([started(), user(), assistant(), complete()]),
                }
            )

    def test_reused_old_path_is_not_a_new_candidate(self):
        path = "reused.jsonl"
        self.assertIsNone(
            evidence.bind_launch(
                {path: fixture_bytes("rollout-a.jsonl")},
                frozenset({path}),
                cwd=CWD,
                launched_at=LAUNCHED,
                observed_at=OBSERVED,
                prompt=PROMPT,
                reply=REPLY,
            )
        )

    def test_ambiguous_candidate_turn_raises(self):
        records = minimal()
        records.insert(2, started("turn-B", "turn-B"))
        with self.assertRaises(evidence.EvidenceError):
            self.bind(
                {"old.jsonl": b"", "new.jsonl": lines(records)}
            )

    def test_invalid_rollout_inventory_types_raise(self):
        for rollouts, before in (
            ({"old.jsonl": b"", "new.jsonl": "text"}, frozenset({"old.jsonl"})),
            ({b"old.jsonl": b""}, frozenset()),
            ({"old.jsonl": b""}, ["old.jsonl"]),
            ({"old.jsonl": b""}, frozenset({1})),
        ):
            with self.subTest(rollouts=rollouts):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        rollouts,
                        before,
                        cwd=CWD,
                        launched_at=LAUNCHED,
                        observed_at=OBSERVED,
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_caller_inventory_is_not_mutated(self):
        rollouts = {
            "old.jsonl": b"",
            "new.jsonl": fixture_bytes("rollout-a.jsonl"),
        }
        before_paths = frozenset({"old.jsonl"})
        rollouts_before = dict(rollouts)
        before_before = set(before_paths)
        self.bind(rollouts, before_paths)
        self.assertEqual(rollouts, rollouts_before)
        self.assertEqual(before_paths, before_before)


class PidRecordTests(unittest.TestCase):
    def test_real_daemon_and_updater_records(self):
        daemon, _ = pid_fixture("daemon.pid")
        updater, _ = pid_fixture("daemon-updater.pid")
        self.assertEqual(
            evidence.parse_pid_record(json.dumps(daemon).encode(), kind="daemon"),
            {
                "pid": 4001,
                "boot_id": "00000000-0000-4000-8000-000000000001",
                "start_ticks": 14001,
            },
        )
        self.assertEqual(
            evidence.parse_pid_record(json.dumps(updater).encode(), kind="updater"),
            {
                "pid": 4002,
                "boot_id": "00000000-0000-4000-8000-000000000001",
                "start_ticks": 14002,
            },
        )

    def test_extra_fields_and_optional_executable_identity(self):
        daemon, _ = pid_fixture("daemon.pid")
        record = copy.deepcopy(daemon)
        record["extra"] = {"ignored": True}
        record["executableIdentity"]["extra"] = 1
        self.assertIsNotNone(evidence.parse_pid_record(json.dumps(record).encode(), kind="daemon"))
        record["executableIdentity"].pop("digest")
        record["executableIdentity"].pop("path_digest")
        self.assertIsNotNone(evidence.parse_pid_record(json.dumps(record).encode(), kind="daemon"))

        missing = copy.deepcopy(daemon)
        missing.pop("executableIdentity")
        self.assertIsNotNone(
            evidence.parse_pid_record(json.dumps(missing).encode(), kind="daemon")
        )
        updater, _ = pid_fixture("daemon-updater.pid")
        self.assertIsNotNone(
            evidence.parse_pid_record(json.dumps(updater).encode(), kind="updater")
        )

    def test_invalid_kind_raises(self):
        daemon, _ = pid_fixture("daemon.pid")
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_pid_record(json.dumps(daemon).encode(), kind="server")

    def test_legacy_layouts_are_rejected(self):
        daemon, _ = pid_fixture("daemon.pid")
        legacy_object = {
            "pid": 123,
            "processStartTime": "Wed Nov  4 00:00:00 2026",
        }
        for data in (b"123\n", json.dumps(legacy_object).encode()):
            with self.subTest(data=data):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(data, kind="daemon")
        snake = copy.deepcopy(daemon)
        identity = snake["processIdentity"]
        identity["boot_id"] = identity.pop("bootId")
        identity["start_ticks"] = identity.pop("startTicks")
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_pid_record(json.dumps(snake).encode(), kind="daemon")

    def test_bool_negative_and_zero_identities_are_rejected(self):
        daemon, _ = pid_fixture("daemon.pid")
        mutations = (
            lambda value: value.__setitem__("pid", True),
            lambda value: value.__setitem__("pid", -1),
            lambda value: value.__setitem__("pid", 0),
            lambda value: value["processIdentity"].__setitem__("startTicks", False),
            lambda value: value["processIdentity"].__setitem__("startTicks", -1),
            lambda value: value["processIdentity"].__setitem__("startTicks", 0),
        )
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                value = copy.deepcopy(daemon)
                mutate(value)
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(json.dumps(value).encode(), kind="daemon")

    def test_missing_or_wrong_fields_raise(self):
        daemon, _ = pid_fixture("daemon.pid")
        cases = (
            ("pid", None),
            ("processStartTime", 123),
            ("processStartTime", ""),
            ("processIdentity", None),
            ("processIdentity", {"bootId": "", "startTicks": 1}),
            ("processIdentity", {"bootId": "b"}),
            ("processIdentity", {"startTicks": 1}),
        )
        for field, value in cases:
            with self.subTest(field=field, value=value):
                record = copy.deepcopy(daemon)
                if value is None:
                    record.pop(field)
                else:
                    record[field] = value
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(json.dumps(record).encode(), kind="daemon")

    def test_start_ticks_bounds_and_digest_arrays(self):
        daemon, _ = pid_fixture("daemon.pid")
        value = copy.deepcopy(daemon)
        value["processIdentity"]["startTicks"] = 1 << 64
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_pid_record(json.dumps(value).encode(), kind="daemon")

        for digest in ([0] * 31, [0] * 33, [0] * 31 + [256], [0] * 31 + [True]):
            with self.subTest(length=len(digest)):
                value = copy.deepcopy(daemon)
                value["executableIdentity"]["digest"] = digest
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(json.dumps(value).encode(), kind="daemon")

        value = copy.deepcopy(daemon)
        value["executableIdentity"] = []
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_pid_record(json.dumps(value).encode(), kind="daemon")

    def test_invalid_json_utf8_and_duplicate_keys_raise(self):
        for data in (
            b"not json",
            b"\xff",
            b'{"pid":1,"pid":2}',
            b"[]",
        ):
            with self.subTest(data=data):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(data, kind="daemon")


class ProcStatTests(unittest.TestCase):
    def test_real_daemon_and_updater_stats(self):
        for name in ("daemon.pid", "daemon-updater.pid"):
            with self.subTest(name=name):
                _, proc = pid_fixture(name)
                self.assertIsNotNone(evidence.parse_proc_stat(proc["stat"]))

    def test_comm_with_spaces_and_parentheses(self):
        data = "42 (worker ) with (nested) spaces) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777 0 0 0 0 0 0\n"
        self.assertEqual(
            evidence.parse_proc_stat(data), {"pid": 42, "start_ticks": 777}
        )

    def test_zombie_and_dead_states_are_rejected(self):
        base = "42 (codex) {state} 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777 0 0 0 0 0 0\n"
        for state in ("Z", "X", "x"):
            with self.subTest(state=state):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_proc_stat(base.format(state=state))

    def test_malformed_structure_is_rejected(self):
        cases = (
            "42(codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777",
            "42 (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0",
            "42 (codex) Q 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777",
            "42 (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 -1",
            "42 (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777 nope",
            "not-a-pid (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777",
        )
        for data in cases:
            with self.subTest(data=data):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_proc_stat(data)

    def test_integer_bounds_are_enforced(self):
        base = "42 (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 {ticks} 0\n"
        for ticks in ("0", str(1 << 64)):
            with self.subTest(ticks=ticks):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_proc_stat(base.format(ticks=ticks))
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_proc_stat(base.format(ticks="777").replace("42 ", f"{1 << 31} "))

    def test_newline_is_optional_and_extra_fields_are_allowed(self):
        data = "42 (codex) S 1 2 3 4 0 4194304 0 0 0 0 0 0 0 0 20 0 1 0 777 0 0 0\n"
        self.assertEqual(evidence.parse_proc_stat(data), {"pid": 42, "start_ticks": 777})
        self.assertEqual(evidence.parse_proc_stat(data[:-1]), {"pid": 42, "start_ticks": 777})

    def test_non_string_input_raises(self):
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_proc_stat(b"42 (codex)")


class AttestProcessTests(unittest.TestCase):
    def attest(self, name="daemon.pid", kind="daemon", *, record=None, snap=None, **kwargs):
        if record is None:
            record = parsed_pid(name, kind)
        if snap is None:
            snap = snapshot(name)
        return evidence.attest_process(
            record,
            snap,
            executable=EXECUTABLE,
            codex_home=CODEX_HOME,
            kind=kind,
            **kwargs,
        )

    def test_daemon_and_updater_snapshots(self):
        self.assertEqual(
            self.attest("daemon.pid", "daemon"),
            ("00000000-0000-4000-8000-000000000001", 4001, 14001),
        )
        self.assertEqual(
            self.attest("daemon-updater.pid", "updater"),
            ("00000000-0000-4000-8000-000000000001", 4002, 14002),
        )

    def test_serving_rollout_fd_snapshot(self):
        serving = fixture_json("serving-fd.json")
        proc = serving["proc"]
        record = evidence.parse_pid_record(
            json.dumps(serving["record"]).encode(), kind="daemon"
        )
        snap = {
            "stat": proc["stat"],
            "boot_id": proc["boot_id"],
            "exe": proc["exe"],
            "cmdline": proc["cmdline"].encode(),
            "environ": ("CODEX_HOME=" + proc["codex_home"] + "\0").encode(),
            "fd_targets": [fd["target"] for fd in proc["fds"]],
        }
        self.assertEqual(
            evidence.attest_process(
                record,
                snap,
                executable=EXECUTABLE,
                codex_home=CODEX_HOME,
                kind="daemon",
                rollout=serving["rollout"],
            ),
            ("00000000-0000-4000-8000-000000000002", 4101, 15001),
        )

    def test_extra_snapshot_and_record_keys_are_tolerated(self):
        record = parsed_pid("daemon.pid", "daemon")
        record["extra"] = 1
        snap = snapshot("daemon.pid")
        snap["extra"] = {"ignored": True}
        self.assertIsNotNone(self.attest(record=record, snap=snap))

    def test_pid_start_and_boot_must_agree(self):
        for field, value in (
            ("pid", 9999),
            ("start_ticks", 9999),
            ("boot_id", "00000000-0000-4000-8000-000000000099"),
        ):
            with self.subTest(field=field):
                record = parsed_pid("daemon.pid", "daemon")
                record[field] = value
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(record=record)

    def test_zombie_stat_is_not_liveness_evidence(self):
        snap = snapshot("daemon.pid")
        snap["stat"] = snap["stat"].replace(") S ", ") Z ", 1)
        with self.assertRaises(evidence.EvidenceError):
            self.attest(snap=snap)

    def test_exe_mismatch_and_deleted_suffix_raise(self):
        for exe in (EXECUTABLE + "-other", EXECUTABLE + " (deleted)"):
            with self.subTest(exe=exe):
                snap = snapshot("daemon.pid", exe=exe)
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(snap=snap)

    def test_argv_mismatch_raises(self):
        base = snapshot("daemon.pid")
        data = base["cmdline"]
        cases = (
            data.replace(b"app-server", b"other-server", 1),
            data.replace(b"--managed-daemon", b"--other", 1),
            data + b"extra\0",
            b"",
            b"\xff\0",
        )
        for cmdline in cases:
            with self.subTest(cmdline=cmdline[:30]):
                snap = dict(base, cmdline=cmdline)
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(snap=snap)

    def test_updater_mistaken_for_daemon_and_reverse(self):
        with self.assertRaises(evidence.EvidenceError):
            self.attest("daemon-updater.pid", "daemon")
        with self.assertRaises(evidence.EvidenceError):
            self.attest("daemon.pid", "updater")

    def test_private_codex_home_is_exact_and_unambiguous(self):
        base = snapshot("daemon.pid")
        cases = (
            b"",
            b"CODEX_HOME=/RUN/home/.codex-evil\0",
            b"XCODEX_HOME=/RUN/home/.codex\0",
            b"CODEX_HOME=/RUN/home/.codex\0CODEX_HOME=/RUN/home/.codex\0",
            b"CODEX_HOME=/RUN/home/.codex\0CODEX_HOME=/RUN/home/.codex-evil\0",
            b"NO_EQUALS\0",
            b"CODEX_HOME\0",
        )
        for environ in cases:
            with self.subTest(environ=environ):
                snap = dict(base, environ=environ)
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(snap=snap)

    def test_other_environment_entries_are_allowed(self):
        snap = snapshot(
            "daemon.pid",
            environ=b"PATH=/usr/bin\0CHROME=/opt/x\0CODEX_HOME=/RUN/home/.codex\0",
        )
        self.assertIsNotNone(self.attest(snap=snap))

    def test_expected_paths_must_be_canonical_absolute_and_contained(self):
        base = snapshot("daemon.pid")
        bad_executables = (
            "packages/codex",
            "/RUN/home/.codex/packages/../codex",
            "//RUN/home/.codex/packages/bin/codex",
            "/RUN/home/.codex/packages-evil/bin/codex",
            "/RUN/home/.codex/packages2/bin/codex",
            "/RUN/home/.codex/packages",
            EXECUTABLE + " (deleted)",
        )
        for executable in bad_executables:
            with self.subTest(executable=executable):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.attest_process(
                        parsed_pid("daemon.pid", "daemon"),
                        base,
                        executable=executable,
                        codex_home=CODEX_HOME,
                        kind="daemon",
                    )
        for codex_home in (
            "relative/.codex",
            CODEX_HOME + "/",
            CODEX_HOME + "/../.codex",
            "//RUN/home/.codex",
        ):
            with self.subTest(codex_home=codex_home):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.attest_process(
                        parsed_pid("daemon.pid", "daemon"),
                        base,
                        executable=EXECUTABLE,
                        codex_home=codex_home,
                        kind="daemon",
                    )

    def test_rollout_binding_rules(self):
        base = snapshot("daemon.pid", fd_targets=[ROLLOUT])
        record = parsed_pid("daemon.pid", "daemon")
        self.assertIsNotNone(
            evidence.attest_process(
                record,
                base,
                executable=EXECUTABLE,
                codex_home=CODEX_HOME,
                kind="daemon",
                rollout=ROLLOUT,
            )
        )
        for rollout in (
            "sessions/rollout.jsonl",
            CODEX_HOME + "/sessions-evil/rollout.jsonl",
            CODEX_HOME + "/sessions/../rollout.jsonl",
            CODEX_HOME + "/sessions",
            CODEX_HOME + "/sessions/rollout.jsonl (deleted)",
        ):
            with self.subTest(rollout=rollout):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.attest_process(
                        record,
                        base,
                        executable=EXECUTABLE,
                        codex_home=CODEX_HOME,
                        kind="daemon",
                        rollout=rollout,
                    )

    def test_rollout_must_be_an_exact_live_fd(self):
        for fd_targets in (
            [],
            [ROLLOUT + " (deleted)"],
            [ROLLOUT + "-other"],
            ["pipe:[123]"],
        ):
            with self.subTest(fd_targets=fd_targets):
                snap = snapshot("daemon.pid", fd_targets=fd_targets)
                with self.assertRaises(evidence.EvidenceError):
                    evidence.attest_process(
                        parsed_pid("daemon.pid", "daemon"),
                        snap,
                        executable=EXECUTABLE,
                        codex_home=CODEX_HOME,
                        kind="daemon",
                        rollout=ROLLOUT,
                    )

    def test_deleted_fd_target_is_rejected_without_rollout(self):
        snap = snapshot("daemon.pid", fd_targets=[ROLLOUT + " (deleted)"])
        with self.assertRaises(evidence.EvidenceError):
            self.attest(snap=snap)

    def test_updater_cannot_bind_rollout(self):
        with self.assertRaises(evidence.EvidenceError):
            self.attest(
                "daemon-updater.pid",
                "updater",
                snap=snapshot("daemon-updater.pid", fd_targets=[ROLLOUT]),
                rollout=ROLLOUT,
            )

    def test_snapshot_shape_types_are_validated(self):
        base = snapshot("daemon.pid")
        for key in tuple(base):
            with self.subTest(missing=key):
                snap = dict(base)
                snap.pop(key)
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(snap=snap)
        for key, value in (
            ("stat", None),
            ("boot_id", ""),
            ("exe", 1),
            ("cmdline", "text"),
            ("environ", "text"),
            ("fd_targets", "text"),
            ("fd_targets", [1]),
        ):
            with self.subTest(key=key, value=value):
                snap = dict(base, **{key: value})
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(snap=snap)

    def test_invalid_record_and_kind_raise(self):
        base = snapshot("daemon.pid")
        for field in ("pid", "boot_id", "start_ticks"):
            with self.subTest(field=field):
                record = parsed_pid("daemon.pid", "daemon")
                record.pop(field)
                with self.assertRaises(evidence.EvidenceError):
                    self.attest(record=record, snap=base)
        with self.assertRaises(evidence.EvidenceError):
            evidence.attest_process(
                parsed_pid("daemon.pid", "daemon"),
                base,
                executable=EXECUTABLE,
                codex_home=CODEX_HOME,
                kind="server",
            )

    def test_error_messages_are_redacted(self):
        try:
            self.attest(
                snap=snapshot(
                    "daemon.pid",
                    environ=b"CODEX_HOME=/wrong/private/path\0",
                )
            )
        except evidence.EvidenceError as error:
            self.assertEqual(str(error), error.code)
            self.assertNotIn("/wrong/private/path", str(error))
            self.assertNotIn(CWD, str(error))
        else:
            self.fail("expected EvidenceError")

    def test_caller_snapshots_are_not_mutated(self):
        record = parsed_pid("daemon.pid", "daemon")
        snap = snapshot("daemon.pid", fd_targets=[ROLLOUT])
        record_before = copy.deepcopy(record)
        snap_before = copy.deepcopy(snap)
        self.attest(record=record, snap=snap, rollout=ROLLOUT)
        self.assertEqual(record, record_before)
        self.assertEqual(snap, snap_before)


class NoSideEffectsTests(unittest.TestCase):
    def test_public_readers_do_not_open_files_processes_or_networks(self):
        rollout_data = fixture_bytes("rollout-a.jsonl")
        daemon_record, daemon_proc = pid_fixture("daemon.pid")
        record = evidence.parse_pid_record(
            json.dumps(daemon_record).encode(), kind="daemon"
        )
        snap = snapshot("daemon.pid", fd_targets=[ROLLOUT])
        before = copy.deepcopy(snap)

        forbidden = {
            "builtins.open": AssertionError("filesystem access"),
            "os.open": AssertionError("filesystem access"),
            "os.stat": AssertionError("filesystem access"),
            "os.lstat": AssertionError("filesystem access"),
            "os.listdir": AssertionError("filesystem access"),
            "os.scandir": AssertionError("filesystem access"),
            "os.fork": AssertionError("process access"),
            "os.system": AssertionError("process access"),
            "socket.socket": AssertionError("network access"),
            "subprocess.Popen": AssertionError("process access"),
        }
        patches = [
            mock.patch(name, side_effect=error) for name, error in forbidden.items()
        ]
        with patches[0], patches[1], patches[2], patches[3], patches[4], patches[5], patches[6], patches[7], patches[8], patches[9]:
            self.assertIsNotNone(
                evidence.parse_rollout(rollout_data, prompt=PROMPT, reply=REPLY)
            )
            self.assertIsNotNone(
                evidence.bind_launch(
                    {"old.jsonl": b"", "new.jsonl": rollout_data},
                    frozenset({"old.jsonl"}),
                    cwd=CWD,
                    launched_at=LAUNCHED,
                    observed_at=OBSERVED,
                    prompt=PROMPT,
                    reply=REPLY,
                )
            )
            self.assertIsNotNone(
                evidence.parse_pid_record(
                    json.dumps(daemon_record).encode(), kind="daemon"
                )
            )
            self.assertEqual(
                evidence.parse_proc_stat(daemon_proc["stat"]),
                {"pid": 4001, "start_ticks": 14001},
            )
            self.assertIsNotNone(
                evidence.attest_process(
                    record,
                    snap,
                    executable=EXECUTABLE,
                    codex_home=CODEX_HOME,
                    kind="daemon",
                    rollout=ROLLOUT,
                )
            )
        self.assertEqual(snap, before)


class AdversarialReviewTests(unittest.TestCase):
    def test_review_messages_from_another_turn_are_rejected(self):
        for index in (7, 10):
            with self.subTest(message_index=index):
                records = fixture_records("rollout-a-full.jsonl")
                message = records[index]
                message["payload"]["internal_chat_message_metadata_passthrough"][
                    "turn_id"
                ] = "other-turn"
                message["metadata"]["retained_source"]["id"]["turn_id"] = "other-turn"
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_review_conflicting_turn_context_is_rejected(self):
        records = fixture_records("rollout-a-full.jsonl")
        records[5]["payload"]["turn_id"] = "other-turn"
        records[5]["payload"]["root_turn_id"] = "other-turn"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_review_all_assistant_output_must_agree_with_completion(self):
        records = minimal()
        records[3]["payload"]["content"].append(
            {"type": "output_text", "text": "CONTRADICTORY_EXTRA_OUTPUT"}
        )
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_review_appended_contradictory_prompt_text_is_rejected(self):
        records = minimal()
        records[2]["payload"]["content"].append(
            {"type": "input_text", "text": "CONTRADICTORY_EXTRA_PROMPT"}
        )
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_review_non_json_constants_are_rejected(self):
        for constant in (float("nan"), float("inf"), -float("inf")):
            with self.subTest(reader="rollout", constant=constant):
                records = minimal()
                records[1]["payload"]["extra"] = constant
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
            with self.subTest(reader="pid", constant=constant):
                record_value, _ = pid_fixture("daemon.pid")
                record_value["extra"] = constant
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_pid_record(
                        json.dumps(record_value).encode(), kind="daemon"
                    )

    def test_review_parser_exception_messages_do_not_leak_input(self):
        import traceback

        marker = "/private/path/SECRET_PROMPT"
        records = minimal()
        records[0]["payload"]["timestamp"] = marker
        try:
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
        except evidence.EvidenceError as error:
            formatted = "".join(traceback.format_exception(error))
            self.assertNotIn(marker, formatted)
            self.assertNotIn("Invalid isoformat string", formatted)
        else:
            self.fail("expected EvidenceError")


class RoundThreeAdversarialTests(unittest.TestCase):
    def test_retained_source_role_must_agree_with_message_role(self):
        for index, contradictory_role in ((7, "assistant"), (10, "user")):
            with self.subTest(message_index=index):
                records = fixture_records("rollout-a-full.jsonl")
                records[index]["metadata"]["retained_source"]["id"]["role"] = (
                    contradictory_role
                )
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_unsupported_assistant_content_cannot_be_discarded(self):
        records = minimal()
        records[3]["payload"]["content"].append(
            {"type": "refusal", "refusal": "I cannot comply with that request."}
        )
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_proc_stat_nonidentity_integer_bounds_are_validated(self):
        _, proc = pid_fixture("daemon.pid")
        prefix, suffix = proc["stat"].rsplit(") ", 1)
        # Indices relative to state: 1 is field 4 (ppid), 11 field 14 (utime).
        for index, invalid_value in ((1, 1 << 100), (11, 1 << 64), (11, -1)):
            with self.subTest(field=index + 3, value=invalid_value):
                fields = suffix.split()
                fields[index] = str(invalid_value)
                invalid_stat = prefix + ") " + " ".join(fields) + "\n"
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_proc_stat(invalid_stat)

    def test_retained_source_binding_fields_must_have_types_and_agree(self):
        for value in (7, "", None):
            with self.subTest(role=value):
                records = fixture_records("rollout-a-full.jsonl")
                records[7]["metadata"]["retained_source"]["id"]["role"] = value
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

        records = fixture_records("rollout-a-full.jsonl")
        records[10]["metadata"]["retained_source"]["id"]["root_turn_id"] = "turn-B"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

        records = fixture_records("rollout-a-full.jsonl")
        records[7]["metadata"]["retained_source"]["id"]["turn_id"] = "turn-B"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_unsupported_user_content_cannot_be_discarded(self):
        for item in (
            {"type": "refusal", "refusal": "I cannot comply."},
            {"type": "input_image", "image_url": "data:image/png;base64,AA=="},
            {"type": "input_text"},
            {"text": PROMPT},
        ):
            with self.subTest(item=item):
                records = minimal()
                records[2]["payload"]["content"].append(copy.deepcopy(item))
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_selected_text_items_require_their_text_value(self):
        records = minimal()
        records[3]["payload"]["content"] = [{"type": "output_text"}]
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_developer_and_reasoning_records_remain_tolerated(self):
        records = minimal()
        records[1:1] = [
            reasoning(),
            record(
                "response_item",
                {
                    "type": "message",
                    "role": "developer",
                    "content": [
                        {"type": "input_image", "image_url": "img"},
                        {"type": "input_text", "text": "context"},
                    ],
                },
            ),
        ]
        self.assertIsNotNone(
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
        )

    def test_stat_signed_fields_keep_legitimate_negative_values(self):
        for field in (8, 16, 17, 39, 44):
            with self.subTest(field=field):
                self.assertEqual(
                    evidence.parse_proc_stat(stat_with_field(field, "-1")),
                    {"pid": 4001, "start_ticks": 14001},
                )

    def test_stat_unsigned_fields_reject_negative_and_overflow(self):
        for field, value in (
            (4, "-1"),
            (4, str(1 << 31)),
            (9, "-1"),
            (9, str(1 << 32)),
            (10, "-1"),
            (14, "-1"),
            (14, str(1 << 64)),
            (22, "-1"),
            (25, "-1"),
            (25, str(1 << 64)),
            (42, str(1 << 64)),
        ):
            with self.subTest(field=field, value=value):
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_proc_stat(stat_with_field(field, value))

    def test_stat_unsigned_field_maxima_are_supported(self):
        for field, value in (
            (9, str((1 << 32) - 1)),
            (25, str((1 << 64) - 1)),
            (42, str((1 << 64) - 1)),
        ):
            with self.subTest(field=field):
                self.assertIsNotNone(
                    evidence.parse_proc_stat(stat_with_field(field, value))
                )


class IndependentBoundaryReviewTests(unittest.TestCase):
    def test_tolerated_records_cannot_contradict_session_or_turn_binding(self):
        # These are existing binding fields in the full real-format fixture.
        # Valid duplicates must remain tolerated, but foreign IDs are not duplicates
        # belonging to the independently attested session/turn.
        for index, field, value in (
            (6, "thread_id", "thread-B"),
            (8, "thread_id", "thread-B"),
            (8, "turn_id", "turn-B"),
            (9, "turn_id", "turn-B"),
            (11, "thread_id", "thread-B"),
            (11, "session_id", "thread-B"),
            (11, "turn_id", "turn-B"),
            (11, "root_turn_id", "turn-B"),
        ):
            with self.subTest(record_index=index, field=field):
                records = fixture_records("rollout-a-full.jsonl")
                records[index]["payload"][field] = value
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_launch_binding_cannot_discard_submicrosecond_time_differences(self):
        for candidate, launched, observed in (
            # The sole candidate is later than the observation.
            ("7080009", "7080000", "7080001"),
            # The sole candidate predates launch.
            ("7080001", "7080009", "7080010"),
            # Wall time reversed between launch and observation.
            ("7080001", "7080009", "7080001"),
        ):
            with self.subTest(candidate=candidate, launched=launched, observed=observed):
                records = minimal()
                records[0]["payload"]["timestamp"] = (
                    "2026-11-04T05:59:39." + candidate + "Z"
                )
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        {"new.jsonl": lines(records)},
                        frozenset(),
                        cwd=CWD,
                        launched_at="2026-11-04T05:59:39." + launched + "Z",
                        observed_at="2026-11-04T05:59:39." + observed + "Z",
                        prompt=PROMPT,
                        reply=REPLY,
                    )


class RoundFourBindingRegressionTests(unittest.TestCase):
    def test_every_tolerated_record_checks_all_binding_fields(self):
        for name in ("world_state", "token_usage_record"):
            for field in ("thread_id", "session_id", "turn_id", "root_turn_id"):
                with self.subTest(record=name, field=field):
                    extra = record(name, {field: "foreign"})
                    records = [session(), started(), extra, user(), assistant(), complete()]
                    with self.assertRaises(evidence.EvidenceError):
                        evidence.parse_rollout(
                            lines(records), prompt=PROMPT, reply=REPLY
                        )
        for name in (
            "thread_settings_applied",
            "item_completed",
            "user_message",
            "agent_message",
            "token_count",
        ):
            for field in ("thread_id", "session_id", "turn_id", "root_turn_id"):
                with self.subTest(record=name, field=field):
                    extra = event(name, **{field: "foreign"})
                    records = [session(), started(), extra, user(), assistant(), complete()]
                    with self.assertRaises(evidence.EvidenceError):
                        evidence.parse_rollout(
                            lines(records), prompt=PROMPT, reply=REPLY
                        )

    def test_matching_tolerated_binding_fields_remain_tolerated(self):
        for name, builder in (
            ("thread_settings_applied", event),
            ("item_completed", event),
            ("user_message", event),
            ("agent_message", event),
            ("token_count", event),
        ):
            with self.subTest(record=name):
                records = minimal()
                records[1:1] = [
                    builder(
                        name,
                        thread_id="thread-A",
                        session_id="thread-A",
                        turn_id="turn-A",
                        root_turn_id="turn-A",
                    )
                ]
                self.assertIsNotNone(
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
                )

    def test_message_item_ids_are_not_conversation_identities(self):
        records = fixture_records("rollout-a-full.jsonl")
        records[8]["payload"]["item"]["id"] = "thread-B"
        records[9]["payload"]["item"]["id"] = "turn-B"
        self.assertIsNotNone(
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
        )

        records = minimal()
        records[1:1] = [
            event(
                "user_message",
                thread_id="thread-A",
                turn_id="turn-A",
                message={"id": "thread-B"},
            )
        ]
        self.assertIsNotNone(
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)
        )

    def test_nested_item_and_retained_source_bindings_are_checked(self):
        records = fixture_records("rollout-a-full.jsonl")
        records[8]["payload"]["item"]["turn_id"] = "turn-B"
        with self.assertRaises(evidence.EvidenceError):
            evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

        for field in ("thread_id", "session_id"):
            with self.subTest(field=field):
                records = fixture_records("rollout-a-full.jsonl")
                records[7]["metadata"]["retained_source"]["id"][field] = "thread-B"
                with self.assertRaises(evidence.EvidenceError):
                    evidence.parse_rollout(lines(records), prompt=PROMPT, reply=REPLY)

    def test_six_digit_fraction_is_exact_and_longer_fractions_are_rejected(self):
        records = minimal()
        records[0]["payload"]["timestamp"] = "2026-11-04T05:59:39.708001Z"
        self.assertIsNotNone(
            evidence.bind_launch(
                {"new.jsonl": lines(records)},
                frozenset(),
                cwd=CWD,
                launched_at="2026-11-04T05:59:39.708001Z",
                observed_at="2026-11-04T05:59:39.708001Z",
                prompt=PROMPT,
                reply=REPLY,
            )
        )
        records[0]["payload"]["timestamp"] = "2026-11-04T05:59:39.7080010Z"
        with self.assertRaises(evidence.EvidenceError):
            evidence.bind_launch(
                {"new.jsonl": lines(records)},
                frozenset(),
                cwd=CWD,
                launched_at="2026-11-04T05:59:39.7080010Z",
                observed_at="2026-11-04T05:59:39.7080010Z",
                prompt=PROMPT,
                reply=REPLY,
            )



class TimestampPrecisionReviewTests(unittest.TestCase):
    def test_earlier_decimal_cannot_hide_timestamp_precision_loss(self):
        cases = (
            (
                "candidate_after_observation",
                "2026-11-04T05:59:39.708001+00:00:01.0000001",
                "2026-11-04T05:59:39.708001+00:00:01.0000009",
                "2026-11-04T05:59:39.708001+00:00:01.0000009",
            ),
            (
                "reversed_boundaries",
                "2026-11-04T05:59:39.708001+00:00:01.0000001",
                "2026-11-04T05:59:39.708001+00:00:01.0000001",
                "2026-11-04T05:59:39.708001+00:00:01.0000009",
            ),
        )
        for name, candidate, launched, observed in cases:
            with self.subTest(case=name):
                records = minimal()
                records[0]["payload"]["timestamp"] = candidate
                # Reject unsupported shapes/precision rather than letting
                # fromisoformat truncate unequal instants into a valid binding.
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        {"new.jsonl": lines(records)},
                        frozenset(),
                        cwd=CWD,
                        launched_at=launched,
                        observed_at=observed,
                        prompt=PROMPT,
                        reply=REPLY,
                    )

    def test_supported_closed_timestamp_subset_binds_exactly(self):
        cases = (
            ("2026-11-04T05:59:39Z", "2026-11-04T05:59:39Z"),
            ("2026-11-04T05:59:39.7Z", "2026-11-04T05:59:39.7Z"),
            ("2026-11-04T05:59:39.708001Z", "2026-11-04T05:59:39.708001Z"),
            (
                "2026-11-04T05:59:39.708001+00:00",
                "2026-11-04T05:59:39.708001Z",
            ),
            (
                "2026-11-04T06:29:39.708001+00:30",
                "2026-11-04T05:59:39.708001Z",
            ),
            (
                "2026-11-04T05:29:39.708001-00:30",
                "2026-11-04T05:59:39.708001Z",
            ),
        )
        for candidate, boundary in cases:
            with self.subTest(candidate=candidate):
                records = minimal()
                records[0]["payload"]["timestamp"] = candidate
                result = evidence.bind_launch(
                    {"new.jsonl": lines(records)},
                    frozenset(),
                    cwd=CWD,
                    launched_at=boundary,
                    observed_at=boundary,
                    prompt=PROMPT,
                    reply=REPLY,
                )
                self.assertEqual(result["id"], "thread-A")

    def test_unsupported_timestamp_extensions_are_rejected(self):
        unsupported = (
            # Offset seconds and fractional offsets previously reached datetime.
            "2026-11-04T05:59:39+00:00:01",
            "2026-11-04T05:59:39+00:00.1",
            "2026-11-04T05:59:39.708001+00:00:01.0000001",
            # Longer than microsecond precision, anywhere.
            "2026-11-04T05:59:39.7080010Z",
            "2026-11-04T05:59:39.7080010+00:00",
            # Missing/alternative separators, timezone or seconds.
            "2026-11-04T05:59:39",
            "2026-11-04 05:59:39Z",
            "2026-11-04T05:59Z",
            "2026-11-04T05:59:39,708Z",
            "2026-11-04T05:59:39+0000",
            "2026-11-04T05:59:39z",
            "2026-11-04T05:59:39Z\n",
        )
        for timestamp in unsupported:
            with self.subTest(timestamp=timestamp):
                records = minimal()
                records[0]["payload"]["timestamp"] = timestamp
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        {"new.jsonl": lines(records)},
                        frozenset(),
                        cwd=CWD,
                        launched_at=LAUNCHED,
                        observed_at=OBSERVED,
                        prompt=PROMPT,
                        reply=REPLY,
                    )
                with self.assertRaises(evidence.EvidenceError):
                    evidence.bind_launch(
                        {"old.jsonl": b"", "new.jsonl": lines(minimal())},
                        frozenset({"old.jsonl"}),
                        cwd=CWD,
                        launched_at=timestamp,
                        observed_at=OBSERVED,
                        prompt=PROMPT,
                        reply=REPLY,
                    )


if __name__ == "__main__":
    unittest.main()
