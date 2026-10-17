/**
 * Dev-Selbsttest in der echten WebView (WebKitGTK + Tauri-IPC + Rust-Backend), ohne Eingaben am
 * Desktop: `VITE_TASKS_SELFTEST=1 pnpm tauri dev` mit leerem `TODO_DATA_DIR`. Ergebnis als Overlay
 * oben rechts (PASS/FAIL je Schritt). In normalen Builds nicht enthalten.
 */

import type { CoreClient } from "@tasks/core-client";

/** Ein Schritt; ein zurückgegebener Text erscheint als Messwert hinter PASS. */
type Step = [name: string, run: (client: CoreClient) => Promise<string | void>];

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function until<T>(what: string, probe: () => T | null | undefined | false, timeoutMs = 3000): Promise<T> {
  const end = performance.now() + timeoutMs;
  for (;;) {
    const value = probe();
    if (value) return value;
    if (performance.now() > end) throw new Error(`timeout: ${what}`);
    await sleep(25);
  }
}

const heading = () => document.querySelector("h1")?.textContent?.trim();

function byRole(role: string, name: string | RegExp): HTMLElement | null {
  const nodes = [...document.querySelectorAll<HTMLElement>(role === "link" ? "a[href]" : `[role="${role}"], ${role}`)];
  return (
    nodes.find((n) => {
      const label = n.getAttribute("aria-label") ?? n.textContent?.trim() ?? "";
      return typeof name === "string" ? label === name : name.test(label);
    }) ?? null
  );
}

async function click(role: string, name: string | RegExp) {
  (await until(`${role} ${name}`, () => byRole(role, name))).click();
}

function type(input: HTMLInputElement | HTMLTextAreaElement, text: string) {
  input.focus();
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function key(target: EventTarget, key: string, init: KeyboardEventInit = {}) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
}

export const steps: Step[] = [
  ["App startet im Eingang", async () => void (await until("h1 Eingang", () => heading() === "Eingang"))],
  [
    "Navigation zu Heute",
    async () => {
      await click("link", /^Heute/);
      await until("h1 Heute", () => heading() === "Heute");
    },
  ],
  [
    "Liste anlegen und öffnen",
    async () => {
      await click("button", "Neue Liste");
      const input = await until("Listen-Eingabe", () => document.querySelector<HTMLInputElement>('input[aria-label="Name der neuen Liste"]'));
      type(input, "Selbsttest");
      key(input, "Enter");
      await until("h1 Selbsttest", () => heading() === "Selbsttest");
    },
  ],
  [
    "Zurück zum Eingang",
    async () => {
      await click("link", /^Eingang/);
      await until("h1 Eingang", () => heading() === "Eingang");
    },
  ],
];

const frame = () => new Promise<number>((r) => requestAnimationFrame(r));

function stats(deltas: number[]): { p50: number; p95: number; max: number; dropped: number } {
  const sorted = [...deltas].sort((a, b) => a - b);
  const q = (p: number) => sorted[Math.floor(p * (sorted.length - 1))];
  return { p50: q(0.5), p95: q(0.95), max: sorted[sorted.length - 1], dropped: deltas.filter((d) => d > q(0.5) * 1.5).length };
}

// P7-07: Task-Liste mit 1 000 Tasks.
steps.push(
  [
    "1 000 Tasks anlegen (IPC)",
    async (client) => {
      const t0 = performance.now();
      for (let i = 0; i < 1000; i += 50) {
        await Promise.all(
          Array.from({ length: 50 }, (_, j) => client.createTask({ title: `Selbsttest-Task ${String(i + j + 1).padStart(4, "0")}` })),
        );
      }
      return `${Math.round(performance.now() - t0)} ms`;
    },
  ],
  [
    "Liste virtualisiert 1 000 Zeilen",
    async () => {
      await until("Zähler 999+", () => document.querySelector('a[href="#/inbox"]')?.textContent?.includes("999+"), 8000);
      const rows = await until("Zeilen", () => {
        const n = document.querySelectorAll('[role="option"]').length;
        return n > 0 && n;
      });
      if (rows > 100) throw new Error(`${rows} DOM-Zeilen`);
      return `${rows} DOM-Zeilen`;
    },
  ],
  [
    "Scrollen durch 1 000 Zeilen",
    async () => {
      const viewport = document.querySelector<HTMLElement>('[role="listbox"]')!;
      const deltas: number[] = [];
      let prev = await frame();
      while (viewport.scrollTop + viewport.clientHeight < viewport.scrollHeight - 1) {
        viewport.scrollTop += 96;
        const now = await frame();
        deltas.push(now - prev);
        prev = now;
      }
      viewport.scrollTop = 0;
      const s = stats(deltas.slice(1));
      if (s.p95 > 25) throw new Error(`p95 ${s.p95.toFixed(1)} ms`);
      return `${deltas.length} Frames, p50 ${s.p50.toFixed(1)} / p95 ${s.p95.toFixed(1)} / max ${s.max.toFixed(1)} ms, ${s.dropped} Drops`;
    },
  ],
  [
    "Abhaken sofort sichtbar",
    async () => {
      await frame();
      const box = await until("Haken", () => document.querySelector<HTMLElement>('[role="option"] [role="checkbox"]'));
      let dom = 0;
      const checked = new Promise<void>((resolve) => {
        const obs = new MutationObserver(() => {
          if (box.getAttribute("aria-checked") !== "true") return;
          dom = performance.now() - t0;
          obs.disconnect();
          resolve();
        });
        obs.observe(box, { attributes: true });
      });
      const t0 = performance.now();
      box.click();
      await checked;
      const painted = (await frame()) - t0;
      if (painted > 50) throw new Error(`${painted.toFixed(1)} ms bis Frame`);
      return `DOM ${dom.toFixed(1)} ms, nächster Frame ${painted.toFixed(1)} ms`;
    },
  ],
);

export async function runSelftest(client: CoreClient, extra: Step[] = []): Promise<void> {
  const box = document.createElement("pre");
  box.style.cssText =
    "position:fixed;top:8px;right:8px;z-index:9999;margin:0;padding:8px 12px;max-width:520px;font:12px/1.5 monospace;background:#000c;color:#fff;border-radius:6px;white-space:pre-wrap";
  document.body.append(box);
  const lines: string[] = [];
  let failed = 0;
  for (const [name, run] of [...steps, ...extra]) {
    try {
      const detail = await run(client);
      lines.push(`PASS ${name}${detail ? ` – ${detail}` : ""}`);
    } catch (e) {
      failed++;
      lines.push(`FAIL ${name}: ${(e as Error).message}`);
    }
    box.textContent = lines.join("\n");
  }
  lines.push(failed ? `${failed} FEHLGESCHLAGEN` : "ALLE BESTANDEN");
  box.textContent = lines.join("\n");
  box.style.background = failed ? "#a00e" : "#060e";
}
