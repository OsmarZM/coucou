import { State } from "./state";
import { ApprovalPresentation } from "../personal/core/approval-presentation";

const presentation = new ApprovalPresentation();
let open: (() => void) | null = null;
function reveal() { presentation.reveal(State.paused || !!State.pendingApproval || !open, () => open?.()); }

export function connectPersonalAlerts(handler: () => void) {
  open = handler;
  return State.subscribe(reveal);
}
export function updatePersonalApproval(requestId: string | null, active: boolean) {
  presentation.update(requestId, active);
  State.personalAgentApproval = presentation.pending;
  reveal();
}
