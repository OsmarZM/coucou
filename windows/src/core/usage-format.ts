import { tr, formatText } from "./i18n";
import type { RateWindow } from "./personal-types";

/** Titles describe the duration reported by the CLI, never inferred renewal. */
export function rateWindowTitle(window: Pick<RateWindow, "id" | "durationMinutes">) {
  const minutes = window.durationMinutes;
  if (typeof minutes !== "number" || !Number.isFinite(minutes) || minutes <= 0) return window.id;
  if (minutes === 10080) return tr("usage.windowWeekly");
  if (minutes % 1440 === 0) return formatText("usage.windowDays", { days: minutes / 1440 });
  if (minutes % 60 === 0) return formatText("usage.windowHours", { hours: minutes / 60 });
  return formatText("usage.windowMinutes", { minutes: minutes.toLocaleString("pt-BR") });
}
export function reportedDuration(minutes: number | null) {
  return typeof minutes === "number" && Number.isFinite(minutes) && minutes > 0 ? `${minutes.toLocaleString("pt-BR")} min` : tr("usage.unavailable");
}
