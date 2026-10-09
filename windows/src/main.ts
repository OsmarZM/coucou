// Entry point: boot the bridge, wire the island, start the greeting.

import "./style.css";
import { Bridge, IS_TAURI, onEvent } from "./core/bridge";
import { Sound } from "./core/sound";
import { State, type Settings } from "./core/state";
import { Island } from "./island/island";
import { registerHookHandlers, cancelPendingApproval } from "./island/hooks";
import { registerIntegrationHandlers, refreshConfigured } from "./island/integrations";
import { AgentChat, registerAgentChatHandlers, cancelAgentChatTurns, disposeAgentChatHandlers, setHermesDisplayName, refreshAgentChatStatus } from "./core/agent-chat";
import { chatAgentForTask } from "./core/sessions";
import type { ManagedModelStatus } from "./core/bridge";

async function main() {
  const root = document.getElementById("root");
  if (!root) return;

  void Sound.preload();

  const island = new Island(root);

  const boot = await Bridge.boot();
  if (boot) {
    State.appVersion = boot.version;
    State.settings = { ...State.settings, ...boot.settings };
  }
  island.applySettings();
  State.loadIntegrationTasks();
  const applyPersonalAgent=(status:ManagedModelStatus)=>{setHermesDisplayName(status.settings.agentName);const task=State.tasks.find(task=>task.id==="hermes");if(task)task.name=status.settings.agentName;State.notify();};
  if(IS_TAURI)void Bridge.managedServiceStatus().then(applyPersonalAgent).catch(()=>{});
  await onEvent<ManagedModelStatus>("managed-models-changed",status=>{applyPersonalAgent(status);void refreshAgentChatStatus();});
  if (boot && !boot.cursorPoll) island.followPageCursor();

  await onEvent<{ x: number; y: number }>("cursor", ({ x, y }) => island.onCursor(x, y));

  /** Pause has to reach Rust too, or the pollers keep calling out. */
  const setPaused = (on: boolean) => {
    if (State.paused === on) return;
    if (on) {
      cancelPendingApproval(island);
      cancelAgentChatTurns();
    }
    State.paused = on;
    void Bridge.setPaused(on);
    State.notify();
  };

  await onEvent<string>("tray", (what) => {
    switch (what) {
      case "settings":
        setPaused(false);
        island.alert("settings");
        break;
      case "open":
        setPaused(false);
        island.alert(State.defaultView());
        break;
      case "pause":
        setPaused(!State.paused);
        if (State.paused) island.fsm.forceHidden();
        else island.reveal();
        break;
    }
  });

  await onEvent<null>("screen-changed", () => void Bridge.reposition());

  // The settings window writes preferences; apply them here without a restart.
  await onEvent<Settings>("settings-changed", (s) => {
    State.settings = { ...State.settings, ...s };
    island.applySettings();
    State.loadIntegrationTasks();
    void refreshConfigured();
  });

  registerHookHandlers(island);
  registerIntegrationHandlers(island);
  await registerAgentChatHandlers(island);
  // Align the opening overview with the restored character once. Incoming
  // activity does not change the user's later selection.
  const restoredTask = State.tasks.find(task => chatAgentForTask(task.id) === AgentChat.provider);
  if (restoredTask) State.setFocus(restoredTask.id);
  window.addEventListener("beforeunload", () => {
    cancelAgentChatTurns();
    disposeAgentChatHandlers();
  });

  island.launch();

  // In a plain browser there is no wake strip behind the cursor: make the whole
  // page wake the island so the visuals can be checked with `npm run dev`.
  if (!IS_TAURI) {
    document.addEventListener("click", () => Sound.resume(), { once: true });
  }
}

void main();
