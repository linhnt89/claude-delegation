"""Scenario assertions shared by S071--S074, S076 and P020."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import sys
import time
from typing import Any

sys.path.insert(0, os.environ["EVALS_LIB"])
from kit import d, dhome, seed, spec  # noqa: E402
from vendors import VendorServer  # noqa: E402

HOME = Path(os.environ["HOME"])
WORK = Path(os.environ["EVALS_WORK"])


class CheckError(Exception):
    """The check could not construct or inspect its own fixture."""


CURRENT_CASE: str | None = None


def fail(message: str) -> None:
    if CURRENT_CASE:
        print(f"smallest failing case: {CURRENT_CASE}", flush=True)
    print("FAIL: " + " ".join(str(message).split())[:700], flush=True)
    raise SystemExit(1)


def check_error(message: str) -> None:
    print("CHECK ERROR: " + " ".join(str(message).split())[:700], flush=True)
    raise SystemExit(3)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def setup_require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckError(message)


def run(*args: str) -> tuple[int, str]:
    return d(*args)


def iso_after(seconds: float) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()


def epoch_after(seconds: float) -> int:
    return int(time.time() + seconds)


def claude_body(five_used: float = 42, week_used: float = 87,
                five_reset: float = 4 * 3600, week_reset: float = 3600) -> dict[str, Any]:
    return {
        "five_hour": {"utilization": five_used, "resets_at": iso_after(five_reset)},
        "seven_day": {"utilization": week_used, "resets_at": iso_after(week_reset)},
    }


def codex_body(used: float = 25, reset: float = 4 * 3600,
               window_seconds: int = 18_000,
               secondary: tuple[float, float, int] | None = None) -> dict[str, Any]:
    rate_limit: dict[str, Any] = {
        "allowed": True,
        "limit_reached": False,
        "primary_window": {
            "used_percent": used,
            "limit_window_seconds": window_seconds,
            "reset_at": epoch_after(reset),
        },
    }
    if secondary is not None:
        secondary_used, secondary_reset, secondary_seconds = secondary
        rate_limit["secondary_window"] = {
            "used_percent": secondary_used,
            "limit_window_seconds": secondary_seconds,
            "reset_at": epoch_after(secondary_reset),
        }
    return {"plan_type": "plus", "rate_limit": rate_limit}


def copilot_body(used: float = 10, reset: float = 86_400) -> dict[str, Any]:
    remaining = 100 - used
    return {
        "copilot_plan": "individual",
        "quota_reset_date_utc": iso_after(reset),
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 100,
                "remaining": remaining,
                "percent_remaining": remaining,
            },
        },
    }


def route(status: int = 200, body: Any = None, *, raw: str | None = None,
          delay: float = 0) -> dict[str, Any]:
    result: dict[str, Any] = {"status": status, "delay": delay}
    if raw is not None:
        result["raw"] = raw
    else:
        result["body"] = {} if body is None else body
    return result


def routes_for(claude: dict[str, Any] | None = None,
               codex: dict[str, Any] | None = None,
               copilot: dict[str, Any] | None = None,
               hyper: dict[str, Any] | None = None,
               exchange: dict[str, Any] | None = None) -> dict[str, dict[str, Any]]:
    return {
        "GET /api/oauth/usage": claude or route(body=claude_body()),
        "GET /backend-api/wham/usage": codex or route(body=codex_body()),
        "GET /copilot_internal/user": copilot or route(body=copilot_body()),
        "GET /v1/credits": hyper or route(body={"balance": 228}),
        "POST /token/exchange": exchange or route(body={"access_token": "FAKE_HYPER_EXCHANGED"}),
    }


def put_bytes(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def put_json(path: Path, value: Any) -> bytes:
    data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
    put_bytes(path, data)
    return data


def snapshots(paths: list[Path]) -> dict[Path, bytes | None]:
    return {path: path.read_bytes() if path.is_file() else None for path in paths}


def assert_unchanged(before: dict[Path, bytes | None]) -> None:
    for path, expected in before.items():
        actual = path.read_bytes() if path.is_file() else None
        require(actual == expected, f"login file changed: {path}")


def assert_no_secrets(output: str, secrets: set[str]) -> None:
    for secret in secrets:
        if secret and secret in output:
            fail(f"output exposed credential material ({secret[:24]})")


def lower_headers(record: dict[str, Any]) -> dict[str, str]:
    return {str(key).lower(): str(value) for key, value in record.get("headers", {}).items()}


def assert_local_requests(server: VendorServer) -> None:
    allowed = {
        ("GET", "/api/oauth/usage"),
        ("GET", "/backend-api/wham/usage"),
        ("GET", "/copilot_internal/user"),
        ("GET", "/v1/credits"),
        ("POST", "/token/exchange"),
    }
    for request in server.requests:
        headers = lower_headers(request)
        require(headers.get("host") == server.expected_host,
                f"quota request went to an unexpected host: {headers.get('host')!r}")
        require((request["method"], request["path"]) in allowed,
                f"unexpected quota request: {request['method']} {request['path']}")


def assert_auth(server: VendorServer, path: str, token: str, method: str = "GET") -> None:
    requests = [r for r in server.requests if r["method"] == method and r["path"] == path]
    require(any(lower_headers(r).get("authorization") == f"Bearer {token}" for r in requests),
            f"{method} {path} did not carry the login's expected bearer token")


def assert_header(server: VendorServer, path: str, name: str, value: str,
                  method: str = "GET") -> None:
    requests = [r for r in server.requests if r["method"] == method and r["path"] == path]
    require(any(lower_headers(r).get(name.lower()) == value for r in requests),
            f"{method} {path} did not carry expected {name} header")


def find_json_object(output: str) -> dict[str, Any] | None:
    decoder = json.JSONDecoder()
    for index, char in enumerate(output):
        if char != "{":
            continue
        try:
            value, _end = decoder.raw_decode(output[index:])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "schemaVersion" in value and "accounts" in value:
            return value
    return None


def json_percent(value: Any) -> float:
    number = float(value)
    return number * 100 if 0 <= number <= 1 else number


def accounts_by_name(record: dict[str, Any]) -> dict[str, dict[str, Any]]:
    accounts = record.get("accounts")
    require(isinstance(accounts, list), "quota JSON has no accounts array")
    return {str(account.get("account")): account for account in accounts if isinstance(account, dict)}


def account_line(output: str, account: str) -> str:
    matches = [line.strip() for line in output.splitlines()
               if line.strip().lower().startswith(account.lower())]
    require(len(matches) == 1, f"expected one {account} quota line, got {matches!r}")
    return matches[0]


def make_claude_login(token: str, *, expires: float = 3600) -> Path:
    path = HOME / ".claude" / ".credentials.json"
    put_json(path, {"claudeAiOauth": {
        "accessToken": token,
        "expiresAt": int((time.time() + expires) * 1000),
    }})
    return path


def make_pi_login(*, codex_token: str = "FAKE_PI_CODEX_ACCESS",
                  codex_refresh: str = "FAKE_PI_CODEX_REFRESH",
                  copilot_token: str = "ghu_FAKE_COPILOT_REFRESH",
                  hyper_access: str = "FAKE_HYPER_ACCESS",
                  hyper_refresh: str = "FAKE_HYPER_REFRESH",
                  hyper_expires: float = 3600,
                  include_codex: bool = True, include_copilot: bool = True,
                  include_hyper: bool = True) -> Path:
    entries: dict[str, Any] = {}
    if include_codex:
        entries["openai-codex"] = {
            "type": "oauth", "access": codex_token, "refresh": codex_refresh,
            "expires": int((time.time() + 3600) * 1000), "accountId": "fake-account",
        }
    if include_copilot:
        entries["github-copilot"] = {
            "type": "oauth", "access": "FAKE_COPILOT_ACCESS", "refresh": copilot_token,
        }
    if include_hyper:
        entries["hyper"] = {
            "type": "oauth", "access": hyper_access,
            "expires": int((time.time() + hyper_expires) * 1000),
            "refresh": hyper_refresh,
        }
    path = HOME / ".pi" / "agent" / "auth.json"
    put_json(path, entries)
    return path


def make_codex_login(token: str = "FAKE_CODEX_ACCESS",
                     refresh: str = "FAKE_CODEX_REFRESH") -> Path:
    path = HOME / ".codex" / "auth.json"
    put_json(path, {"auth_mode": "chatgpt", "tokens": {
        "access_token": token, "refresh_token": refresh,
        "id_token": "FAKE_CODEX_ID", "account_id": "fake-account",
    }})
    return path


def worker_calls() -> list[tuple[str, str]]:
    calls_dir = HOME / ".cache" / "stub" / "calls"
    result = []
    if not calls_dir.is_dir():
        return result
    for path in calls_dir.iterdir():
        if not path.is_dir():
            continue
        model_file = path / "model"
        phase_file = path / "phase"
        if model_file.is_file() and phase_file.is_file():
            result.append((model_file.read_text().strip(), phase_file.read_text().strip()))
    return result


def write_task(task_id: str = "t001") -> Path:
    try:
        spec(task_id, ("true",), title="Quota-aware worker selection")
    except Exception as exc:  # Fixture creation errors are check errors, not product failures.
        raise CheckError(f"could not create {task_id}: {exc}") from exc
    path = Path(dhome()) / task_id / "SPEC.md"
    setup_require(path.is_file(), f"delegate new did not create {path}")
    return path


def assert_command_succeeded(rc: int, output: str, label: str) -> None:
    if rc != 0:
        fail(f"{label} failed (exit {rc}): {output[-500:]}")


def check_s071() -> None:
    tokens = {
        "claude": "FAKE_S071_CLAUDE_ACCESS",
        "codex": "FAKE_S071_PI_CODEX_ACCESS",
        "codex_refresh": "FAKE_S071_PI_CODEX_REFRESH",
        "copilot": "ghu_FAKE_S071_COPILOT_REFRESH",
        "hyper": "FAKE_S071_HYPER_ACCESS",
        "hyper_refresh": "FAKE_S071_HYPER_REFRESH",
    }
    paths = [
        make_claude_login(tokens["claude"]),
        make_pi_login(codex_token=tokens["codex"], codex_refresh=tokens["codex_refresh"],
                      copilot_token=tokens["copilot"], hyper_access=tokens["hyper"],
                      hyper_refresh=tokens["hyper_refresh"]),
    ]
    before = snapshots(paths)
    endpoints = {
        "claude": "/api/oauth/usage",
        "codex": "/backend-api/wham/usage",
        "copilot": "/copilot_internal/user",
        "hyper": "/v1/credits",
    }
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        server.set_routes(routes_for(
            claude=route(body=claude_body(58, 87, 4 * 3600, 3600), delay=0.5),
            codex=route(body=codex_body(25, 4 * 3600, 18_000,
                                        secondary=(36, 3600, 604_800)), delay=0.5),
            copilot=route(body=copilot_body(76, 3600), delay=0.5),
            hyper=route(body={"balance": 228}, delay=0.5),
        ))
        start = time.monotonic()
        rc, plain = run("quota")
        elapsed = time.monotonic() - start
        assert_command_succeeded(rc, plain, "delegate quota")
        require(elapsed < 1.5, f"quota reads were not returned in parallel within a few seconds ({elapsed:.2f}s)")
        for account, fragments in {
            "claude": ("13%", "42%", "weekly", "5-hour", "runway", ".credentials.json"),
            "codex": ("75%", "64%", "runway", "auth.json"),
            "copilot": ("24%", "runway", "auth.json"),
            "hyper": ("228", "250", "runway", "auth.json"),
        }.items():
            line = account_line(plain, account)
            require("resets" in line.lower(), f"{account} line did not show its reset: {line}")
            for fragment in fragments:
                require(fragment.lower() in line.lower(),
                        f"{account} line did not show {fragment!r}: {line}")
        rc, encoded = run("quota", "--json")
        assert_command_succeeded(rc, encoded, "delegate quota --json")
        record = find_json_object(encoded)
        require(record is not None, f"quota --json did not emit the versioned account record: {encoded[-400:]}")
        assert record is not None
        require(record.get("schemaVersion") == 1, "quota JSON schemaVersion was not 1")
        accounts = accounts_by_name(record)
        expected = {"claude", "codex", "copilot", "hyper"}
        require(set(accounts) == expected, f"quota JSON accounts were {sorted(accounts)}, expected {sorted(expected)}")
        expected_left = {"claude": (13, 42), "codex": (75, 64), "copilot": (24,)}
        for name, percentages in expected_left.items():
            account = accounts[name]
            require(account.get("status") == "ok", f"{name} status was not ok: {account}")
            windows = account.get("windows")
            require(isinstance(windows, list) and len(windows) >= len(percentages),
                    f"{name} JSON omitted quota windows: {account}")
            actual = [json_percent(window.get("percentLeft")) for window in windows]
            for percentage in percentages:
                require(any(abs(value - percentage) < 0.6 for value in actual),
                        f"{name} JSON did not report {percentage}% left: {actual}")
            require(all(window.get("resetsAt") for window in windows),
                    f"{name} JSON omitted a window reset time: {windows}")
            require(account.get("runway", {}).get("status") in {
                "exhausted_now", "projected_exhaustion", "through_reset", "unknown",
            }, f"{name} JSON omitted runway status: {account}")
        require(accounts["claude"].get("runway", {}).get("status") == "projected_exhaustion",
                f"Claude did not use its shorter 5-hour window for runway: {accounts['claude']}")
        require(accounts["codex"].get("runway", {}).get("status") == "projected_exhaustion",
                f"Codex did not use its shorter primary window for runway: {accounts['codex']}")
        hyper = accounts["hyper"]
        require(hyper.get("status") == "ok", f"hyper status was not ok: {hyper}")
        hyper_windows = hyper.get("windows")
        require(isinstance(hyper_windows, list) and hyper_windows and
                all(window.get("resetsAt") for window in hyper_windows),
                f"hyper JSON omitted the daily reset: {hyper}")
        require(hyper.get("runway", {}).get("status") in {
            "exhausted_now", "projected_exhaustion", "through_reset", "unknown",
        }, f"hyper JSON omitted runway status: {hyper}")
        balance = hyper.get("balance")
        if isinstance(balance, dict):
            balance = balance.get("remaining", balance.get("balance"))
        require(float(balance) == 228, f"hyper JSON did not report the 228-credit balance: {hyper}")
        for name, suffix in {
            "claude": ".credentials.json", "codex": "auth.json",
            "copilot": "auth.json", "hyper": "auth.json",
        }.items():
            require(suffix in str(accounts[name].get("source", "")),
                    f"{name} JSON did not name the login source: {accounts[name]}")
        server.set_routes(routes_for(
            claude=route(status=503, body={"error": "scripted unexpected response"}),
            codex=route(body=codex_body(25, 4 * 3600, 18_000,
                                        secondary=(36, 3600, 604_800))),
            copilot=route(body=copilot_body(76, 3600)),
            hyper=route(body={"balance": 228}),
        ))
        rc, unknown = run("quota")
        assert_command_succeeded(rc, unknown, "quota with one vendor unavailable")
        require("unknown" in account_line(unknown, "claude").lower(),
                "a vendor error did not make only Claude quota unknown")
        for name in ("codex", "copilot", "hyper"):
            require("unknown" not in account_line(unknown, name).lower(),
                    f"a Claude vendor error hid the healthy {name} reading")
        assert_no_secrets(plain + encoded + unknown, set(tokens.values()))
        assert_unchanged(before)
        for account, path in endpoints.items():
            token = tokens[account]
            assert_auth(server, path, token)
        assert_local_requests(server)
        for path in endpoints.values():
            count = sum(r["path"] == path for r in server.requests)
            require(count >= 3, f"quota reads did not query {path} for all commands")
    print("S071 quota text and JSON reported all four accounts, windows, resets, runway and sources without changing or exposing credentials")


def check_s072() -> None:
    codex_token = "FAKE_S072_CODEX_ACCESS"
    copilot_token = "ghu_FAKE_S072_COPILOT_REFRESH"
    paths = [make_codex_login(codex_token),
             make_pi_login(copilot_token=copilot_token, include_codex=False, include_hyper=False)]
    before = snapshots(paths)
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        server.set_routes(routes_for(
            codex=route(body=codex_body(50, 4 * 3600 + 50 * 60, 18_000)),
            copilot=route(body=copilot_body(10, 86_400)),
        ))
        write_task()
        rc, dry = run("run", "t001", "--dry-run")
        assert_command_succeeded(rc, dry, "S072 dry run")
        low = dry.lower()
        require("skipping" in low and "codex" in low and "account" in low,
                f"dry run did not skip the nearly exhausted Codex account: {dry[-500:]}")
        require("runs out" in low or "runway" in low or "exhaust" in low,
                f"dry run did not say what is left or when it runs out: {dry[-500:]}")
        require("copilot" in low and "candidate:" in low,
                f"dry run did not offer the healthy second candidate: {dry[-500:]}")
        rc, actual = run("run", "t001")
        assert_command_succeeded(rc, actual, "S072 run")
        low = actual.lower()
        require("skipping" in low and "codex" in low,
                f"run did not skip the nearly exhausted first account: {actual[-500:]}")
        calls = worker_calls()
        require(any(model == "copilot-worker" and phase == "work" for model, phase in calls),
                f"the task did not run on the healthy second candidate: {calls}")
        require(not any(model == "codex-worker" for model, _phase in calls),
                f"the nearly exhausted first worker was started: {calls}")
        old_calls = len(calls)
        request_count_before_disable = len(server.requests)
        config = HOME / ".config" / "delegate" / "workers.toml"
        text = config.read_text(encoding="utf-8")
        require("check = true" in text, "quota check fixture did not enable checks")
        config.write_text(text.replace("check = true", "check = false", 1), encoding="utf-8")
        write_task("t002")
        rc, disabled = run("run", "t002")
        assert_command_succeeded(rc, disabled, "run with quota checks disabled")
        calls = worker_calls()
        require(len(server.requests) == request_count_before_disable,
                "quota.check=false still made vendor requests")
        require(any(model == "codex-worker" and phase == "work" for model, phase in calls[old_calls:]),
                f"with quota checks disabled, the first candidate did not run: {calls[old_calls:]}")
        require("skipping codex" not in disabled.lower(),
                f"quota-disabled run still skipped the first candidate: {disabled[-400:]}")
        assert_no_secrets(dry + actual + disabled, {codex_token, "FAKE_CODEX_REFRESH", copilot_token})
        assert_unchanged(before)
        assert_auth(server, "/backend-api/wham/usage", codex_token)
        assert_auth(server, "/copilot_internal/user", copilot_token)
        assert_local_requests(server)
    print("S072 skipped the 10-minute Codex runway for dry and live runs, used Copilot, and honored quota.check=false")


def check_s073() -> None:
    codex_token = "FAKE_S073_CODEX_ACCESS"
    copilot_token = "ghu_FAKE_S073_COPILOT_REFRESH"
    paths = [make_codex_login(codex_token),
             make_pi_login(copilot_token=copilot_token, include_codex=False, include_hyper=False)]
    before = snapshots(paths)
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        server.set_routes(routes_for(
            codex=route(body=codex_body(50, 3 * 3600, 18_000)),
            copilot=route(body=copilot_body(10, 86_400)),
        ))
        spec_path = write_task()
        spec_path.write_text(spec_path.read_text(encoding="utf-8") + "\nsize: long\n", encoding="utf-8")
        rc, long_out = run("run", "t001", "--dry-run")
        assert_command_succeeded(rc, long_out, "long-task dry run")
        low = long_out.lower()
        require("skipping" in low and "codex" in low and "long" in low and "3h" in low,
                f"long task was not kept off the 2-hour account: {long_out[-500:]}")
        for size in ("small", None):
            content = spec_path.read_text(encoding="utf-8")
            content = re.sub(r"(?m)^size:.*\n?", "", content).rstrip() + "\n"
            if size:
                content += f"size: {size}\n"
            spec_path.write_text(content, encoding="utf-8")
            rc, output = run("run", "t001", "--dry-run")
            assert_command_succeeded(rc, output, f"{size or 'default-medium'} dry run")
            low = output.lower()
            require("candidate:" in low and "codex" in low,
                    f"{size or 'default-medium'} task did not use the first Codex candidate: {output[-500:]}")
            require("skipping codex" not in low,
                    f"{size or 'default-medium'} task was incorrectly kept off the account: {output[-500:]}")
        content = re.sub(r"(?m)^size:.*\n?", "", spec_path.read_text(encoding="utf-8")).rstrip()
        spec_path.write_text(content + "\nsize: huge\n", encoding="utf-8")
        rc, bad_size = run("run", "t001", "--dry-run")
        low = bad_size.lower()
        require(rc != 0 and "size" in low and all(size in low for size in ("small", "medium", "long")),
                f"invalid size was not refused with the accepted values: {bad_size[-500:]}")
        assert_no_secrets(long_out + bad_size, {codex_token, copilot_token})
        assert_unchanged(before)
        assert_auth(server, "/backend-api/wham/usage", codex_token)
        assert_local_requests(server)
    print("S073 used runway rather than quota percentage: long skipped the 2-hour account; small and default-medium used it; invalid size was refused")


def check_s074() -> None:
    codex_token = "FAKE_S074_EXPIRED_CODEX_ACCESS"
    refresh = "FAKE_S074_CODEX_REFRESH"
    path = make_codex_login(codex_token, refresh)
    before = snapshots([path])
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        server.set_routes(routes_for(codex=route(status=401, body={"error": "login expired"}, delay=0.15)))
        write_task()
        start = time.monotonic()
        rc, output = run("run", "t001")
        elapsed = time.monotonic() - start
        assert_command_succeeded(rc, output, "run with unknown quota")
        low = output.lower()
        require("quota for codex unknown" in low and "using" in low and "codex-worker" in low,
                "run did not warn that quota is unknown and use the first worker")
        require(elapsed < 3.0, f"unknown quota held the run too long ({elapsed:.2f}s)")
        require(any(model == "codex-worker" and phase == "work" for model, phase in worker_calls()),
                "the worker did not start when quota was unknown")
        assert_unchanged(before)
        assert_no_secrets(output, {codex_token, refresh})
        assert_auth(server, "/backend-api/wham/usage", codex_token)
        assert_local_requests(server)
    print("S074 an expired Codex login made quota unknown with a warning, did not block the run, and remained unchanged")


def check_s076() -> None:
    codex_token = "FAKE_S076_CODEX_ACCESS"
    copilot_token = "ghu_FAKE_S076_COPILOT_REFRESH"
    paths = [make_codex_login(codex_token),
             make_pi_login(copilot_token=copilot_token, include_codex=False, include_hyper=False)]
    before = snapshots(paths)
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        server.set_routes(routes_for(
            codex=route(body=codex_body(100, 3 * 3600, 18_000)),
            copilot=route(body=copilot_body(100, 3600)),
        ))
        write_task()
        rc, output = run("run", "t001")
        require(rc != 0, "run started although every candidate account was exhausted")
        require("no [implement].fast candidate has quota left:" in output.lower(),
                f"run did not report that the fast tier has no quota: {output[-500:]}")
        account_lines = [line for line in output.splitlines()
                         if "copilot" in line.lower() or "codex" in line.lower()]
        copilot_index = next((i for i, line in enumerate(account_lines) if "copilot" in line.lower()), -1)
        codex_index = next((i for i, line in enumerate(account_lines) if "codex" in line.lower()), -1)
        require(copilot_index >= 0 and codex_index >= 0 and copilot_index < codex_index,
                f"exhausted accounts were not listed earliest reset first: {account_lines}")
        require(all("reset" in line.lower() or "until" in line.lower() for line in account_lines),
                f"account failure lines did not name reset times: {account_lines}")
        require(not any(model in ("codex-worker", "copilot-worker") for model, _ in worker_calls()),
                f"a worker started despite every account being exhausted: {worker_calls()}")
        rc_list, listed = run("list")
        assert_command_succeeded(rc_list, listed, "delegate list")
        require("t001" in listed and "not run" in listed.lower(),
                f"an unsuccessful round was recorded: {listed[-400:]}")
        require("[implement].standard" not in output.lower() and "@ standard" not in output.lower(),
                f"runner moved to another tier instead of stopping: {output[-500:]}")
        assert_unchanged(before)
        assert_no_secrets(output, {codex_token, copilot_token})
        assert_local_requests(server)
    print("S076 stopped before starting or recording a round, listed both exhausted accounts in reset order, and did not step up")


P020_STATES = ("valid", "expired", "malformed", "missing")
P020_OUTCOMES = ("normal", "exhausted", "error", "timeout", "garbage")


def state_login(path: Path, state: str, name: str, secrets: set[str], *, kind: str) -> None:
    if state == "missing":
        path.unlink(missing_ok=True)
        return
    token = f"FAKE_P020_{name}_{state.upper()}"
    secrets.add(token)
    if state == "malformed":
        put_bytes(path, f'{{"broken": "{token}"'.encode())
        return
    if kind == "claude":
        put_json(path, {"claudeAiOauth": {
            "accessToken": token,
            "expiresAt": int((time.time() + (3600 if state == "valid" else -1)) * 1000),
        }})
    elif kind == "codex":
        refresh = f"FAKE_P020_{name}_REFRESH"
        secrets.add(refresh)
        put_json(path, {"auth_mode": "chatgpt", "tokens": {
            "access_token": token, "refresh_token": refresh,
            "id_token": f"FAKE_P020_{name}_ID", "account_id": "fake-account",
        }})
        secrets.add(f"FAKE_P020_{name}_ID")
    else:
        raise CheckError(f"unknown login fixture type {kind}")


def p020_routes(outcomes: dict[str, str], tag: str) -> dict[str, dict[str, Any]]:
    def response(outcome: str, body: dict[str, Any]) -> dict[str, Any]:
        if outcome == "normal":
            return route(body=body)
        if outcome == "exhausted":
            exhausted = json.loads(json.dumps(body))
            if "five_hour" in exhausted:
                exhausted["five_hour"]["utilization"] = 100
                exhausted["seven_day"]["utilization"] = 100
            elif "rate_limit" in exhausted:
                exhausted["rate_limit"]["primary_window"]["used_percent"] = 100
            elif "quota_snapshots" in exhausted:
                exhausted["quota_snapshots"]["premium_interactions"].update(
                    remaining=0, percent_remaining=0)
            else:
                exhausted["balance"] = 0
            return route(body=exhausted)
        if outcome == "error":
            return route(status=503, body={"error": "scripted vendor unavailable"})
        if outcome == "timeout":
            return route(body=body, delay=5.3)
        return route(raw="not-json")

    hyper_body = {"balance": 228}
    return {
        "GET /api/oauth/usage": response(outcomes["claude"], claude_body(20, 25)),
        "GET /backend-api/wham/usage": response(outcomes["codex"], codex_body(20)),
        "GET /copilot_internal/user": response(outcomes["copilot"], copilot_body(20)),
        "GET /v1/credits": response(outcomes["hyper"], hyper_body),
        "POST /token/exchange": route(body={"access_token": f"FAKE_P020_EXCHANGED_{tag}"}),
    }


def check_p020() -> None:
    global CURRENT_CASE
    rng, actual_seed = seed()
    write_task()
    claude_path = HOME / ".claude" / ".credentials.json"
    codex_path = HOME / ".codex" / "auth.json"
    pi_path = HOME / ".pi" / "agent" / "auth.json"
    paths = [claude_path, codex_path, pi_path]
    state_order = list(P020_STATES)
    outcome_order = list(P020_OUTCOMES)
    pi_login_cases = ["all-valid", "missing-entry", "malformed", "missing-file", "refresh-near-expiry"]
    rng.shuffle(state_order)
    rng.shuffle(outcome_order)
    rng.shuffle(pi_login_cases)
    all_secrets: set[str] = set()
    with VendorServer(WORK / "vendors.jsonl") as server:
        os.environ.update(server.env())
        # First exercise every vendor response class with a readable login for
        # each account. Rotating the five answers makes each service see normal,
        # exhausted, error, timeout, and malformed responses exactly once.
        for index in range(5):
            tag = f"{actual_seed}_{index}"
            case_secrets: set[str] = set()
            state_login(claude_path, "valid", f"CLAUDE_{tag}", case_secrets, kind="claude")
            state_login(codex_path, "valid", f"CODEX_{tag}", case_secrets, kind="codex")
            pi_tokens = {
                "codex_token": f"FAKE_P020_PI_CODEX_{tag}",
                "codex_refresh": f"FAKE_P020_PI_CODEX_REFRESH_{tag}",
                "copilot_token": f"ghu_FAKE_P020_COPILOT_{tag}",
                "hyper_access": f"FAKE_P020_HYPER_ACCESS_{tag}",
                "hyper_refresh": f"FAKE_P020_HYPER_REFRESH_{tag}",
            }
            make_pi_login(**pi_tokens)
            case_secrets.update(pi_tokens.values())
            case_secrets.add("FAKE_COPILOT_ACCESS")
            outcomes = {
                name: outcome_order[(index + offset) % len(outcome_order)]
                for offset, name in enumerate(("claude", "codex", "copilot", "hyper"))
            }
            exchanged = f"FAKE_P020_EXCHANGED_{tag}"
            case_secrets.add(exchanged)
            CURRENT_CASE = f"response trial={index}; login=valid; responses={outcomes}"
            server.set_routes(p020_routes(outcomes, tag))
            before = snapshots(paths)
            all_secrets.update(case_secrets)
            rc, quota_output = run("quota")
            if rc != 0:
                fail(f"P020 response trial {index}: delegate quota failed before privacy assertions: {quota_output[-400:]}")
            assert_no_secrets(quota_output, all_secrets)
            rc, dry_output = run("run", "t001", "--dry-run")
            lower = dry_output.lower()
            require("unknown subcommand" not in lower and
                    ("candidate:" in lower or "quota left" in lower or "quota for" in lower),
                    f"P020 response trial {index}: dry-run did not reach quota-aware candidate selection: {dry_output[-400:]}")
            assert_no_secrets(dry_output, all_secrets)
            assert_unchanged(before)
            assert_local_requests(server)

        # Then vary valid, expired, malformed, and missing local auth stores
        # against normal replies, including an in-memory Hyper token exchange.
        for index, pi_case in enumerate(pi_login_cases):
            tag = f"{actual_seed}_login_{index}"
            case_secrets: set[str] = set()
            claude_state = state_order[index % len(state_order)]
            codex_state = state_order[(index + 1) % len(state_order)]
            state_login(claude_path, claude_state, f"CLAUDE_{tag}", case_secrets, kind="claude")
            state_login(codex_path, codex_state, f"CODEX_{tag}", case_secrets, kind="codex")
            if pi_case == "malformed":
                marker = f"FAKE_P020_PI_MALFORMED_{tag}"
                case_secrets.add(marker)
                put_bytes(pi_path, f'{{"invalid": {marker}'.encode())
            elif pi_case == "missing-file":
                pi_path.unlink(missing_ok=True)
            else:
                copilot = f"ghu_FAKE_P020_COPILOT_{tag}"
                hyper_access = f"FAKE_P020_HYPER_ACCESS_{tag}"
                hyper_refresh = f"FAKE_P020_HYPER_REFRESH_{tag}"
                entries: dict[str, Any] = {
                    "hyper": {
                        "type": "oauth", "access": hyper_access,
                        "expires": int((time.time() + (20 if pi_case == "refresh-near-expiry" else 3600)) * 1000),
                        "refresh": hyper_refresh,
                    },
                }
                case_secrets.update({copilot, hyper_access, hyper_refresh})
                if pi_case != "missing-entry":
                    entries["github-copilot"] = {
                        "type": "oauth", "access": f"FAKE_P020_COPILOT_ACCESS_{tag}",
                        "refresh": copilot,
                    }
                    case_secrets.add(f"FAKE_P020_COPILOT_ACCESS_{tag}")
                put_json(pi_path, entries)
                if pi_case == "refresh-near-expiry":
                    case_secrets.add(f"FAKE_P020_EXCHANGED_{tag}")
            CURRENT_CASE = (f"login trial={index}; Claude={claude_state}; "
                            f"Codex={codex_state}; Pi={pi_case}")
            login_routes = p020_routes({name: "normal" for name in
                                         ("claude", "codex", "copilot", "hyper")}, tag)
            if codex_state == "expired":
                login_routes["GET /backend-api/wham/usage"] = route(401, {"error": "expired login"})
            server.set_routes(login_routes)
            before = snapshots(paths)
            all_secrets.update(case_secrets)
            rc, quota_output = run("quota")
            if rc != 0:
                fail(f"P020 login trial {index}: delegate quota failed before privacy assertions: {quota_output[-400:]}")
            assert_no_secrets(quota_output, all_secrets)
            rc, dry_output = run("run", "t001", "--dry-run")
            lower = dry_output.lower()
            require("unknown subcommand" not in lower and
                    ("candidate:" in lower or "quota left" in lower or "quota for" in lower),
                    f"P020 login trial {index}: dry-run did not reach quota-aware candidate selection: {dry_output[-400:]}")
            assert_no_secrets(dry_output, all_secrets)
            assert_unchanged(before)
            assert_local_requests(server)

        # Check the persisted stand-in log too, not only the server's in-memory view.
        log_path = WORK / "vendors.jsonl"
        for line in log_path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            require(lower_headers(record).get("host") == server.expected_host,
                    f"P020 request log contains a non-local destination: {record}")
            require((record["method"], record["path"]) in {
                ("GET", "/api/oauth/usage"), ("GET", "/backend-api/wham/usage"),
                ("GET", "/copilot_internal/user"), ("GET", "/v1/credits"),
                ("POST", "/token/exchange"),
            }, f"P020 request log contains an unexpected request: {record}")
    CURRENT_CASE = None
    print(f"P020 checked five seeded vendor response mixes and five login-file mixes without leaks or mutations (seed {actual_seed})")


def main() -> None:
    scenario = sys.argv[1] if len(sys.argv) > 1 else ""
    checks = {
        "S071": check_s071,
        "S072": check_s072,
        "S073": check_s073,
        "S074": check_s074,
        "S076": check_s076,
        "P020": check_p020,
    }
    if scenario not in checks:
        check_error(f"unknown quota scenario {scenario!r}")
    try:
        checks[scenario]()
    except CheckError as exc:
        check_error(str(exc))
    except SystemExit:
        raise
    except Exception as exc:  # A broken fixture/assertion is exit 3, not a product failure.
        check_error(f"{type(exc).__name__}: {exc}")


if __name__ == "__main__":
    main()
