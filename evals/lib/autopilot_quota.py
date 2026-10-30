#!/usr/bin/env python3
"""A local stand-in for the Claude quota endpoint, for the autopilot quota-floor check.

    autopilot_quota.py <workdir> <five_hour_used> <reset_seconds> <ready_file>

Writes a Claude login for this HOME, starts the vendor server, exports its base URL into
<workdir>/quota.env, touches <ready_file>, and serves until SIGTERM. The reset used for both
Claude windows is <reset_seconds> from now, so `delegate quota` and the run report both name
the same clock. S072's quota_checks.py supplies the same stand-in pieces.
"""

from __future__ import annotations

import os
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0, os.environ["EVALS_LIB"])
from quota_checks import claude_body, make_claude_login, route  # noqa: E402
from vendors import VendorServer  # noqa: E402


def main() -> None:
    work = Path(sys.argv[1])
    used = float(sys.argv[2])
    reset = float(sys.argv[3])
    ready = Path(sys.argv[4])
    make_claude_login("FAKE_T064_CLAUDE_ACCESS")
    server = VendorServer()
    server.set_routes({"GET /api/oauth/usage": route(body=claude_body(used, used, reset, reset))})
    with (work / "quota.env").open("w", encoding="utf-8") as env:
        for name, value in server.env().items():
            env.write(f"export {name}={value}\n")
    ready.write_text(f"{server.base_url}\n", encoding="utf-8")
    stop = {"now": False}

    def _stop(*_args: object) -> None:
        stop["now"] = True

    signal.signal(signal.SIGTERM, _stop)
    print(server.base_url, flush=True)
    while not stop["now"]:
        time.sleep(0.1)
    server.close()


if __name__ == "__main__":
    main()
