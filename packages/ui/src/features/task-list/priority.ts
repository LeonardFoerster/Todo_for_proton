import type { TaskSummaryDto } from "@tasks/core-client";

/** Zugänglicher Name der Priorität (Zeile, Karte, Tooltip am Haken – P5-03). */
export const priorityLabel: Record<TaskSummaryDto["priority"], string> = {
  none: "",
  low: "Niedrige Priorität",
  medium: "Mittlere Priorität",
  high: "Hohe Priorität",
};

/** Für den Zeilennamen: „…, hohe Priorität“ (klein, weil mitten im Satz). */
export function priorityInLabel(priority: TaskSummaryDto["priority"]): string {
  const label = priorityLabel[priority];
  return label && label[0].toLowerCase() + label.slice(1);
}
