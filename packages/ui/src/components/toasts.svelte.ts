/** Dezente Hinweise unten links (ui-guidelines §4): 4 s, optional eine Aktion wie „Rückgängig“. */

export interface ToastOptions {
  message: string;
  /** Beschriftung der Aktion, z. B. „Rückgängig“. */
  actionLabel?: string;
  onAction?: () => void;
  /** Anzeigedauer in ms; 0 = bis zum Schließen. */
  timeoutMs?: number;
  tone?: "default" | "danger";
}

export interface Toast extends ToastOptions {
  id: number;
}

const DEFAULT_TIMEOUT_MS = 4000;
const MAX_VISIBLE = 3;

class ToastStore {
  items = $state<Toast[]>([]);
  private nextId = 1;
  private timers = new Map<number, { handle: ReturnType<typeof setTimeout> | null; left: number; since: number }>();

  show(options: ToastOptions): number {
    const id = this.nextId++;
    this.items = [...this.items, { ...options, id }].slice(-MAX_VISIBLE);
    for (const old of this.timers.keys()) if (!this.items.some((t) => t.id === old)) this.clear(old);
    const timeout = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (timeout > 0) this.start(id, timeout);
    return id;
  }

  dismiss(id: number) {
    this.clear(id);
    this.items = this.items.filter((t) => t.id !== id);
  }

  /** Aktion ausführen und Toast schließen. */
  act(id: number) {
    const toast = this.items.find((t) => t.id === id);
    this.dismiss(id);
    toast?.onAction?.();
  }

  /** Beim Überfahren/Fokussieren anhalten, damit „Rückgängig“ erreichbar bleibt. */
  pause(id: number) {
    const timer = this.timers.get(id);
    if (!timer?.handle) return;
    clearTimeout(timer.handle);
    timer.handle = null;
    timer.left -= Date.now() - timer.since;
  }

  resume(id: number) {
    const timer = this.timers.get(id);
    if (timer && !timer.handle) this.start(id, Math.max(timer.left, 1000));
  }

  private start(id: number, ms: number) {
    this.timers.set(id, { handle: setTimeout(() => this.dismiss(id), ms), left: ms, since: Date.now() });
  }

  private clear(id: number) {
    const timer = this.timers.get(id);
    if (timer?.handle) clearTimeout(timer.handle);
    this.timers.delete(id);
  }
}

export const toasts = new ToastStore();
