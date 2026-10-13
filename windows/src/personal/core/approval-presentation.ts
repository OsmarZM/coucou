/** Coordinates presentation only; permission still belongs to the Rust broker and an explicit click. */
export class ApprovalPresentation {
  private request: string | null = null;
  private presented: string | null = null;
  private active = false;
  update(request: string | null, active: boolean) {
    if (request !== this.request) this.presented = null;
    this.request = request; this.active = active;
  }
  get pending() { return this.active && this.request !== null; }
  reveal(blocked: boolean, open: () => void) {
    if (blocked || !this.pending || this.presented === this.request) return;
    this.presented = this.request;
    open();
  }
}
