"""Owned adapter for a pinned Hermes runtime, never the user's Hermes launcher.

The Rust process owner clears the environment and validates the source. This
adapter additionally rejects inherited configuration, native tools and runtime
route changes. ACP itself does not provide an operating system sandbox.
"""
from __future__ import annotations

import argparse
import asyncio
import json
import logging
import os
from pathlib import Path
import sys

REVISION = "781334eea4b9225a3e194faf0c241d9afe218634"
MIN_CONTEXT_LENGTH = 64_000
MODEL_REQUEST_TIMEOUT_SECONDS = 180.0
TOOLS = frozenset("mcp__coucou__" + name for name in (
    "context_read", "files_request", "files_count", "files_list",
    "document_read", "memory_propose", "skill_propose", "delegate_cli",
    "open_device_manager", "open_windows_settings", "open_application", "open_directory",
    "query_processes", "organize_files_dry_run", "query_recycle_bin", "cli_version",
))
MOA_MARKER_PREFIX = "__HERMES_MOA_TURN_V1__"


class IsolationError(RuntimeError):
    """Deliberately carries no credentials, document text or provider exception."""


def manifest(phase: str, names: set[str] | frozenset[str]) -> dict:
    return {"schemaVersion": 1, "revision": REVISION, "phase": phase,
            "toolNames": sorted(names), "nativeToolsDisabled": True,
            "inheritedConfigurationDisabled": True}


def effective_names(agent) -> set[str]:
    tools = getattr(agent, "tools", None)
    if not isinstance(tools, list) or len(tools) != len(TOOLS):
        raise IsolationError("Managed Hermes tool count changed")
    try:
        names = {tool["function"]["name"] for tool in tools}
    except (TypeError, KeyError):
        raise IsolationError("Managed Hermes tool schema changed") from None
    if names != TOOLS or getattr(agent, "valid_tool_names", None) != TOOLS:
        raise IsolationError("Managed Hermes tool surface changed")
    return names


def validate_route(agent, model: dict, key: str) -> None:
    client_kwargs = getattr(agent, "_client_kwargs", {})
    if (getattr(agent, "provider", None) != "custom"
            or getattr(agent, "model", None) != model["default"]
            or str(getattr(agent, "base_url", "")).rstrip("/") != model["base_url"].rstrip("/")
            or getattr(agent, "api_key", None) != key
            or getattr(agent, "api_mode", None) != "chat_completions"
            or getattr(agent, "_fallback_chain", None)
            or getattr(agent, "_memory_manager", None)
            or getattr(agent, "_memory_enabled", False)
            or getattr(agent, "_environment_probe", False)
            or getattr(agent, "_skip_mcp_refresh", None) is not True
            or getattr(agent, "skip_background_review", None) is not True
            or getattr(agent, "_auto_recovery_cycles", 0) != 0
            or getattr(agent, "_api_max_retries", 0) != 1
            or not isinstance(client_kwargs, dict)
            or client_kwargs.get("api_key") != key
            or str(client_kwargs.get("base_url", "")).rstrip("/") != model["base_url"].rstrip("/")
            or client_kwargs.get("timeout") != MODEL_REQUEST_TIMEOUT_SECONDS
            or client_kwargs.get("max_retries") != 0):
        raise IsolationError("Managed Hermes route or background policy changed")


def validate_model_calls(agent, assistant_message, model: dict, key: str) -> None:
    try:
        effective_names(agent)
        validate_route(agent, model, key)
        for call in getattr(assistant_message, "tool_calls", None) or []:
            if getattr(getattr(call, "function", None), "name", None) not in TOOLS:
                raise IsolationError("Model requested an unapproved Hermes tool")
    except IsolationError:
        agent._coucou_turn_failure = True
        raise


def validate_outbound_request(agent, request, model: dict, key: str) -> None:
    try:
        if getattr(agent, "_coucou_turn_failure", False):
            raise IsolationError("Managed Hermes turn already failed")
        effective_names(agent)
        validate_route(agent, model, key)
        if not isinstance(request, dict) or request.get("model") != model["default"]:
            raise IsolationError("Managed Hermes model request changed")
        tools = request.get("tools")
        if (not isinstance(tools, list) or len(tools) != len(TOOLS)
                or {tool.get("function", {}).get("name") for tool in tools} != TOOLS):
            raise IsolationError("Managed Hermes outbound tools changed")
    except (IsolationError, AttributeError, TypeError):
        agent._coucou_turn_failure = True
        raise IsolationError("Managed Hermes outbound request denied") from None


def abort_policy_retry(agent, error) -> None:
    # The upstream outer loop otherwise retries escaped tool-validation errors
    # eight times. A policy failure ends this turn before another model call.
    if isinstance(error, IsolationError) or getattr(agent, "_coucou_turn_failure", False):
        agent._coucou_turn_failure = True
        raise IsolationError("Managed Hermes turn failed") from None


def validate_request_client(client, model: dict, key: str) -> None:
    timeout = getattr(client, "timeout", None)
    limits = ([timeout] if isinstance(timeout, (int, float))
              else [getattr(timeout, name, None) for name in ("connect", "read", "write", "pool")])
    if (getattr(client, "max_retries", None) != 0
            or getattr(client, "api_key", None) != key
            or str(getattr(client, "base_url", "")).rstrip("/") != model["base_url"].rstrip("/")
            or any(not isinstance(limit, (int, float)) or not 0 < limit <= MODEL_REQUEST_TIMEOUT_SECONDS
                   for limit in limits)):
        raise IsolationError("Managed Hermes request client changed")


def validate_prompt_text(text: str) -> None:
    if text.lstrip().startswith(("/", MOA_MARKER_PREFIX)):
        raise IsolationError("Hermes control commands are disabled")


def managed_decode_moa_turn(message):
    if isinstance(message, str) and message.startswith(MOA_MARKER_PREFIX):
        raise IsolationError("Hermes inline provider orchestration is disabled")
    return message, None


def validate_turn_result(agent, result):
    if (not isinstance(result, dict)
            or getattr(agent, "_coucou_turn_failure", False)
            or result.get("failed") or result.get("error")
            or str(result.get("final_response") or "").startswith("Error:")):
        agent._coucou_turn_failure = True
        raise IsolationError("Managed Hermes turn failed")


def checked_paths(source: str, profile: str) -> tuple[Path, Path]:
    src, home = Path(source), Path(profile)
    if not src.is_absolute() or not home.is_absolute():
        raise IsolationError("Absolute managed paths required")
    src, home = src.resolve(strict=True), home.resolve(strict=True)
    if src == home or src.is_relative_to(home) or home.is_relative_to(src):
        raise IsolationError("Managed source and profile must be separate")
    if Path(os.environ.get("HERMES_HOME", "")).resolve() != home:
        raise IsolationError("Managed home mismatch")
    marker = json.loads((home / "coucou-managed.json").read_text(encoding="utf-8"))
    if marker.get("owner") != "coucou" or marker.get("revision") != REVISION:
        raise IsolationError("Unowned Hermes profile")
    # A profile never silently grows an alternate auth/config source. Preserve
    # any offending files, but stop rather than importing their contents.
    for path in (home / ".env", home / "auth.json", home / "plugins", src / ".env"):
        if path.exists():
            raise IsolationError("Unexpected Hermes configuration source")
    return src, home


def checked_connection(home: Path) -> tuple[dict, str]:
    config = json.loads((home / "config.yaml").read_text(encoding="utf-8"))
    model = config.get("model", {})
    key = os.environ.get("COUCOU_HERMES_MODEL_KEY", "")
    if (not key or model.get("provider") != "custom"
            or model.get("api_key") != "${COUCOU_HERMES_MODEL_KEY}"
            or model.get("api_mode") != "chat_completions"
            or not model.get("default") or not model.get("base_url")
            or not isinstance(model.get("context_length"), int)
            or not MIN_CONTEXT_LENGTH <= model["context_length"] <= 1_000_000
            or config.get("mcp_servers") != {}
            or config.get("platform_toolsets", {}).get("acp") != []
            or config.get("plugins", {}).get("enabled") != []
            or config.get("tools", {}).get("tool_search", {}).get("enabled") != "off"
            or config.get("auxiliary", {}).get("title_generation", {}).get("enabled") is not False
            or config.get("memory", {}).get("memory_enabled") is not False
            or config.get("memory", {}).get("user_profile_enabled") is not False
            or config.get("fallback_model") != []):
        raise IsolationError("Managed connection configuration changed")
    return model, key


def checked_bridge() -> dict:
    spec = json.loads(os.environ.get("COUCOU_HERMES_BRIDGE_SPEC", "{}"))
    if set(spec) != {"command", "args", "environment"}:
        raise IsolationError("Managed bridge specification changed")
    command, args, env = spec["command"], spec["args"], spec["environment"]
    if (not isinstance(command, str) or not Path(command).is_absolute()
            or not Path(command).is_file() or args != ["--hermes-tool-bridge"]
            or not isinstance(env, dict) or not env):
        raise IsolationError("Invalid managed bridge")
    for name, value in env.items():
        if (not isinstance(name, str) or not name.startswith("COUCOU_")
                or not name.replace("_", "").isalpha() or name.upper() != name
                or not isinstance(value, str) or len(value) > 16384
                or any(ord(char) < 32 for char in value)):
            raise IsolationError("Invalid managed bridge environment")
    return spec


def disable_managed_scope(module) -> None:
    # This administrative overlay is independent of HOME and otherwise probes
    # /etc/hermes even in an env-cleared Windows child.
    module.get_managed_dir = lambda: None
    module.load_managed_config = lambda: {}
    module.load_managed_env = lambda: {}
    module.apply_managed_overlay = lambda config: config
    module.invalidate_managed_cache()


def disable_provider_plugins(module) -> None:
    # Provider profiles have a separate registry from general Hermes plugins.
    # Only the explicit custom route is used; no package, home or bundled
    # provider profile may execute during credential-pool imports.
    module.get_provider_profile = lambda *args, **kwargs: None
    module.list_providers = lambda: []
    module._discover_providers = lambda: None
    module._run_discovery_steps = lambda: None
    module._home_layer = lambda: module._HomeLayer()


def deny_auxiliary_call(*args, **kwargs):
    raise IsolationError("Hermes auxiliary model calls are disabled")


async def deny_async_auxiliary_call(*args, **kwargs):
    raise IsolationError("Hermes auxiliary model calls are disabled")


def install_runtime_guards() -> None:
    # Install guards BEFORE run_agent/model_tools imports. Hermes otherwise
    # reads its checkout .env and discovers bundled, pip and project plugins.
    from hermes_cli import env_loader
    env_loader.load_hermes_dotenv = lambda *args, **kwargs: []
    from hermes_cli import managed_scope
    disable_managed_scope(managed_scope)
    import providers
    disable_provider_plugins(providers)
    # Install these before context_compressor/title_generator import their
    # call_llm bindings. Compression is owned by the Coucou context boundary.
    from agent import auxiliary_client
    auxiliary_client.call_llm = deny_auxiliary_call
    auxiliary_client.async_call_llm = deny_async_auxiliary_call
    # plugins_discovery imports config; no config consumer may precede the
    # managed overlay and dotenv guards above.
    from hermes_cli import plugins_discovery, moa_config
    plugins_discovery.plugin_discovery_suppressed = lambda: True
    moa_config.decode_moa_turn = managed_decode_moa_turn
    from hermes_cli import plugins, mcp_startup
    plugins.discover_plugins = lambda *args, **kwargs: None
    plugins.start_background_plugin_discovery = lambda *args, **kwargs: None
    mcp_startup.start_background_mcp_discovery = lambda *args, **kwargs: None
    mcp_startup.ensure_mcp_discovery_before_agent_build = lambda *args, **kwargs: None
    from hermes_cli import left_core_migration
    left_core_migration.recover_at_startup = lambda *args, **kwargs: None


def make_agent_server(model: dict, key: str, bridge: dict, home: Path):
    import acp
    from acp.exceptions import RequestError
    from acp.schema import (AgentCapabilities, Implementation, InitializeResponse,
                           McpServerStdio, PromptCapabilities)
    from acp_adapter.session import SessionManager
    from acp_adapter.server import HermesACPAgent
    from run_agent import AIAgent
    from agent import turn_tool_round, conversation_loop
    import model_tools
    from tools.registry import registry

    original_dispatch = registry.dispatch
    original_handle = model_tools.handle_function_call
    original_execute = AIAgent._execute_tool_calls
    original_validate_calls = turn_tool_round.validate_tool_calls
    original_outer_error = conversation_loop.handle_outer_loop_error
    original_request_client = AIAgent._create_request_openai_client

    def guarded_dispatch(function_name, *args, **kwargs):
        if function_name not in TOOLS:
            raise IsolationError("Native Hermes tool dispatch denied")
        return original_dispatch(function_name, *args, **kwargs)

    def guarded_handle(function_name, *args, **kwargs):
        if function_name not in TOOLS:
            raise IsolationError("Native Hermes function call denied")
        return original_handle(function_name, *args, **kwargs)

    def check_calls(agent, assistant_message):
        validate_model_calls(agent, assistant_message, model, key)

    def guarded_validate_calls(agent, assistant_message, *args, **kwargs):
        # Validate the raw model names before upstream can repair or filter an
        # invalid batch, including mixed native/MCP calls.
        check_calls(agent, assistant_message)
        return original_validate_calls(agent, assistant_message, *args, **kwargs)

    def guarded_execute(agent, assistant_message, *args, **kwargs):
        check_calls(agent, assistant_message)
        return original_execute(agent, assistant_message, *args, **kwargs)

    def guarded_outer_error(agent, *args, **kwargs):
        abort_policy_retry(agent, kwargs.get("e"))
        return original_outer_error(agent, *args, **kwargs)

    def guarded_request_client(agent, *args, **kwargs):
        validate_outbound_request(agent, kwargs.get("api_kwargs"), model, key)
        client = original_request_client(agent, *args, **kwargs)
        try:
            # Inspect the actual request-local SDK instance, not only the
            # kwargs. The already-built shared primary is not used to send.
            validate_request_client(client, model, key)
        except IsolationError:
            agent._coucou_turn_failure = True
            raise
        return client

    registry.dispatch = guarded_dispatch
    model_tools.handle_function_call = guarded_handle
    AIAgent._execute_tool_calls = guarded_execute
    AIAgent._try_refresh_env_client_credentials = lambda *args, **kwargs: False
    turn_tool_round.validate_tool_calls = guarded_validate_calls
    conversation_loop.handle_outer_loop_error = guarded_outer_error
    AIAgent._create_request_openai_client = guarded_request_client

    class ManagedSessions(SessionManager):
        def _make_agent(self, *, session_id, cwd, **kwargs):
            if Path(cwd).resolve() != (home / "workspace").resolve():
                raise IsolationError("Hermes cwd outside managed workspace")
            agent = AIAgent(
                model=model["default"], provider="custom", requested_provider="custom",
                base_url=model["base_url"], api_key=key, api_mode="chat_completions",
                max_iterations=12, max_tokens=4096, run_budget_seconds=180,
                enabled_toolsets=[], disabled_toolsets=None,
                session_id=session_id, session_db=self._get_db(), cwd=cwd,
                platform="acp", quiet_mode=True, save_trajectories=False,
                skip_context_files=True, load_soul_identity=False, skip_memory=True,
                skip_background_review=True, checkpoints_enabled=False, fallback_model=[],
            )
            agent._print_fn = lambda *args, **kwargs: None
            agent._skip_mcp_refresh = True
            # The actual request-local OpenAI client copies these kwargs.
            # Do not inherit the SDK timeout/retry defaults for this route.
            agent._client_kwargs["timeout"] = MODEL_REQUEST_TIMEOUT_SECONDS
            agent._client_kwargs["max_retries"] = 0
            validate_route(agent, model, key)
            return agent

    class ManagedHermes(HermesACPAgent):
        async def initialize(self, protocol_version=None, **kwargs):
            if protocol_version != 1:
                raise RequestError(-32602, "Unsupported protocol version")
            return InitializeResponse(
                protocol_version=1, agent_info=Implementation(name="coucou-hermes", version=REVISION),
                agent_capabilities=AgentCapabilities(load_session=True,
                    prompt_capabilities=PromptCapabilities(image=False)), auth_methods=[],
                field_meta={"coucouIsolation": manifest("policy", TOOLS)},
            )

        def _build_model_state(self, state):
            return None  # No catalog lookup or model picker in the managed route.

        def _schedule_available_commands_update(self, session_id):
            return None

        def _schedule_mcp_late_refresh(self, state):
            return None

        async def _attach_session_mcp(self, state, mcp_servers, *args):
            if mcp_servers:
                raise RequestError(-32602, "Client MCP servers are disabled")
            server = McpServerStdio(name="coucou", command=bridge["command"], args=bridge["args"],
                env=[{"name": name, "value": value} for name, value in bridge["environment"].items()])
            await super()._register_session_mcp_servers(state, [server])
            effective_names(state.agent)
            validate_route(state.agent, model, key)

        async def _session_response_fields(self, state, replay_verb=None):
            fields = await super()._session_response_fields(state, replay_verb)
            fields["field_meta"] = {"coucouIsolation": manifest("ready", effective_names(state.agent))}
            return fields

        async def prompt(self, prompt, session_id, **kwargs):
            state = self.session_manager.get_session(session_id)
            if (state is None or state.is_running or state.queued_prompts
                    or getattr(state.agent, "_coucou_turn_failure", False)):
                raise RequestError(-32602, "Unknown or busy managed session")
            if any(getattr(block, "type", None) != "text" for block in prompt):
                raise RequestError(-32602, "Only prepared text is accepted")
            try:
                validate_prompt_text("".join(getattr(block, "text", "") for block in prompt))
            except IsolationError:
                raise RequestError(-32602, "Hermes control commands are disabled") from None
            effective_names(state.agent)
            validate_route(state.agent, model, key)
            response = await super().prompt(prompt, session_id, **kwargs)
            # Upstream prompt() catches executor errors and returns end_turn.
            # Keep a policy failure from becoming a successful empty response.
            if getattr(state.agent, "_coucou_turn_failure", False):
                raise RequestError(-32000, "Managed Hermes turn failed")
            return response

        def _run_agent_turn(self, **kwargs):
            agent = kwargs["state"].agent
            try:
                effective_names(agent)
                validate_route(agent, model, key)
                result = super()._run_agent_turn(**kwargs)
            except Exception:
                agent._coucou_turn_failure = True
                raise IsolationError("Managed Hermes turn failed") from None
            # The upstream worker converts some exceptions to assistant prose.
            # A provider exception can contain a key/URL; surface an ACP error
            # through the generic Rust diagnostic instead of echoing it.
            validate_turn_result(agent, result)
            effective_names(agent)
            validate_route(agent, model, key)
            return result

        async def authenticate(self, *args, **kwargs):
            raise RequestError(-32601, "Managed authentication is configured by Coucou")

        async def set_session_model(self, *args, **kwargs):
            raise RequestError(-32601, "Managed model changes are disabled")

        async def set_session_mode(self, *args, **kwargs):
            raise RequestError(-32601, "Managed mode changes are disabled")

        async def set_config_option(self, *args, **kwargs):
            raise RequestError(-32601, "Managed configuration changes are disabled")

        async def fork_session(self, *args, **kwargs):
            raise RequestError(-32601, "Managed session forks are disabled")

        async def resume_session(self, *args, **kwargs):
            raise RequestError(-32601, "Use the owned session/load operation")

        async def list_sessions(self, *args, **kwargs):
            raise RequestError(-32601, "Managed session discovery is disabled")

    return acp, ManagedHermes(session_manager=ManagedSessions())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--profile", required=True)
    args = parser.parse_args()
    source, home = checked_paths(args.source, args.profile)
    model, key = checked_connection(home)
    bridge = checked_bridge()
    if (not sys.flags.isolated or sys.version_info[:2] != (3, 14)
            or os.environ.get("HERMES_DISABLE_LAZY_INSTALLS") != "1"):
        raise IsolationError("Managed isolated Python launch required")
    # Neither the workspace nor the profile may shadow runtime modules.
    sys.path.insert(0, str(source))
    os.chdir(home / "workspace")
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8", errors="replace")
    # Upstream logging includes prompt previews. Discard them in this adapter.
    logging.disable(logging.CRITICAL)
    import hermes_bootstrap  # pinned activation, lazy installs disabled
    install_runtime_guards()
    from agent.ssl_verify import install_truststore
    install_truststore()
    acp, agent = make_agent_server(model, key, bridge, home)
    try:
        asyncio.run(acp.run_agent(agent, use_unstable_protocol=True))
    finally:
        agent.session_manager.end_all_sessions()


if __name__ == "__main__":
    try:
        main()
    except BaseException:
        # Provider/library tracebacks can include secrets. The Rust side emits
        # a scoped failure and never retries a prompt after uncertain delivery.
        print("Coucou Hermes runtime failed; inspect managed configuration.", file=sys.stderr)
        raise SystemExit(1) from None
