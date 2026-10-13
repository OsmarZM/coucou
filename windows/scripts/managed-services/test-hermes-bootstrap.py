"""Policy unit tests; no model, provider, global profile or subprocess started."""
import importlib.util
import asyncio
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

_path = Path(__file__).resolve().parents[2] / "src-tauri/src/hermes/bootstrap.py"
_spec = importlib.util.spec_from_file_location("coucou_hermes_bootstrap", _path)
adapter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adapter)


class ManagedIsolation(unittest.TestCase):
    def test_runtime_tools_reach_model_only_through_owned_mcp(self):
        model = {"default": "test-model", "base_url": "http://127.0.0.1:14500/v1"}
        agent = self.agent()
        self.assertEqual(len(adapter.TOOLS), 16)
        for name in ("query_processes", "open_directory", "organize_files_dry_run", "cli_version"):
            self.assertIn("mcp__coucou__" + name, adapter.TOOLS)
            adapter.validate_model_calls(agent, SimpleNamespace(tool_calls=[
                SimpleNamespace(function=SimpleNamespace(name="mcp__coucou__" + name))]), model, "owned-test-key")
        adapter.validate_outbound_request(agent, {"model": "test-model", "tools": agent.tools}, model, "owned-test-key")

    def agent(self):
        return SimpleNamespace(tools=[{"type": "function", "function": {"name": name}}
                                      for name in sorted(adapter.TOOLS)],
            valid_tool_names=set(adapter.TOOLS), provider="custom", model="test-model",
            base_url="http://127.0.0.1:14500/v1", api_key="owned-test-key", api_mode="chat_completions",
            _fallback_chain=[], _memory_manager=None, _memory_enabled=False,
            _environment_probe=False, _skip_mcp_refresh=True, skip_background_review=True, _auto_recovery_cycles=0,
            _api_max_retries=1,
            _client_kwargs={"api_key": "owned-test-key", "base_url": "http://127.0.0.1:14500/v1",
                            "timeout": adapter.MODEL_REQUEST_TIMEOUT_SECONDS, "max_retries": 0})

    def test_extra_native_tool_missing_bridge_and_duplicate_are_rejected(self):
        agent = self.agent()
        self.assertEqual(adapter.effective_names(agent), adapter.TOOLS)
        for bad in ("terminal", "read_file", "delegate_task", "mcp__foreign__send"):
            with self.subTest(tool=bad):
                agent = self.agent()
                agent.tools[0]["function"]["name"] = bad
                agent.valid_tool_names = {item["function"]["name"] for item in agent.tools}
                with self.assertRaises(adapter.IsolationError):
                    adapter.effective_names(agent)
        agent = self.agent()
        agent.tools[0] = agent.tools[1]
        with self.assertRaises(adapter.IsolationError):
            adapter.effective_names(agent)

    def test_route_fallback_background_or_auth_drift_are_rejected_without_secret(self):
        model = {"default": "test-model", "base_url": "http://127.0.0.1:14500/v1"}
        adapter.validate_route(self.agent(), model, "owned-test-key")
        for attr, value in (("base_url", "https://unexpected.example/v1"),
                            ("api_key", "inherited-key"), ("provider", "openrouter"),
                            ("_fallback_chain", [{"provider": "paid"}]),
                            ("_memory_enabled", True), ("_environment_probe", True),
                            ("_skip_mcp_refresh", False),
                            ("_client_kwargs", {"api_key": "inherited-key", "base_url": "http://127.0.0.1:14500/v1",
                                                "timeout": 180, "max_retries": 0}),
                            ("_client_kwargs", {"api_key": "owned-test-key", "base_url": "http://127.0.0.1:14500/v1",
                                                "timeout": 1800, "max_retries": 0}),
                            ("_client_kwargs", {"api_key": "owned-test-key", "base_url": "http://127.0.0.1:14500/v1",
                                                "timeout": 180, "max_retries": 2}),
                            ("skip_background_review", False), ("_auto_recovery_cycles", 1)):
            with self.subTest(attribute=attr):
                agent = self.agent()
                setattr(agent, attr, value)
                with self.assertRaises(adapter.IsolationError) as caught:
                    adapter.validate_route(agent, model, "owned-test-key")
                self.assertNotIn("owned-test-key", str(caught.exception))
                self.assertNotIn("inherited-key", str(caught.exception))

    def test_raw_native_or_mixed_batch_latches_failure_before_upstream_repair(self):
        model = {"default": "test-model", "base_url": "http://127.0.0.1:14500/v1"}
        permitted = "mcp__coucou__context_read"
        for names in (("terminal",), (permitted, "read_file"), ("skill_manage", permitted)):
            with self.subTest(names=names):
                agent = self.agent()
                message = SimpleNamespace(tool_calls=[SimpleNamespace(function=SimpleNamespace(name=name)) for name in names])
                with self.assertRaises(adapter.IsolationError):
                    adapter.validate_model_calls(agent, message, model, "owned-test-key")
                self.assertTrue(agent._coucou_turn_failure)
                self.assertEqual([call.function.name for call in message.tool_calls], list(names))
        agent = self.agent()
        adapter.validate_model_calls(agent, SimpleNamespace(tool_calls=[
            SimpleNamespace(function=SimpleNamespace(name=permitted))]), model, "owned-test-key")
        self.assertFalse(getattr(agent, "_coucou_turn_failure", False))

    def test_hidden_provider_control_and_slash_commands_are_denied(self):
        for text in ("/model other", "\n /moa question", adapter.MOA_MARKER_PREFIX + "encoded", "  " + adapter.MOA_MARKER_PREFIX + "encoded"):
            with self.subTest(text=text):
                with self.assertRaises(adapter.IsolationError):
                    adapter.validate_prompt_text(text)
        with self.assertRaises(adapter.IsolationError):
            adapter.managed_decode_moa_turn(adapter.MOA_MARKER_PREFIX + "encoded")
        ordinary = "Explique este texto: " + adapter.MOA_MARKER_PREFIX
        adapter.validate_prompt_text(ordinary)
        self.assertEqual(adapter.managed_decode_moa_turn(ordinary), (ordinary, None))

    def test_structured_failure_is_not_success_or_provider_error_prose(self):
        for result in ({"failed": True, "completed": False, "final_response": "Provider rejected inherited-key"},
                       {"error": "owned-test-key", "final_response": "Billing refused"},
                       {"final_response": "Error: internal"}, None):
            with self.subTest(result=result):
                agent = self.agent()
                with self.assertRaises(adapter.IsolationError) as caught:
                    adapter.validate_turn_result(agent, result)
                self.assertTrue(agent._coucou_turn_failure)
                self.assertEqual(str(caught.exception), "Managed Hermes turn failed")
        agent = self.agent()
        adapter.validate_turn_result(agent, {"failed": False, "completed": True, "final_response": "Resposta."})
        adapter.validate_turn_result(agent, {"failed": False, "interrupted": True, "final_response": ""})

    def test_connection_refuses_context_below_pinned_hermes_minimum(self):
        with tempfile.TemporaryDirectory(prefix="coucou-hermes-context-") as root:
            home = Path(root)
            config = {"model": {"provider": "custom", "default": "test-model", "base_url": "http://127.0.0.1:12345/v1",
                "api_key": "${COUCOU_HERMES_MODEL_KEY}", "api_mode": "chat_completions"},
                "mcp_servers": {}, "platform_toolsets": {"acp": []}, "plugins": {"enabled": []},
                "tools": {"tool_search": {"enabled": "off"}},
                "auxiliary": {"title_generation": {"enabled": False}},
                "memory": {"memory_enabled": False, "user_profile_enabled": False}, "fallback_model": []}
            with patch.dict(os.environ, {"COUCOU_HERMES_MODEL_KEY": "owned-test-key"}, clear=True):
                for size in (8192, 63999, 1_000_001, 64000.0, None):
                    config["model"]["context_length"] = size
                    (home / "config.yaml").write_text(json.dumps(config), encoding="utf-8")
                    with self.subTest(size=size), self.assertRaises(adapter.IsolationError):
                        adapter.checked_connection(home)
                config["model"]["context_length"] = adapter.MIN_CONTEXT_LENGTH
                (home / "config.yaml").write_text(json.dumps(config), encoding="utf-8")
                model, key = adapter.checked_connection(home)
                self.assertEqual(model["context_length"], 64000)
                self.assertEqual(key, "owned-test-key")

                # Hermes reads tools.tool_search, not a root-level tool_search.
                # A typo otherwise enables progressive disclosure and replaces
                # the 8 owned MCP schemas with 3 unapproved meta tools.
                for tools in ({}, {"tool_search": {"enabled": False}},
                              {"tool_search": {"enabled": "auto"}}):
                    config["tools"] = tools
                    config["tool_search"] = {"enabled": False}
                    (home / "config.yaml").write_text(json.dumps(config), encoding="utf-8")
                    with self.subTest(tools=tools), self.assertRaises(adapter.IsolationError):
                        adapter.checked_connection(home)
                config["tools"] = {"tool_search": {"enabled": "off"}}
                for auxiliary in ({}, {"session_title": {"enabled": False}},
                                  {"title_generation": {"enabled": True}}):
                    config["auxiliary"] = auxiliary
                    (home / "config.yaml").write_text(json.dumps(config), encoding="utf-8")
                    with self.subTest(auxiliary=auxiliary), self.assertRaises(adapter.IsolationError):
                        adapter.checked_connection(home)

    def test_administrative_overlay_is_disabled_without_reading_global_files(self):
        reads = []
        managed = SimpleNamespace(
            get_managed_dir=lambda: reads.append("global-directory"),
            load_managed_config=lambda: reads.append("global-config"),
            load_managed_env=lambda: reads.append("global-env"),
            apply_managed_overlay=lambda config: reads.append("global-overlay"),
            invalidate_managed_cache=lambda: reads.append("cache-invalidated"))
        adapter.disable_managed_scope(managed)
        owned = {"tools": {"tool_search": {"enabled": "off"}}}
        self.assertIsNone(managed.get_managed_dir())
        self.assertEqual(managed.load_managed_config(), {})
        self.assertEqual(managed.load_managed_env(), {})
        self.assertIs(managed.apply_managed_overlay(owned), owned)
        self.assertEqual(reads, ["cache-invalidated"])

    def test_auxiliary_sync_and_async_model_calls_cannot_run(self):
        with self.assertRaises(adapter.IsolationError) as caught:
            adapter.deny_auxiliary_call("title", api_key="must-not-appear")
        self.assertNotIn("must-not-appear", str(caught.exception))
        with self.assertRaises(adapter.IsolationError):
            asyncio.run(adapter.deny_async_auxiliary_call("compression"))

    def test_provider_plugin_registry_cannot_discover_or_import_profiles(self):
        discovery = []
        providers = SimpleNamespace(
            get_provider_profile=lambda *args: discovery.append("lookup"),
            list_providers=lambda: discovery.append("list"),
            _discover_providers=lambda: discovery.append("discovery"),
            _run_discovery_steps=lambda: discovery.append("bundled-pip-home"),
            _home_layer=lambda: discovery.append("home"),
            _HomeLayer=lambda: SimpleNamespace(registry={}, aliases={}))
        adapter.disable_provider_plugins(providers)
        self.assertIsNone(providers.get_provider_profile("paid-profile"))
        self.assertEqual(providers.list_providers(), [])
        providers._discover_providers()
        providers._run_discovery_steps()
        self.assertEqual(providers._home_layer().registry, {})
        self.assertEqual(discovery, [])

    def test_outbound_model_calls_refuse_drift_and_never_retry_a_policy_failure(self):
        model = {"default": "test-model", "base_url": "http://127.0.0.1:14500/v1"}
        request = {"model": "test-model", "tools": self.agent().tools}
        agent = self.agent()
        adapter.validate_outbound_request(agent, request, model, "owned-test-key")
        adapter.abort_policy_retry(agent, RuntimeError("ordinary transport error"))
        for bad in (None, {"model": "another", "tools": request["tools"]},
                    {"model": "test-model", "tools": []},
                    {"model": "test-model", "tools": request["tools"] + [{"function": {"name": "terminal"}}]}):
            with self.subTest(request=bad):
                agent = self.agent()
                with self.assertRaises(adapter.IsolationError):
                    adapter.validate_outbound_request(agent, bad, model, "owned-test-key")
                self.assertTrue(agent._coucou_turn_failure)
                with self.assertRaises(adapter.IsolationError):
                    adapter.validate_outbound_request(agent, request, model, "owned-test-key")
                with self.assertRaises(adapter.IsolationError):
                    adapter.abort_policy_retry(agent, RuntimeError("upstream retry"))
        agent = self.agent()
        with self.assertRaises(adapter.IsolationError):
            adapter.abort_policy_retry(agent, adapter.IsolationError("native tool denied"))
        self.assertTrue(agent._coucou_turn_failure)

    def test_actual_request_client_requires_owned_auth_bounded_timeout_and_no_sdk_retry(self):
        model = {"default": "test-model", "base_url": "http://127.0.0.1:14500/v1"}
        def client(timeout=180, retries=0, key="owned-test-key", base=model["base_url"]):
            return SimpleNamespace(timeout=timeout, max_retries=retries, api_key=key, base_url=base)
        adapter.validate_request_client(client(), model, "owned-test-key")
        adapter.validate_request_client(client(SimpleNamespace(connect=10, read=180, write=180, pool=180)), model, "owned-test-key")
        for bad in (client(600), client(None), client(180, 2), client(key="inherited-key"),
                    client(base="https://unexpected.example/v1"),
                    client(SimpleNamespace(connect=10, read=None, write=180, pool=180))):
            with self.subTest(client=bad), self.assertRaises(adapter.IsolationError):
                adapter.validate_request_client(bad, model, "owned-test-key")

    def test_profile_env_and_unowned_home_are_refused_and_preserved(self):
        with tempfile.TemporaryDirectory(prefix="coucou-hermes-policy-") as root:
            source, home = Path(root) / "source", Path(root) / "profile"
            source.mkdir()
            home.mkdir()
            marker = home / "coucou-managed.json"
            marker.write_text(json.dumps({"owner": "coucou", "revision": adapter.REVISION}), encoding="utf-8")
            with patch.dict(os.environ, {"HERMES_HOME": str(home)}, clear=True):
                self.assertEqual(adapter.checked_paths(str(source), str(home)), (source.resolve(), home.resolve()))
                for unexpected in (home / ".env", source / ".env", home / "auth.json"):
                    unexpected.write_text("preserved-user-content", encoding="utf-8")
                    with self.assertRaises(adapter.IsolationError):
                        adapter.checked_paths(str(source), str(home))
                    self.assertEqual(unexpected.read_text(encoding="utf-8"), "preserved-user-content")
                    unexpected.unlink()
                marker.write_text('{"owner":"another-app"}', encoding="utf-8")
                with self.assertRaises(adapter.IsolationError):
                    adapter.checked_paths(str(source), str(home))

    def test_bridge_accepts_only_owned_command_shape_and_namespaced_environment(self):
        with tempfile.TemporaryDirectory(prefix="coucou-hermes-bridge-") as root:
            executable = Path(root) / "coucou.exe"
            executable.write_bytes(b"fixture")
            spec = {"command": str(executable), "args": ["--hermes-tool-bridge"],
                    "environment": {"COUCOU_BRIDGE_ENDPOINT": "127.0.0.1:12345"}}
            with patch.dict(os.environ, {"COUCOU_HERMES_BRIDGE_SPEC": json.dumps(spec)}, clear=True):
                self.assertEqual(adapter.checked_bridge(), spec)
            spec["environment"]["OPENAI_API_KEY"] = "must-not-be-inherited"
            with patch.dict(os.environ, {"COUCOU_HERMES_BRIDGE_SPEC": json.dumps(spec)}, clear=True):
                with self.assertRaises(adapter.IsolationError):
                    adapter.checked_bridge()
            del spec["environment"]["OPENAI_API_KEY"]
            spec["args"] = ["arbitrary-shell"]
            with patch.dict(os.environ, {"COUCOU_HERMES_BRIDGE_SPEC": json.dumps(spec)}, clear=True):
                with self.assertRaises(adapter.IsolationError):
                    adapter.checked_bridge()


if __name__ == "__main__":
    unittest.main(verbosity=2)
