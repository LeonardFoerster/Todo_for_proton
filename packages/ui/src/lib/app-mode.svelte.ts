/**
 * Modus der App (N5, `notes.md` §5): Tasks oder Notes. Pro Gerät gemerkt (`tasks.mode`); die
 * App startet im zuletzt benutzten Modus.
 */
import { readChoice, writeChoice } from "./preferences";

export type Mode = "tasks" | "notes";

export const MODE_KEY = "tasks.mode";

export const MODE_LABELS: Record<Mode, string> = { tasks: "Tasks", notes: "Notes" };

export class AppMode {
  mode = $state<Mode>(readChoice(MODE_KEY, ["tasks", "notes"] as const, "tasks"));

  /** Wechselt und merkt den Modus; `false`, wenn er schon aktiv war. */
  set(next: Mode): boolean {
    if (next === this.mode) return false;
    this.mode = next;
    writeChoice(MODE_KEY, next);
    return true;
  }

  toggle(): Mode {
    this.set(this.mode === "tasks" ? "notes" : "tasks");
    return this.mode;
  }
}
