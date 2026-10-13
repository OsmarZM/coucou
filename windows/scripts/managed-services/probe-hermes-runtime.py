"""Hermes/ACP + real Coucou MCP transport against strictly LOCAL fixtures.

This is NOT a real provider, quota, CLI delegation or UI qualification. The
caller must pass a freshly built Coucou executable with --hermes-tool-bridge.
No user profile, provider key, global login or existing chat is consulted.
"""
import argparse
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import hashlib
import json
import os
from pathlib import Path
from queue import Empty, Queue
import secrets
import socketserver
import shutil
import subprocess
import sys
import threading
import time

REVISION = "781334eea4b9225a3e194faf0c241d9afe218634"
EXPECTED = sorted("mcp__coucou__" + name for name in (
    "context_read", "files_request", "files_count", "files_list",
    "document_read", "memory_propose", "skill_propose", "delegate_cli",
    "open_device_manager", "open_windows_settings", "open_application", "open_directory",
    "query_processes", "organize_files_dry_run", "query_recycle_bin", "cli_version"))


def load_bootstrap():
    path = Path(__file__).resolve().parents[2] / "src-tauri/src/hermes/bootstrap.py"
    spec = importlib.util.spec_from_file_location("coucou_hermes_bootstrap", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return path, module


class ModelFixture(BaseHTTPRequestHandler):
    calls = []
    native = False
    slow = False
    release_slow = threading.Event()
    failures = []

    def log_message(self, *args):
        pass

    def do_POST(self):
        try:
            if self.path != "/v1/chat/completions" or self.headers.get("Authorization") != "Bearer local-fixture-key":
                raise AssertionError("Unexpected model route/authentication")
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            names = sorted(item["function"]["name"] for item in request.get("tools", []))
            if names != EXPECTED:
                raise AssertionError("Actual model request changed tool names: " + json.dumps(names))
            self.calls.append({"model": request.get("model"), "toolNames": names})
            if self.slow:
                self.release_slow.wait(10)
            tool_seen = any(message.get("role") == "tool" for message in request["messages"])
            runtime_seen = any(message.get("role") == "tool" and (message.get("name") == "mcp__coucou__query_processes" or message.get("tool_call_id") == "owned-runtime") for message in request["messages"])
            if self.native:
                message = {"role": "assistant", "content": None, "tool_calls": [{"id": "attempt-native",
                    "type": "function", "function": {"name": "terminal", "arguments": '{"command":"must-not-execute"}'}}]}
                finish = "tool_calls"
            elif runtime_seen:
                message = {"role": "assistant", "content": "Contexto local recebido pela ponte Coucou."}
                finish = "stop"
            elif tool_seen:
                message = {"role": "assistant", "content": None, "tool_calls": [{"id": "owned-runtime",
                    "type": "function", "function": {"name": "mcp__coucou__query_processes", "arguments": "{}"}}]}
                finish = "tool_calls"
            else:
                message = {"role": "assistant", "content": None, "tool_calls": [{"id": "owned-context",
                    "type": "function", "function": {"name": "mcp__coucou__context_read", "arguments": "{}"}}]}
                finish = "tool_calls"
            self.send_response(200)
            if request.get("stream"):
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                delta = dict(message)
                if "tool_calls" in delta:
                    delta["tool_calls"][0]["index"] = 0
                chunks = [
                    {"id": "local-proof", "object": "chat.completion.chunk", "created": 1,
                     "model": "local-fixture", "choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                    {"id": "local-proof", "object": "chat.completion.chunk", "created": 1,
                     "model": "local-fixture", "choices": [{"index": 0, "delta": {}, "finish_reason": finish}]},
                    {"id": "local-proof", "object": "chat.completion.chunk", "created": 1,
                     "model": "local-fixture", "choices": [], "usage": {"prompt_tokens": 20, "completion_tokens": 5, "total_tokens": 25}},
                ]
                for chunk in chunks:
                    self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
            else:
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"id": "local-proof", "object": "chat.completion", "created": 1,
                    "model": "local-fixture", "choices": [{"index": 0, "message": message, "finish_reason": finish}],
                    "usage": {"prompt_tokens": 20, "completion_tokens": 5, "total_tokens": 25}}).encode())
        except (BrokenPipeError, ConnectionResetError):
            pass
        except BaseException as error:
            self.failures.append(type(error).__name__ + ": " + str(error))
            self.send_error(500)


class BrokerFixture(socketserver.StreamRequestHandler):
    token = ""
    calls = []

    def handle(self):
        request = json.loads(self.rfile.readline(262145))
        if request.get("token") != self.token or request.get("name") not in ("context_read", "query_processes") or request.get("arguments") != {}:
            response = {"ok": False, "error": "Fixture denied unexpected operation"}
        else:
            self.calls.append({"id": request["id"], "name": request["name"]})
            value = {"text": "Contexto fictício autorizado", "memories": [], "skills": []} if request["name"] == "context_read" else {"processes": [{"pid":123,"name":"fixture.exe"}], "fixtureOnly":True}
            response = {"ok": True, "value": value}
        self.wfile.write((json.dumps(response) + "\n").encode())


class ACPClient:
    def __init__(self, command, env, cwd):
        self.process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8",
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.frames = Queue()
        self.errors = deque(maxlen=30)
        self.events = []
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._errors, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            try:
                self.frames.put(json.loads(line))
            except ValueError:
                self.frames.put({"invalidFrame": True})
        self.frames.put({"closed": True})

    def _errors(self):
        for line in self.process.stderr:
            self.errors.append(line.replace("local-fixture-key", "[fixture-key]").rstrip())

    def send(self, frame):
        self.process.stdin.write(json.dumps(frame) + "\n")
        self.process.stdin.flush()

    def response(self, request_id, timeout=300):
        deadline = time.monotonic() + timeout
        while True:
            frame = self.frames.get(timeout=max(0.01, deadline - time.monotonic()))
            if frame.get("closed") or frame.get("invalidFrame"):
                raise RuntimeError("ACP closed/invalid; diagnostics: " + " | ".join(self.errors))
            if frame.get("id") == request_id and "method" not in frame:
                if "error" in frame:
                    self.errors.append("ACP error: " + json.dumps(frame["error"]).replace("local-fixture-key", "[fixture-key]"))
                return frame
            if frame.get("id") is not None and "method" in frame:
                raise AssertionError("Native ACP client resource requested")
            self.events.append(frame)

    def request(self, method, params, request_id):
        print("ACP probe stage: " + request_id, flush=True)
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        return self.response(request_id)

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--coucou", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--debug-tool-surface", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    # The parent may rebuild the original release while this probe runs. Run a
    # byte-identical, private fixture copy so the release file is never locked.
    qualified_executable = args.output.resolve() / "coucou.exe"
    shutil.copyfile(args.coucou, qualified_executable)
    config = json.loads((args.runtime / "coucou-runtime.json").read_text(encoding="utf-8-sig"))
    assert config.get("owner") == "coucou" and config.get("revision") == REVISION, "Runtime manifest mismatch"
    source, python = Path(config["sourceDirectory"]), Path(config["pythonExecutable"])
    bootstrap, module = load_bootstrap()
    if args.debug_tool_surface:
        # Fixture-only observation. Keep the exact allowlist/check unchanged;
        # record names, never definitions, arguments, credentials or prompts.
        original = bootstrap.read_text(encoding="utf-8")
        needle = '    tools = getattr(agent, "tools", None)\n'
        diagnostic = '''    from tools.registry import registry as diagnostic_registry
    from toolsets import get_toolset as diagnostic_toolset
    print(json.dumps({"fixtureToolSurface": {
        "names": [item.get("function", {}).get("name") for item in (tools or [])],
        "enabled": getattr(agent, "enabled_toolsets", None),
        "disabled": getattr(agent, "disabled_toolsets", None),
        "registered": diagnostic_registry.get_tool_names_for_toolset("mcp-coucou"),
        "toolset": diagnostic_toolset("mcp-coucou")}}), file=sys.stderr)
'''
        assert original.count(needle) == 1, "Diagnostic insertion changed"
        bootstrap = args.output.resolve() / "diagnostic-bootstrap.py"
        bootstrap.write_text(original.replace(needle, needle + diagnostic), encoding="utf-8")
    BrokerFixture.token = secrets.token_hex(32)
    model = ThreadingHTTPServer(("127.0.0.1", 0), ModelFixture)
    broker = socketserver.ThreadingTCPServer(("127.0.0.1", 0), BrokerFixture)
    for server in (model, broker):
        threading.Thread(target=server.serve_forever, daemon=True).start()
    home = args.output.resolve() / "profile"
    for relative in ("workspace", "home", "cache/scratch", "memories", "skills", "isolated-appdata", "isolated-localappdata"):
        (home / relative).mkdir(parents=True, exist_ok=True)
    (home / "coucou-managed.json").write_text(json.dumps({"owner": "coucou", "revision": REVISION}), encoding="utf-8")
    profile = {"model": {"provider": "custom", "default": "local-fixture", "base_url": f"http://127.0.0.1:{model.server_port}/v1",
        "api_key": "${COUCOU_HERMES_MODEL_KEY}", "api_mode": "chat_completions", "context_length": 64000},
        "platform_toolsets": {"acp": []}, "mcp_servers": {}, "plugins": {"enabled": [], "disabled": []},
        "agent": {"api_max_retries": 1, "auto_recovery_cycles": 0, "environment_probe": False},
        "memory": {"memory_enabled": False, "user_profile_enabled": False, "provider": ""},
        "skills": {"external_dirs": [], "auto_load": [], "project_dirs": [], "inline_shell": False},
        "auxiliary": {"background_review": {"enabled": False}, "title_generation": {"enabled": False}},
        "curator": {"enabled": False}, "tools": {"tool_search": {"enabled": "off"}}, "fallback_model": [],
        "security": {"allow_lazy_installs": False}, "hooks": {}}
    (home / "config.yaml").write_text(json.dumps(profile), encoding="utf-8")
    spec = {"command": str(qualified_executable), "args": ["--hermes-tool-bridge"],
        "environment": {"COUCOU_BRIDGE_ENDPOINT": f"127.0.0.1:{broker.server_address[1]}", "COUCOU_BRIDGE_TOKEN": BrokerFixture.token}}
    env = {name: os.environ[name] for name in ("SystemRoot", "WINDIR", "SYSTEMDRIVE", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE") if name in os.environ}
    env.update({"PATH": str(python.parent) + os.pathsep + str(Path(os.environ["SystemRoot"]) / "System32"),
        "HOME": str(home / "home"), "USERPROFILE": str(home / "home"), "APPDATA": str(home / "isolated-appdata"),
        "LOCALAPPDATA": str(home / "isolated-localappdata"), "HERMES_HOME": str(home),
        "HERMES_RUNTIME_DIR": str(source), "HERMES_DISABLE_LAZY_INSTALLS": "1", "HERMES_SUPERVISED_CHILD": "1",
        "HERMES_ACP_SKIP_CONFIGURED_MCP": "1", "HERMES_ENABLE_PROJECT_PLUGINS": "0",
        "PYTHONNOUSERSITE": "1", "PYTHONUTF8": "1", "COUCOU_HERMES_MODEL_KEY": "local-fixture-key",
        "COUCOU_HERMES_BRIDGE_SPEC": json.dumps(spec), "TEMP": str(home / "cache/scratch"), "TMP": str(home / "cache/scratch")})
    command = [str(python), "-I", "-B", "-u", str(bootstrap), "--source", str(source), "--profile", str(home)]
    evidence = {"qualification": "real Hermes runtime + real Rust MCP transport; local model and broker fixtures only",
        "revision": REVISION, "passed": False, "diagnosticAdapter": args.debug_tool_surface,
        "startedUtc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "bootstrapSha256": hashlib.sha256(bootstrap.read_bytes()).hexdigest(),
        "coucouExecutableSha256": hashlib.sha256(qualified_executable.read_bytes()).hexdigest()}
    client = ACPClient(command, env, home / "workspace")
    try:
        initialize_started = time.monotonic()
        initialized = client.request("initialize", {"protocolVersion": 1, "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False}}, "init")
        evidence["initializeSeconds"] = round(time.monotonic() - initialize_started, 3)
        assert initialized["result"]["_meta"]["coucouIsolation"]["phase"] == "policy", initialized
        new_started = time.monotonic()
        session = client.request("session/new", {"cwd": str(home / "workspace"), "mcpServers": []}, "new")
        evidence["sessionNewSeconds"] = round(time.monotonic() - new_started, 3)
        assert "result" in session, session
        assert session["result"]["_meta"]["coucouIsolation"]["toolNames"] == EXPECTED, session
        session_id = session["result"]["sessionId"]
        evidence["attestation"] = session["result"]["_meta"]["coucouIsolation"]
        prompt = client.request("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "Consulte o contexto local desta fixture."}]}, "prompt")
        assert prompt.get("result", {}).get("stopReason") == "end_turn", prompt
        assert [call["name"] for call in BrokerFixture.calls] == ["context_read", "query_processes"], BrokerFixture.calls
        assert not ModelFixture.failures, ModelFixture.failures
        assert len(ModelFixture.calls) == 3, f"Normal turn changed model request count: {len(ModelFixture.calls)}"
        evidence["normalTurn"] = {"stopReason": prompt["result"]["stopReason"], "modelRequests": len(ModelFixture.calls), "brokerRequests": len(BrokerFixture.calls)}
        assert any(event.get("params", {}).get("update", {}).get("sessionUpdate") == "agent_message_chunk"
                   for event in client.events), "No streamed assistant response"
        requests_before_controls = len(ModelFixture.calls)
        for method, params in (("session/set_model", {"sessionId": session_id, "modelId": "unexpected"}),
                               ("session/set_mode", {"sessionId": session_id, "modeId": "yolo"}),
                               ("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "/model unexpected"}]}),
                               ("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "__HERMES_MOA_TURN_V1__e30="}]})):
            frame = client.request(method, params, method)
            assert "error" in frame, frame
        assert len(ModelFixture.calls) == requests_before_controls, "Control request reached the model"
        evidence["modelModeSlashAndMoaDeniedBeforeModel"] = True
        ModelFixture.native = True
        requests_before_native = len(ModelFixture.calls)
        malicious = client.request("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "Recuse o comando nativo da fixture."}]}, "native")
        assert "error" in malicious, malicious
        assert malicious["error"].get("message") == "Managed Hermes turn failed", malicious
        assert len(BrokerFixture.calls) == 2
        native_requests = len(ModelFixture.calls) - requests_before_native
        assert native_requests == 1, f"Policy failure repeated model requests: {native_requests}"
        evidence["nativeTerminalDenied"] = True
        evidence["nativeModelRequests"] = native_requests
        repeated = client.request("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "Não retome a sessão que falhou."}]}, "failed-session")
        assert "error" in repeated, "Failed managed session accepted a new prompt"
        assert len(ModelFixture.calls) == requests_before_native + 1, "Failed session reached the model"
        assert len(ModelFixture.calls) == 4, f"Unexpected total model requests: {len(ModelFixture.calls)}"
        assert not ModelFixture.failures, ModelFixture.failures
        evidence["failedSessionDeniedBeforeModel"] = True
        evidence["passed"] = not args.debug_tool_surface
    except BaseException as error:
        evidence["failureType"] = type(error).__name__
        raise
    finally:
        client.close()
        ModelFixture.release_slow.set()
        for server in (model, broker):
            server.shutdown()
            server.server_close()
        evidence["runtimeExitCode"] = client.process.returncode
        evidence["modelRequests"] = ModelFixture.calls
        evidence["modelFailures"] = ModelFixture.failures
        evidence["brokerRequestCount"] = len(BrokerFixture.calls)
        evidence["finishedUtc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        (args.output / "diagnostics.txt").write_text("\n".join(client.errors), encoding="utf-8")
        (args.output / "evidence.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(evidence, ensure_ascii=False))


if __name__ == "__main__":
    main()
