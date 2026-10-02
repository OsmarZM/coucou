import test from "node:test";
import assert from "node:assert/strict";
import { IslandStateMachine } from "../src/island/fsm.ts";

class Clock {
  time = 0;
  nextId = 0;
  tasks = new Map();
  callbacks = new Map();
  now = () => this.time;
  setTimeout = (callback, delay) => {
    const id = ++this.nextId;
    this.tasks.set(id, { at: this.time + delay, callback });
    this.callbacks.set(id, callback);
    return id;
  };
  clearTimeout = (id) => { this.tasks.delete(id); };
  advance(milliseconds) {
    const end = this.time + milliseconds;
    while (true) {
      const next = [...this.tasks.entries()].filter(([, task]) => task.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      this.time = next[1].at;
      this.tasks.delete(next[0]);
      next[1].callback();
    }
    this.time = end;
  }
}
const setup = () => {
  const clock = new Clock();
  return { clock, fsm: new IslandStateMachine(clock) };
};

test("default policy collapses to compact and never hides automatically", () => {
  const { clock, fsm } = setup();
  fsm.forceHome();
  clock.advance(14999);
  assert.equal(fsm.state, "home");
  clock.advance(1);
  assert.equal(fsm.state, "petit");
  clock.advance(3600000);
  assert.equal(fsm.state, "petit");
  assert.equal(clock.tasks.size, 0);
});

test("a persisted user pin opens the panel after the greeting instead of holding the greeting forever", () => {
  const { clock, fsm } = setup();
  fsm.userPinned = true;
  fsm.launch();
  fsm.greetComplete();
  clock.advance(120000);
  assert.equal(fsm.state, "home");
  assert.equal(fsm.userPinned, true);
});

test("autoHide requires separate collapse and hide intervals", () => {
  const { clock, fsm } = setup();
  fsm.visibilityMode = "autoHide";
  fsm.forceHome();
  clock.advance(15000);
  assert.equal(fsm.state, "petit");
  clock.advance(59999);
  assert.equal(fsm.state, "petit");
  clock.advance(1);
  assert.equal(fsm.state, "hidden");
});

for (const start of ["hidden", "petit"]) {
  test(`hover opens directly from ${start} after a short delay`, () => {
    const { clock, fsm } = setup();
    if (start === "petit") fsm.forcePetit();
    fsm.mouseEntered();
    clock.advance(149);
    assert.equal(fsm.state, start);
    clock.advance(1);
    assert.equal(fsm.state, "home");
    clock.advance(60000);
    assert.equal(fsm.state, "home");
    fsm.mouseLeft();
    clock.advance(15000);
    assert.equal(fsm.state, "petit");
  });
}

test("passing across the top briefly cancels opening", () => {
  const { clock, fsm } = setup();
  fsm.mouseEntered();
  clock.advance(100);
  fsm.mouseLeft();
  clock.advance(1000);
  assert.equal(fsm.state, "hidden");
});

for (const reason of ["approval", "focus", "draft", "selection", "drag", "busy"]) {
  test(`${reason} cancels an existing close timer and restarts a full interval after release`, () => {
    const { clock, fsm } = setup();
    fsm.forceHome();
    clock.advance(14000);
    fsm.setRetention(reason, true);
    assert.equal(fsm.collapseDeadline, null);
    clock.advance(120000);
    assert.equal(fsm.state, "home");
    fsm.setRetention(reason, false);
    assert.equal(fsm.collapseDeadline, clock.now() + 15000);
    clock.advance(14999);
    assert.equal(fsm.state, "home");
    clock.advance(1);
    assert.equal(fsm.state, "petit");
  });
}

test("answering an approval never removes user pin", () => {
  const { clock, fsm } = setup();
  fsm.userPinned = true;
  fsm.pinned = true;
  fsm.forceHome();
  fsm.pinned = false;
  clock.advance(120000);
  assert.equal(fsm.state, "home");
  assert.equal(fsm.userPinned, true);
  fsm.forcePetit();
  assert.equal(fsm.userPinned, true);
  fsm.forceHome();
  fsm.userPinned = false;
  clock.advance(15000);
  assert.equal(fsm.state, "petit");
});

test("retentions are independent and preserve drafts after focus leaves", () => {
  const { clock, fsm } = setup();
  fsm.forceHome();
  fsm.setRetention("focus", true);
  fsm.setRetention("draft", true);
  fsm.setRetention("focus", false);
  clock.advance(120000);
  assert.equal(fsm.state, "home");
  fsm.setRetention("draft", false);
  clock.advance(15000);
  assert.equal(fsm.state, "petit");
});

test("late callbacks from canceled timers cannot collapse a newly opened panel", () => {
  const { clock, fsm } = setup();
  fsm.forceHome();
  const stale = clock.callbacks.get(clock.nextId);
  fsm.forcePetit();
  fsm.forceHome();
  stale();
  assert.equal(fsm.state, "home");
  assert.equal(fsm.collapseDeadline, 15000);
});

test("switching to always visible reveals auto-hidden compact but preserves explicit pause", () => {
  const { clock, fsm } = setup();
  fsm.visibilityMode = "autoHide";
  fsm.forcePetit();
  clock.advance(60000);
  fsm.visibilityMode = "always";
  assert.equal(fsm.state, "petit");
  fsm.forceHidden();
  fsm.visibilityMode = "autoHide";
  fsm.visibilityMode = "always";
  assert.equal(fsm.state, "hidden");
  fsm.reveal();
  assert.equal(fsm.state, "petit");
});
