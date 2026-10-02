// Island visibility policy. No DOM or Tauri; all timer transitions are guarded.
export type FsmState = "hidden" | "petit" | "home" | "coucou";
export type VisibilityMode = "always" | "autoHide";
export type RetentionReason = "approval" | "focus" | "draft" | "selection" | "drag" | "busy";

export interface FsmClock {
  now(): number;
  setTimeout(callback: () => void, milliseconds: number): number;
  clearTimeout(id: number): void;
}
type TimerName = "petitHide" | "homeCollapse" | "greetCollapse" | "hoverOpen";

export class IslandStateMachine {
  state: FsmState = "hidden";
  onTransition: ((from: FsmState, to: FsmState) => void) | null = null;
  homeToPetitDelay = 15;
  petitToHiddenDelay = 60;
  greetAutoCollapseDelay = 0.6;
  greetHoverCollapseDelay = 10;
  hoverOpenDelay = 0.15;
  private policy: VisibilityMode = "always";
  private manualPin = false;
  private inside = false;
  private explicitlyHidden = false;
  private reasons = new Set<RetentionReason>();
  private timers: Record<TimerName, number | null> = { petitHide: null, homeCollapse: null, greetCollapse: null, hoverOpen: null };
  private generations: Record<TimerName, number> = { petitHide: 0, homeCollapse: 0, greetCollapse: 0, hoverOpen: 0 };
  collapseDeadline: number | null = null;

  constructor(private clock: FsmClock = {
    now: () => performance.now(),
    setTimeout: (callback, milliseconds) => globalThis.setTimeout(callback, milliseconds),
    clearTimeout: (id) => globalThis.clearTimeout(id),
  }) {}

  get visibilityMode(): VisibilityMode { return this.policy; }
  set visibilityMode(value: VisibilityMode) {
    const next = value === "autoHide" ? "autoHide" : "always";
    if (next === this.policy) return;
    this.policy = next;
    this.clear("petitHide");
    if (next === "always" && this.state === "hidden" && !this.explicitlyHidden) this.transition("petit");
    else this.armOutsideTimers();
  }

  get userPinned(): boolean { return this.manualPin; }
  set userPinned(value: boolean) {
    if (value === this.manualPin) return;
    this.manualPin = value;
    this.retentionChanged();
  }
  // Compatibility with hook approvals; this never changes the user's pin.
  get pinned(): boolean { return this.reasons.has("approval"); }
  set pinned(value: boolean) { this.setRetention("approval", value); }
  get protected(): boolean { return this.manualPin || this.reasons.size > 0; }

  setRetention(reason: RetentionReason, active: boolean) {
    if (this.reasons.has(reason) === active) return;
    if (active) this.reasons.add(reason);
    else this.reasons.delete(reason);
    this.retentionChanged();
  }
  private retentionChanged() {
    this.clear("homeCollapse");
    this.clear("petitHide");
    this.clear("greetCollapse");
    this.armOutsideTimers();
  }
  launch() {
    this.explicitlyHidden = false;
    this.cancelTimers();
    this.transition("coucou");
  }
  mouseEntered() {
    this.inside = true;
    this.clear("petitHide");
    this.clear("homeCollapse");
    if (this.state === "hidden" || this.state === "petit") {
      const expected = this.state;
      this.schedule("hoverOpen", this.hoverOpenDelay, () => {
        if (this.inside && this.state === expected) this.forceHome();
      });
    } else if (this.state === "coucou") this.scheduleGreetCollapse(this.greetHoverCollapseDelay);
  }
  mouseLeft() {
    this.inside = false;
    this.clear("hoverOpen");
    if (this.state === "coucou" && !this.protected) {
      this.clear("greetCollapse");
      this.transition("petit");
    } else this.armOutsideTimers();
  }
  click() {
    if (this.state === "hidden" || this.state === "petit") this.forceHome();
  }
  greetComplete() {
    if (this.state !== "coucou") return;
    if (this.protected) this.forceHome();
    else if (this.timers.greetCollapse == null) this.scheduleGreetCollapse(this.greetAutoCollapseDelay);
  }
  reveal() {
    this.explicitlyHidden = false;
    if (this.state !== "hidden") return;
    this.cancelTimers();
    this.transition("petit");
  }
  forceHome() {
    this.explicitlyHidden = false;
    this.cancelTimers();
    this.transition("home");
    this.armOutsideTimers();
  }
  // Explicit actions may close a protected panel; the user's preference survives.
  forcePetit() {
    this.explicitlyHidden = false;
    this.cancelTimers();
    this.transition("petit");
    this.armOutsideTimers();
  }
  forceHidden() {
    this.explicitlyHidden = true;
    this.cancelTimers();
    this.transition("hidden");
  }
  private armOutsideTimers() {
    if (this.inside || this.protected) return;
    if (this.state === "home" && this.timers.homeCollapse == null) {
      this.schedule("homeCollapse", this.homeToPetitDelay, () => {
        if (this.state === "home" && !this.inside && !this.protected) this.transition("petit");
      });
      this.collapseDeadline = this.clock.now() + this.homeToPetitDelay * 1000;
    } else if (this.state === "petit" && this.policy === "autoHide" && this.timers.petitHide == null) {
      this.schedule("petitHide", this.petitToHiddenDelay, () => {
        if (this.state === "petit" && !this.inside && !this.protected && this.policy === "autoHide") this.transition("hidden");
      });
    }
  }
  private scheduleGreetCollapse(delay: number) {
    if (this.protected) return;
    this.schedule("greetCollapse", delay, () => {
      if (this.state === "coucou" && !this.protected) this.transition("petit");
    });
  }
  private schedule(which: TimerName, seconds: number, callback: () => void) {
    this.clear(which);
    const generation = this.generations[which];
    this.timers[which] = this.clock.setTimeout(() => {
      if (generation !== this.generations[which]) return;
      this.timers[which] = null;
      if (which === "homeCollapse") this.collapseDeadline = null;
      callback();
    }, Math.max(0, seconds) * 1000);
  }
  private clear(which: TimerName) {
    this.generations[which]++;
    const id = this.timers[which];
    if (id != null) this.clock.clearTimeout(id);
    this.timers[which] = null;
    if (which === "homeCollapse") this.collapseDeadline = null;
  }
  cancelTimers() {
    for (const which of Object.keys(this.timers) as TimerName[]) this.clear(which);
  }
  private transition(next: FsmState) {
    if (next === this.state) return;
    const from = this.state;
    this.state = next;
    this.onTransition?.(from, next);
    this.armOutsideTimers();
  }
}
