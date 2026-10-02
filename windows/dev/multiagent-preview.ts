// Deterministic visual fixture. Not included among Vite's production entries.
import "../src/style.css";
import { State } from "../src/core/state";
import { Island } from "../src/island/island";
import { AGENT_IDS, type AgentEvent } from "../src/core/sessions";

State.loadIntegrationTasks();
for (const [index, agent] of AGENT_IDS.entries()) {
  const event: AgentEvent = {
    protocolVersion: 1, agent, sessionId: `preview-${agent}-one`, eventType: "turnStarted",
    cwd: `D:/Projects/${["Dashboard", "Coucou", "NarraHub", "API"][index]}`, requiresApproval: false,
  };
  State.sessions.apply(event, Date.now());
  State.sessions.apply({ ...event, eventType: "toolStarted", toolName: "Read", summary: "Read · src/main.ts" }, Date.now());
}
State.sessions.apply({
  protocolVersion: 1, agent: "codex", sessionId: "preview-codex-two", eventType: "turnFinished",
  cwd: "D:/Projects/Coucou", turnId: "second-turn", requiresApproval: false,
}, Date.now());
State.syncAgentTasks();
State.setFocus("integration_codex");
const island = new Island(document.getElementById("root")!);
island.launch();
State.isPinned = true;
island.fsm.pinned = true;
island.alert("overview");
