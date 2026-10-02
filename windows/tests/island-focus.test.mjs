import test from "node:test";
import assert from "node:assert/strict";
import { Bridge } from "../src/core/bridge.ts";
import { State } from "../src/core/state.ts";
import { Island } from "../src/island/island.ts";

function fixture(t) {
  const originalFocus = Bridge.focusWindow;
  const originalWindow = globalThis.window;
  const timers = [];
  const requests = [];
  let focused = 0;
  globalThis.window = { setTimeout(callback) { timers.push(callback); return timers.length; } };
  Bridge.focusWindow = (value) => {
    let resolve;
    const promise = new Promise((done) => { resolve = done; });
    requests.push({ value, resolve });
    return promise;
  };
  State.mode = "compact";
  State.view = "overview";
  State.pendingApproval = null;
  State.isPinned = false;
  const island = Object.create(Island.prototype);
  island.focusGeneration = 0;
  island.lastPanel = "overview";
  island.views = new Map([["prompt", { focus() { focused++; } }]]);
  island.fsm = { forceHome() { State.mode = "expanded"; } };
  island.animateGeometry = () => {};
  island.setMode = (mode) => { State.mode = mode; };
  t.after(() => { Bridge.focusWindow = originalFocus; globalThis.window = originalWindow; });
  return { island, requests, timers, focused: () => focused };
}

test("hover or incoming presentation can restore chat without requesting focus", (t) => {
  const f = fixture(t);
  f.island.expand("prompt");
  assert.equal(State.view, "prompt");
  assert.equal(State.mode, "expanded");
  assert.deepEqual(f.requests, []);
  assert.equal(f.focused(), 0);
});

test("an explicit chat tab waits for window focus before scheduling editor focus", async (t) => {
  const f = fixture(t);
  f.island.setView("prompt");
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].value, true);
  assert.equal(f.timers.length, 0);
  f.requests[0].resolve();
  await Promise.resolve();
  assert.equal(f.timers.length, 1);
  f.timers[0]();
  assert.equal(f.focused(), 1);
});

test("late focus acknowledgement after navigation cannot focus a chat restored by hover", async (t) => {
  const f = fixture(t);
  f.island.setView("prompt");
  f.island.expand("overview");
  f.island.expand("prompt");
  f.requests[0].resolve();
  await Promise.resolve();
  assert.equal(f.timers.length, 0);
  assert.equal(f.focused(), 0);
});

test("queued editor focus is invalidated when the user leaves and returns without a click", async (t) => {
  const f = fixture(t);
  f.island.setView("prompt");
  f.requests[0].resolve();
  await Promise.resolve();
  f.island.expand("overview");
  f.island.expand("prompt");
  f.timers[0]();
  assert.equal(f.focused(), 0);
});
