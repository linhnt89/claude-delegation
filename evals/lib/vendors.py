"""Local scripted vendor endpoints for quota scenario checks."""

from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
import time
from typing import Any


class VendorServer:
    """One local server for the four quota vendors and Hyper token exchange."""

    def __init__(self, log_path: str | Path | None = None) -> None:
        self._lock = threading.Lock()
        self._routes: dict[str, dict[str, Any]] = {}
        self._requests: list[dict[str, Any]] = []
        self._log_path = Path(log_path) if log_path else None
        if self._log_path:
            self._log_path.parent.mkdir(parents=True, exist_ok=True)
            self._log_path.write_text("", encoding="utf-8")
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802 - stdlib handler protocol
                self._serve()

            def do_POST(self) -> None:  # noqa: N802 - stdlib handler protocol
                self._serve()

            def log_message(self, _format: str, *_args: Any) -> None:
                return

            def _serve(self) -> None:
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                    body = self.rfile.read(length) if length else b""
                    record = {
                        "method": self.command,
                        "path": self.path,
                        "headers": dict(self.headers.items()),
                        "body": body.decode("utf-8", errors="replace"),
                    }
                    route_key = f"{self.command} {self.path}"
                    with owner._lock:
                        owner._requests.append(record)
                        if owner._log_path:
                            with owner._log_path.open("a", encoding="utf-8") as log:
                                log.write(json.dumps(record, sort_keys=True) + "\n")
                        route = json.loads(json.dumps(owner._routes.get(route_key, {
                            "status": 404,
                            "body": {"error": "route not scripted"},
                        })))
                    delay = float(route.get("delay", 0))
                    if delay > 0:
                        time.sleep(delay)
                    if "raw" in route:
                        response = str(route["raw"]).encode("utf-8")
                    else:
                        response = json.dumps(route.get("body", {})).encode("utf-8")
                    self.send_response(int(route.get("status", 200)))
                    self.send_header("Content-Type", route.get("content_type", "application/json"))
                    self.send_header("Content-Length", str(len(response)))
                    self.send_connection_close()
                    self.end_headers()
                    self.wfile.write(response)
                except (BrokenPipeError, ConnectionResetError):
                    # A scripted timeout may outlive the client's 5-second deadline.
                    return

            def send_connection_close(self) -> None:
                self.send_header("Connection", "close")

        self._httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._httpd.daemon_threads = True
        self._thread = threading.Thread(target=self._httpd.serve_forever, daemon=True)
        self._thread.start()
        self._closed = False

    @property
    def base_url(self) -> str:
        host, port = self._httpd.server_address
        return f"http://{host}:{port}"

    @property
    def expected_host(self) -> str:
        return self.base_url.removeprefix("http://")

    def env(self) -> dict[str, str]:
        return {
            "DELEGATE_QUOTA_BASE_CLAUDE": self.base_url,
            "DELEGATE_QUOTA_BASE_CODEX": self.base_url,
            "DELEGATE_QUOTA_BASE_COPILOT": self.base_url,
            "DELEGATE_QUOTA_BASE_HYPER": self.base_url,
        }

    def set_routes(self, routes: dict[str, dict[str, Any]]) -> None:
        with self._lock:
            self._routes = json.loads(json.dumps(routes))

    @property
    def requests(self) -> list[dict[str, Any]]:
        with self._lock:
            return json.loads(json.dumps(self._requests))

    def clear_requests(self) -> None:
        with self._lock:
            self._requests.clear()
            if self._log_path:
                self._log_path.write_text("", encoding="utf-8")

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._httpd.shutdown()
        self._httpd.server_close()
        self._thread.join(timeout=2)

    def __enter__(self) -> VendorServer:
        return self

    def __exit__(self, _exc_type: Any, _exc: Any, _tb: Any) -> None:
        self.close()
