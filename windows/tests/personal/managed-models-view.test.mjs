import test from "node:test";
import assert from "node:assert/strict";
import { buildManagedModelsSection } from "../../src/settings/managed-models.ts";
import { Bridge } from "../../src/personal/core/bridge.ts";

// DOM interaction fixture: no Tauri, provider key, process, or model is started.
class Element {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.attributes = new Map();
    this.handlers = new Map();
    this.value = "";
    this.disabled = false;
    this.hidden = false;
    this.checked = false;
    this._text = "";
  }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  get firstChild() { return this.children[0]; }
  setAttribute(name, value) {
    this.attributes.set(name, String(value));
    if (name === "value") this.value = String(value);
    if (name === "disabled") this.disabled = true;
  }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  append(...nodes) { this.children.push(...nodes); }
  removeChild(node) { this.children.splice(this.children.indexOf(node), 1); }
  addEventListener(name, handler) { this.handlers.set(name, [...(this.handlers.get(name) ?? []), handler]); }
  async click() {
    assert.equal(this.disabled, false, `disabled button: ${this.textContent}`);
    for (const handler of this.handlers.get("click") ?? []) await handler({ preventDefault() {}, stopPropagation() {} });
    await new Promise((resolve) => setImmediate(resolve));
  }
}
function all(element) { return element.children.flatMap((child) => [child, ...all(child)]); }

test("disabling free models uses saved settings despite enabled autostart and invalid unsaved drafts", async (t) => {
  const previousDocument = globalThis.document;
  const previousStatus = Bridge.managedServiceStatus;
  const previousConfigure = Bridge.managedServiceConfigure;
  globalThis.document = {
    createElement: (tag) => new Element(tag),
    createTextNode: (text) => { const node = new Element("#text"); node.textContent = text; return node; },
  };
  t.after(() => {
    globalThis.document = previousDocument;
    Bridge.managedServiceStatus = previousStatus;
    Bridge.managedServiceConfigure = previousConfigure;
  });

  const settings = Object.freeze({
    setupDecision: "accepted", enabled: true, agentName: "Meu Hermes", autostart: true,
    approvedProviders: Object.freeze(["groq"]),
    model: Object.freeze({ platform: "groq", modelId: "saved-model" }),
  });
  const status = {
    settings, runtimeInstalled: true, running: true, managedByOtherInstance: false,
    port: 12345, processId: 6789, availableProviders: ["groq"],
    revision: "fixture", nodeVersion: "fixture", lastError: null,
  };
  const configurations = [];
  Bridge.managedServiceStatus = async () => status;
  Bridge.managedServiceConfigure = async (next) => {
    configurations.push(structuredClone(next));
    return { ...status, settings: next, running: false, port: null, processId: null };
  };

  const view = await buildManagedModelsSection();
  const field = (label) => all(view).find((node) => node.getAttribute("aria-label") === label);
  assert.equal(field("Iniciar serviço ao entrar no Windows").checked, true);
  field("Nome de exibição do Hermes pessoal").value = "";
  field("Provedor autorizado").value = "cerebras";
  field("Modelo do agente").value = "auto:invalid-unsaved-model";
  const disable = all(view).find((node) => node.tagName === "BUTTON" && node.textContent === "Desabilitar modo gratuito");
  assert(disable, "the active optional mode must expose its disable action");

  await disable.click();

  assert.deepEqual(configurations, [{ ...settings, enabled: false, autostart: false }]);
  assert.equal(settings.enabled, true, "the saved snapshot must not be mutated");
  assert.equal(settings.autostart, true);
  assert.match(view.textContent, /Operação concluída\./);
  assert.match(view.textContent, /Serviço: parado\./);
  assert(all(view).some((node) => node.tagName === "BUTTON" && node.textContent === "Habilitar modo gratuito"));
  assert.equal(field("Nome de exibição do Hermes pessoal").value, "Meu Hermes", "invalid drafts must not overwrite the saved agent name");
  assert.equal(field("Provedor autorizado").value, "groq");
  assert.equal(field("Modelo do agente").value, "saved-model");
  assert.equal(field("Iniciar serviço ao entrar no Windows").checked, false);
});
