/** The personal editor owns only these preferences, never the official chat/provider settings. */
export function mergePersonalPreferences<T extends object>(current: T, personal: {
  visibilityMode: "always" | "autoHide"; userPinned: boolean;
}): T {
  return { ...current, visibilityMode: personal.visibilityMode, userPinned: personal.userPinned };
}
