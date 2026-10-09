import test from "node:test";
import assert from "node:assert/strict";
import { runtimeAvailability } from "../src/core/agent-runtime.ts";

test("runtime availability respects provider, personal mode and platform catalog", () => {
  const tools = [{ name: "coucou_query_processes" }];
  assert.equal(runtimeAvailability("claude", true, tools), null);
  assert.match(runtimeAvailability("hermes", true, tools), /FreeLLMAPI/);
  assert.equal(runtimeAvailability("hermes", false, tools), null);
  assert.equal(runtimeAvailability("codex", false, tools), null);
  assert.equal(runtimeAvailability("codex", true, []), null);
  assert.match(runtimeAvailability("codex", true, tools), /Cada ação pede sua confirmação/);
  assert.match(runtimeAvailability("codex", true, tools), /não move arquivos/);
});
