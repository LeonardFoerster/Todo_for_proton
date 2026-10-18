/**
 * Dev-Selbsttest in der echten WebView (WebKitGTK + Tauri-IPC + Rust-Backend), ohne Eingaben am
 * Desktop: `VITE_TASKS_SELFTEST=1 pnpm tauri dev` mit leerem `TODO_DATA_DIR`. Ergebnis als Overlay
 * oben rechts (PASS/FAIL je Schritt). In normalen Builds nicht enthalten.
 */

import type { CoreClient, TaskSummaryDto } from "@tasks/core-client";

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

async function untilAsync(what: string, probe: () => Promise<boolean>, timeoutMs = 3000): Promise<void> {
  const end = performance.now() + timeoutMs;
  while (!(await probe())) {
    if (performance.now() > end) throw new Error(`timeout: ${what}`);
    await sleep(50);
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

const base: Step[] = [
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
const p707: Step[] = [];
p707.push(
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

const field = (label: string) =>
  until(label, () => document.querySelector<HTMLInputElement & HTMLTextAreaElement>(`[aria-label="${label}"]`));

async function openTask(title: RegExp) {
  (await until(`Zeile ${title}`, () => byRole("option", title))).click();
  await until("Detail-Panel", () => document.querySelector('[aria-label="Task-Details"] textarea'));
}

// P7-08 Teil 1: anlegen, bearbeiten, verschieben (danach App neu starten → p708b).
const p708a: Step[] = [
  [
    "Liste „Projekt“ anlegen",
    async () => {
      await click("button", "Neue Liste");
      const input = await field("Name der neuen Liste");
      type(input, "Projekt");
      key(input, "Enter");
      await until("h1 Projekt", () => heading() === "Projekt");
      await click("link", /^Eingang/);
      await until("h1 Eingang", () => heading() === "Eingang");
    },
  ],
  [
    "Quick-Add mit „morgen“, Feld bleibt fokussiert",
    async () => {
      const input = await field("Neue Task");
      type(input, "Persistenz prüfen morgen");
      key(input, "Enter");
      await until("neue Zeile", () => byRole("option", /^Persistenz prüfen, Fällig/));
      if (document.activeElement !== input) throw new Error("Fokus verloren");
    },
  ],
  [
    "Notizen + Priorität im Detail",
    async () => {
      await openTask(/^Persistenz prüfen/);
      const notes = await field("Notizen");
      type(notes, "Überlebt den Neustart");
      notes.dispatchEvent(new FocusEvent("blur"));
      await click("radio", "Hoch");
      await sleep(300);
    },
  ],
  [
    "In Liste „Projekt“ verschieben",
    async () => {
      const select = await until("Listenauswahl", () => document.querySelector<HTMLSelectElement>("#detail-list"));
      const option = [...select.options].find((o) => o.textContent === "Projekt")!;
      select.value = option.value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      await until("aus Eingang verschwunden", () => !byRole("option", /^Persistenz prüfen/));
    },
  ],
];

// P7-08 Teil 2 (nach Neustart): alles noch da?
const p708b: Step[] = [
  [
    "Nach Neustart: Task in „Projekt“ mit Notiz, Priorität, Fälligkeit",
    async () => {
      await click("link", /^Projekt/);
      await until("h1 Projekt", () => heading() === "Projekt");
      await openTask(/^Persistenz prüfen, Fällig/);
      const notes = await field("Notizen");
      await until("Notiz", () => notes.value === "Überlebt den Neustart");
      const high = await until("Priorität", () => byRole("radio", "Hoch"));
      if (high.getAttribute("aria-checked") !== "true") throw new Error("Priorität nicht hoch");
      const due = await until("Datum", () => document.querySelector<HTMLInputElement>("#detail-due")?.value);
      const tomorrow = new Date(Date.now() + 86_400_000);
      const expected = `${tomorrow.getFullYear()}-${String(tomorrow.getMonth() + 1).padStart(2, "0")}-${String(tomorrow.getDate()).padStart(2, "0")}`;
      if (due !== expected) throw new Error(`Fällig ${due}, erwartet ${expected}`);
      return `fällig ${due}`;
    },
  ],
];

const order = () =>
  [...document.querySelectorAll('[role="listbox"] [role="option"]')].map((r) => r.getAttribute("aria-label"));

async function openList(name: string) {
  await click("link", new RegExp(`^${name}`));
  await until(`h1 ${name}`, () => heading() === name);
  await until("Zeilen", () => document.querySelector('[role="listbox"] [role="option"]'));
}

// P7-09 Teil 1: Tasten, Ziehen, Löschen (danach Neustart → p709b).
const p709a: Step[] = [
  [
    "Liste „Sortierung“ mit S1–S4",
    async (client) => {
      const list = await client.createTaskList("Sortierung");
      for (const t of ["S1", "S2", "S3", "S4"]) await client.createTask({ title: t, listId: list.id });
      await openList("Sortierung");
      await until("4 Zeilen", () => order().length === 4);
    },
  ],
  [
    "Alt+↑ ×3 schiebt S4 nach oben",
    async () => {
      (await until("S4", () => byRole("option", "S4"))).click();
      for (let i = 0; i < 3; i++) {
        key(document.activeElement ?? document.body, "ArrowUp", { altKey: true });
        await sleep(150);
      }
      await until("S4 oben", () => order().join() === "S4,S1,S2,S3");
      return order().join(" ");
    },
  ],
  [
    "S1 ans Ende ziehen",
    async () => {
      const row = (await until("S1", () => byRole("option", "S1")))!;
      const grip = row.querySelector<HTMLElement>(".grip")!;
      const r = row.getBoundingClientRect();
      const listBottom = document.querySelector('[role="listbox"]')!.getBoundingClientRect().top + 4 * 36;
      grip.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientX: r.left + 8, clientY: r.top + 18, bubbles: true }));
      for (const y of [r.top + 40, r.top + 80, listBottom - 2]) {
        dispatchEvent(new PointerEvent("pointermove", { clientX: r.left + 8, clientY: y, bubbles: true }));
        await frame();
        await frame();
      }
      dispatchEvent(new PointerEvent("pointerup", { button: 0, clientX: r.left + 8, clientY: listBottom - 2, bubbles: true }));
      await until("S1 unten", () => order().join() === "S4,S2,S3,S1");
      return order().join(" ");
    },
  ],
  [
    "S2 mit Entf löschen",
    async () => {
      (await until("S2", () => byRole("option", "S2"))).click();
      key(document.activeElement ?? document.body, "Delete");
      await until("S2 weg", () => order().join() === "S4,S3,S1");
      await until("Toast", () => document.querySelector('[role="status"]')?.textContent?.includes("gelöscht"));
      await sleep(400);
      return order().join(" ");
    },
  ],
];

const p709b: Step[] = [
  [
    "Nach Neustart: Reihenfolge S4 S3 S1",
    async () => {
      await openList("Sortierung");
      await until("Reihenfolge", () => order().join() === "S4,S3,S1", 4000);
      return order().join(" ");
    },
  ],
];

/** Taste an das fokussierte Element (wie die echte Tastatur). */
function press(k: string, init: KeyboardEventInit = {}) {
  key(document.activeElement ?? document.body, k, init);
}

// P7-10: anlegen → abhaken → löschen nur mit der Tastatur.
const p710: Step[] = [
  [
    "N fokussiert Quick-Add, Enter legt zwei Tasks an",
    async () => {
      await click("link", /^Eingang/);
      press("n");
      const input = await until("Fokus Quick-Add", () => document.activeElement?.getAttribute("aria-label") === "Neue Task" && (document.activeElement as HTMLInputElement));
      for (const title of ["Tastatur A", "Tastatur B"]) {
        type(input, title);
        press("Enter");
        await until(title, () => byRole("option", title));
      }
      press("Escape");
      if (document.activeElement === input) throw new Error("Quick-Add noch fokussiert");
    },
  ],
  [
    "↓/J wählen aus, Fokus sichtbar",
    async () => {
      press("ArrowDown");
      await sleep(50);
      while (document.activeElement?.getAttribute("aria-label") !== "Tastatur A") {
        press("j");
        await sleep(30);
        if (!document.activeElement?.closest('[role="listbox"]')) throw new Error("Fokus nicht in der Liste");
      }
      const row = document.activeElement as HTMLElement;
      const style = getComputedStyle(row);
      if (style.outlineStyle === "none" || row.getAttribute("aria-selected") !== "true") {
        throw new Error(`outline=${style.outlineStyle}, selected=${row.getAttribute("aria-selected")}`);
      }
      return `Fokusring ${style.outlineWidth} ${style.outlineStyle}`;
    },
  ],
  [
    "Leertaste hakt ab",
    async () => {
      press(" ");
      await until("abgehakt", () => byRole("option", /^Tastatur A, erledigt/));
    },
  ],
  [
    "Enter öffnet, Esc schließt Details",
    async () => {
      press("Enter");
      await until("Detail", () => document.querySelector('[aria-label="Task-Details"]'));
      press("Escape");
      await until("Detail zu", () => !document.querySelector('[aria-label="Task-Details"]'));
    },
  ],
  [
    "J + Entf löscht Tastatur B",
    async () => {
      press("j");
      await until("B ausgewählt", () => document.activeElement?.getAttribute("aria-label") === "Tastatur B");
      press("Delete");
      await until("B weg", () => !byRole("option", "Tastatur B"));
    },
  ],
  [
    "? zeigt die Kürzel",
    async () => {
      press("?");
      const dialog = await until("Dialog", () => document.querySelector<HTMLDialogElement>("dialog[open]"));
      press("Escape");
      await until("Dialog zu", () => !document.querySelector("dialog[open]"));
      return `${dialog.querySelectorAll("dt").length} Kürzel`;
    },
  ],
];

const syncLabel = () => document.querySelector('[aria-label^="Sync-Status:"]')?.getAttribute("aria-label") ?? "";

// P7-11 ohne Konto: Einstellungen, Anbieter-Hinweis, App bleibt lokal nutzbar.
const p711: Step[] = [
  [
    "Ohne Login: Status „Nur auf diesem Gerät“",
    async () => void (await until("Status", () => syncLabel().includes("Nur auf diesem Gerät"))),
  ],
  [
    "Einstellungen (lazy) mit Anbieter und Pflichthinweis",
    async () => {
      await click("button", "Einstellungen");
      const dialog = await until("Dialog", () => document.querySelector<HTMLDialogElement>("dialog[open]"), 5000);
      const login = await until("Login-Button", () =>
        [...dialog.querySelectorAll("button")].find((b) => b.textContent?.includes("Mit Proton anmelden")),
      );
      if ((login as HTMLButtonElement).disabled) throw new Error("Login nicht verfügbar (Schlüsselbund?)");
      if (!dialog.textContent?.includes("not officially supported by Proton")) throw new Error("Hinweis fehlt");
      press("Escape");
      await until("Dialog zu", () => !document.querySelector("dialog[open]"));
    },
  ],
];

// P7-11 live: wartet auf den Login im Browser (durch den Menschen), dann Statuswechsel.
const seen: string[] = [];
const p711live: Step[] = [
  [
    "Anmeldung abwarten (bis 10 min) – bitte in den Einstellungen anmelden",
    async (client) => {
      client.onSyncStatus((st) => {
        if (seen.at(-1) !== st.state) seen.push(st.state);
      });
      const end = Date.now() + 10 * 60_000;
      for (;;) {
        const session = await client.auth.current();
        if (session) return session.accountLabel.replace(/(.).*(@.*)/, "$1…$2");
        if (Date.now() > end) throw new Error("kein Login");
        await sleep(2000);
      }
    },
  ],
  [
    "Status: Synchronisiert",
    async () => {
      await until("Synchronisiert", () => /Synchronisiert|ausstehend/.test(syncLabel()), 120_000);
      return `${syncLabel()} · Verlauf ${seen.join(" → ")}`;
    },
  ],
  [
    "Offline (WebView-Ereignis) → Status Offline",
    async () => {
      dispatchEvent(new Event("offline"));
      await until("Offline", () => syncLabel().includes("Offline"), 10_000);
      return syncLabel();
    },
  ],
  [
    "Wieder online → Synchronisiere → Synchronisiert",
    async () => {
      const before = seen.length;
      dispatchEvent(new Event("online"));
      await until("Synchronisiert", () => seen.length > before && /Synchronisiert/.test(syncLabel()), 120_000);
      return `Verlauf ${seen.join(" → ")}`;
    },
  ],
];

// P8-02: Zwei-Geräte-Abnahme mit zwei Profilen (`tools/acceptance-two-devices.sh`). Jede Suite ist ein
// App-Lauf eines Geräts; das Skript startet/beendet die Läufe und vergleicht die Berichte beider Geräte.

/** Markierungen für das Skript (z. B. Zeitpunkt für `kill -9`). */
let mark: (line: string) => void = () => {};

const ABNAHME = "Abnahme";

async function listId(client: CoreClient, name: string): Promise<string> {
  const list = (await client.listTaskLists()).find((l) => l.name === name);
  if (!list) throw new Error(`Liste ${name} fehlt`);
  return list.id;
}

async function allIn(client: CoreClient, listId: string): Promise<TaskSummaryDto[]> {
  const items: TaskSummaryDto[] = [];
  let cursor: string | null = null;
  do {
    const page = await client.listTasks({ view: { kind: "list", listId } }, { cursor, limit: 500 });
    items.push(...page.items);
    cursor = page.nextCursor;
  } while (cursor);
  return items;
}

async function byTitle(client: CoreClient, list: string, title: RegExp): Promise<TaskSummaryDto> {
  const task = (await allIn(client, await listId(client, list))).find((t) => title.test(t.title));
  if (!task) throw new Error(`Task ${title} fehlt`);
  return task;
}

/** Löst einen Sync aus und wartet, bis ein Lauf danach erfolgreich und ohne Ausstehendes endet. */
async function syncedNow(client: CoreClient, timeoutMs = 120_000): Promise<string> {
  let started = false;
  let done = false;
  const stop = client.onSyncStatus((st) => {
    if (st.state === "syncing" && !started) {
      started = true;
      mark("SYNC_STARTED");
    }
    if (started && st.state === "idle" && st.pendingOps === 0) done = true;
  });
  const t0 = performance.now();
  const before = await client.sync.status();
  if (before.state === "syncing") {
    started = true;
    mark("SYNC_STARTED");
  }
  try {
    await client.sync.now();
    await until("Sync fertig", () => done, timeoutMs);
  } finally {
    stop();
  }
  await until("Anzeige Synchronisiert", () => syncLabel() === "Sync-Status: Synchronisiert", 5000);
  return `${Math.round(performance.now() - t0)} ms, vorher ${before.pendingOps} ausstehend`;
}

const online = async () => {
  dispatchEvent(new Event("online"));
  await sleep(100);
};

const offline: Step = [
  "Netz getrennt (WebView-Ereignis) → Status Offline",
  async () => {
    dispatchEvent(new Event("offline"));
    await until("Offline", () => syncLabel().includes("Offline"), 10_000);
  },
];

/** Zustand der Abnahme-Liste + Konflikt-Task als eine vergleichbare Zeile. */
async function state(client: CoreClient): Promise<string> {
  const tasks = await allIn(client, await listId(client, ABNAHME));
  const t = tasks.find((x) => x.title.startsWith("T"));
  const due = t?.due && "date" in t.due ? t.due.date : "-";
  return `Reihenfolge ${tasks.map((x) => x.title).join(" | ")} · T=${t?.title ?? "fehlt"} fällig ${due}`;
}

// 1. Gerät B läuft schon und wartet; A legt an → erscheint auf B (periodischer Pull/Fokus).
const p802wait: Step[] = [
  ["B bereit: synchronisiert", async (client) => syncedNow(client)],
  [
    "„Von A“ erscheint im Eingang (ohne Zutun, bis 150 s)",
    async () => {
      mark("WAITING");
      const t0 = performance.now();
      await until("Zeile Von A", () => byRole("option", /^Von A/), 150_000);
      return `nach ${Math.round((performance.now() - t0) / 1000)} s`;
    },
  ],
  ["Abnahme-Liste ebenfalls da", async (client) => state(client)],
];

const p802create: Step[] = [
  [
    "Quick-Add „Von A“ im Eingang",
    async () => {
      await until("h1 Eingang", () => heading() === "Eingang");
      const input = await field("Neue Task");
      type(input, "Von A");
      key(input, "Enter");
      await until("Zeile", () => byRole("option", /^Von A/));
    },
  ],
  [
    "Liste „Abnahme“ mit T, X, S1–S4",
    async (client) => {
      const list = await client.createTaskList(ABNAHME);
      for (const title of ["T Konflikt", "X Löschen", "S1", "S2", "S3", "S4"]) {
        await client.createTask({ title, listId: list.id });
      }
    },
  ],
  ["Hochgeladen", async (client) => syncedNow(client)],
  ["Zustand", async (client) => state(client)],
];

// 2. Beide offline, widersprüchliche Änderungen; danach p802sync auf A, B, A.
const p802offA: Step[] = [
  offline,
  [
    "A: T umbenennen, X löschen, S4 nach oben",
    async (client) => {
      const t = await byTitle(client, ABNAHME, /^T /);
      await client.updateTask(t.id, { title: "T neu von A" });
      await client.deleteTask((await byTitle(client, ABNAHME, /^X /)).id);
      const first = (await allIn(client, await listId(client, ABNAHME)))[0];
      await client.moveTask((await byTitle(client, ABNAHME, /^S4$/)).id, { before: first.id });
      await sleep(4000); // Debounce verstreicht – offline darf nichts hochgehen
      const st = await client.sync.status();
      if (st.state !== "offline" || st.pendingOps < 3) throw new Error(`${st.state}, ${st.pendingOps} ausstehend`);
      return `${st.pendingOps} ausstehend`;
    },
  ],
  ["Zustand A (offline)", async (client) => state(client)],
];

const p802offB: Step[] = [
  offline,
  [
    "B: T fällig setzen, X bearbeiten, S1 ans Ende",
    async (client) => {
      const t = await byTitle(client, ABNAHME, /^T /);
      await client.updateTask(t.id, { due: { kind: "date", date: "2026-10-25" } });
      await client.updateTask((await byTitle(client, ABNAHME, /^X /)).id, { title: "X bearbeitet von B" });
      await client.moveTask((await byTitle(client, ABNAHME, /^S1$/)).id, {});
      await sleep(4000);
      const st = await client.sync.status();
      if (st.state !== "offline" || st.pendingOps < 3) throw new Error(`${st.state}, ${st.pendingOps} ausstehend`);
      return `${st.pendingOps} ausstehend`;
    },
  ],
  ["Zustand B (offline)", async (client) => state(client)],
];

const p802sync: Step[] = [
  [
    "Online, synchronisieren",
    async (client) => {
      await online();
      return syncedNow(client);
    },
  ],
  ["Zustand", async (client) => state(client)],
];

// 3. Viele Tasks offline anlegen, dann online: Der Push dauert (Backend mit Verzögerung) und wird von
// außen hart beendet.
const BULK = 2500;
const p802bulk: Step[] = [
  offline,
  [
    `${BULK} Tasks in Liste „Masse“ (offline)`,
    async (client) => {
      const list = await client.createTaskList("Masse");
      for (let i = 0; i < BULK; i += 50) {
        await Promise.all(
          Array.from({ length: 50 }, (_, j) => client.createTask({ title: `M${String(i + j + 1).padStart(4, "0")}`, listId: list.id })),
        );
      }
      mark("BULK_CREATED");
    },
  ],
  [
    "Online, synchronisieren (wird abgeschossen)",
    async (client) => {
      await online();
      return syncedNow(client);
    },
  ],
];

// 4. Sitzung abgelaufen: App bleibt lokal nutzbar.
const p802auth: Step[] = [
  [
    "Status „Anmeldung erforderlich“",
    async (client) => {
      await client.sync.now();
      await until("Anmeldung erforderlich", () => syncLabel().includes("Anmeldung erforderlich"), 30_000);
      return syncLabel();
    },
  ],
  [
    "Lokal weiterarbeiten: Quick-Add „Ohne Sitzung“",
    async (client) => {
      const input = await field("Neue Task");
      type(input, "Ohne Sitzung");
      key(input, "Enter");
      await until("Zeile", () => byRole("option", /^Ohne Sitzung/));
      await sleep(4000);
      const st = await client.sync.status();
      if (st.state !== "authRequired") throw new Error(`Status ${st.state}`);
      return `${st.pendingOps} ausstehend, Status bleibt ${st.state}`;
    },
  ],
];

// P0-02: Sammel-Commands einmal über die echte IPC (ohne UI).
const p002: Step[] = [
  [
    "complete_tasks / move_tasks / delete_tasks / restore_tasks",
    async (client) => {
      const list = await client.createTaskList("Sammel-IPC");
      const ids: string[] = [];
      for (const title of ["s1", "s2", "s3"]) ids.push((await client.createTask({ title, listId: list.id })).id);
      const events: string[][] = [];
      const off = client.onChange((e) => events.push(e.tasks));
      const done = await client.completeTasks([...ids, ids[0]], true);
      const moved = await client.moveTasks([ids[2], ids[1]], { listId: list.id, before: ids[0] });
      const deleted = await client.deleteTasks(ids);
      const restored = await client.restoreTasks(deleted);
      await until("4 Events", () => events.length >= 4);
      off();
      const order = (await client.listTasks({ view: { kind: "list", listId: list.id } }, { limit: 10 })).items;
      const titles = order.map((t) => t.title).join(",");
      if (done.length !== 3 || moved.length !== 2 || deleted.length !== 3 || restored.length !== 3) {
        throw new Error(`Ergebnisse ${done.length}/${moved.length}/${deleted.length}/${restored.length}`);
      }
      if (titles !== "s2,s3,s1" || !order.every((t) => t.status === "done")) throw new Error(`Reihenfolge ${titles}`);
      if (events.length !== 4) throw new Error(`${events.length} Events`);
      return `je 1 Event, Reihenfolge ${titles}`;
    },
  ],
];

// P0-04 bis P0-07: Mehrfachauswahl in der echten WebView (synthetische Ereignisse, keine Eingaben am Desktop).
const SELECT_N = 500;
const multiselectable = () => document.querySelector('[role="listbox"]')?.getAttribute("aria-multiselectable") === "true";
const selectionCount = () => Number(/(\d+) ausgewählt/.exec(document.querySelector('[role="toolbar"] [role="status"]')?.textContent ?? "")?.[1] ?? -1);
const checkedTitles = () =>
  [...document.querySelectorAll<HTMLElement>('[role="option"][aria-selected="true"]')].map((o) => o.getAttribute("aria-label")).join(",");
const row = (title: string) => until(`Zeile ${title}`, () => byRole("option", title));

/** Tasten gehen wie echte an das fokussierte Element (bzw. `body`). */
const focused = (): HTMLElement => (document.activeElement as HTMLElement | null) ?? document.body;

/** Zählt Aufrufe einer Client-Methode (dieselbe Instanz wie die UI). */
function countCalls(client: CoreClient, method: "completeTasks" | "deleteTasks" | "moveTasks" | "restoreTasks" | "getTask") {
  const original = client[method] as (...args: never[]) => Promise<unknown>;
  const counter = { calls: 0, restore: () => void (client[method] = original as never) };
  client[method] = ((...args: never[]) => {
    counter.calls++;
    return original(...args);
  }) as never;
  return counter;
}

/** Titel einer Liste (offen + erledigt) in Anzeigereihenfolge. */
async function titlesOf(client: CoreClient, name: string): Promise<string[]> {
  const list = (await client.listTaskLists()).find((l) => l.name === name)!;
  const titles: string[] = [];
  let cursor: string | null = null;
  do {
    const page = await client.listTasks({ view: { kind: "list", listId: list.id } }, { cursor, limit: 500 });
    titles.push(...page.items.map((t) => t.title));
    cursor = page.nextCursor;
  } while (cursor);
  return titles;
}

const visibleTitles = (n: number) =>
  [...document.querySelectorAll<HTMLElement>('[role="option"]')].slice(0, n).map((o) => o.getAttribute("aria-label")).join(",");

function clickWith(el: HTMLElement, init: MouseEventInit = {}) {
  el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ...init }));
}

const p0sel: Step[] = [
  [
    `Liste „Auswahl“ mit ${SELECT_N} Tasks`,
    async (client) => {
      const list = await client.createTaskList("Auswahl");
      // Nacheinander: parallele Aufrufe vergäben gleiche Sort-Keys (FIX-10), die Reihenfolge wäre zufällig.
      for (let i = 1; i <= SELECT_N; i++) await client.createTask({ title: `W${String(i).padStart(3, "0")}`, listId: list.id });
      location.hash = `#/list/${list.id}`;
      await until("h1 Auswahl", () => heading() === "Auswahl");
      await row("W001");
    },
  ],
  [
    "Modus per Button an, Esc aus",
    async () => {
      await click("button", "Auswählen");
      await until("Modus an", () => multiselectable() && selectionCount() === 0);
      key(focused(), "Escape");
      await until("Modus aus", () => !multiselectable() && byRole("button", "Auswählen"));
    },
  ],
  [
    "Strg+Klick startet ab Cursor, schließt das Detail",
    async () => {
      (await row("W002")).click();
      await until("Detail offen", () => document.querySelector('[aria-label="Task-Details"]'));
      clickWith(await row("W004"), { ctrlKey: true });
      await until("W002+W004", () => checkedTitles() === "W002,W004");
      if (document.querySelector('[aria-label="Task-Details"]')) throw new Error("Detail noch offen");
      return checkedTitles();
    },
  ],
  [
    "Umschalt+Klick fügt Bereich hinzu",
    async () => {
      clickWith(await row("W006"), { shiftKey: true });
      await until("Bereich", () => checkedTitles() === "W002,W004,W005,W006");
      return `${selectionCount()} ausgewählt`;
    },
  ],
  [
    "Umschalt+↓ erweitert, X schaltet um",
    async () => {
      key(focused(), "Escape");
      await until("Modus aus", () => !multiselectable());
      const first = await row("W001");
      first.focus();
      first.click();
      key(focused(), "Escape"); // Detail zu, Cursor bleibt
      key(focused(), "ArrowDown", { shiftKey: true });
      key(focused(), "ArrowDown", { shiftKey: true });
      await until("W001–W003", () => checkedTitles() === "W001,W002,W003");
      key(focused(), "x");
      await until("W003 ab", () => checkedTitles() === "W001,W002");
      return checkedTitles();
    },
  ],
  [
    `Strg+A wählt alle ${SELECT_N} (nachgeladene Seiten)`,
    async () => {
      const t0 = performance.now();
      key(focused(), "a", { ctrlKey: true });
      await until(`${SELECT_N} ausgewählt`, () => selectionCount() === SELECT_N, 5000);
      const ms = performance.now() - t0;
      key(focused(), "a", { ctrlKey: true });
      await until("keine", () => selectionCount() === 0);
      key(focused(), "Escape");
      await until("Modus aus", () => !multiselectable());
      return `${ms.toFixed(0)} ms`;
    },
  ],  [
    "E erledigt die Auswahl (1 Aufruf, Zeilen verlassen die Ansicht), Rückgängig",
    async (client) => {
      const calls = countCalls(client, "completeTasks");
      try {
        await click("button", "Auswählen");
        for (const t of ["W001", "W002", "W003"]) (await row(t)).click();
        key(focused(), "e");
        // P1-01: abgehakte verlassen die offene Ansicht.
        await until("verschwunden", () => visibleTitles(2) === "W004,W005");
        (await until("Rückgängig", () => byRole("button", "Rückgängig"))).click();
        await until("wieder da", () => visibleTitles(5) === "W001,W002,W003,W004,W005");
        if (calls.calls !== 2) throw new Error(`${calls.calls} Aufrufe`);
        return "1 Aufruf + 1 Rückgängig";
      } finally {
        calls.restore();
      }
    },
  ],
  [
    "M verschiebt verstreute Auswahl, Rückgängig an alte Stellen",
    async (client) => {
      const calls = countCalls(client, "moveTasks");
      try {
        key(focused(), "a", { ctrlKey: true });
        await until("alle", () => selectionCount() === SELECT_N, 5000);
        key(focused(), "a", { ctrlKey: true });
        for (const t of ["W002", "W004"]) (await row(t)).click();
        key(focused(), "m");
        await click("menuitem", "Eingang");
        await until("weg", () => visibleTitles(3) === "W001,W003,W005");
        if (calls.calls !== 1) throw new Error(`${calls.calls} Aufrufe beim Verschieben`);
        (await until("Rückgängig", () => byRole("button", "Rückgängig"))).click();
        await until("zurück", () => visibleTitles(5) === "W001,W002,W003,W004,W005", 5000);
        return `Rückgängig mit ${calls.calls - 1} Aufrufen`;
      } finally {
        calls.restore();
      }
    },
  ],
  [
    `${SELECT_N} löschen: 1 Aufruf, sofort leer, Rückgängig an alter Stelle`,
    async (client) => {
      const deletes = countCalls(client, "deleteTasks");
      const gets = countCalls(client, "getTask");
      try {
        const before = await titlesOf(client, "Auswahl");
        key(focused(), "a", { ctrlKey: true });
        await until("alle", () => selectionCount() === SELECT_N, 5000);
        const t0 = performance.now();
        key(focused(), "Delete");
        await frame();
        const empty = document.querySelectorAll('[role="option"]').length === 0;
        const painted = performance.now() - t0;
        if (!empty) throw new Error("Liste nicht sofort leer");
        await until("Toast", () => byRole("button", "Rückgängig"));
        if (deletes.calls !== 1) throw new Error(`${deletes.calls} deleteTasks`);
        (await until("Rückgängig", () => byRole("button", "Rückgängig"))).click();
        await until("zurück", () => visibleTitles(3) === "W001,W002,W003", 8000);
        const after = await titlesOf(client, "Auswahl");
        if (after.length !== SELECT_N || after.join() !== before.join()) {
          const i = after.findIndex((t, j) => t !== before[j]);
          throw new Error(`${after.length} zurück, erste Abweichung an ${i}: ${before[i]} → ${after[i]}`);
        }
        key(focused(), "Escape");
        return `leer nach ${painted.toFixed(0)} ms, ${gets.calls} getTask`;
      } finally {
        deletes.restore();
        gets.restore();
      }
    },
  ],  [
    "Alt+↓ verschiebt den Auswahlblock (verstreut → zusammengezogen)",
    async (client) => {
      const moves = countCalls(client, "moveTasks");
      try {
        if (!multiselectable()) await click("button", "Auswählen");
        for (const t of ["W001", "W003"]) (await row(t)).click();
        key(focused(), "ArrowDown", { altKey: true });
        await until("Block", () => visibleTitles(5) === "W002,W004,W001,W003,W005");
        await untilAsync("gespeichert", async () => (await titlesOf(client, "Auswahl")).slice(0, 5).join() === "W002,W004,W001,W003,W005");
        if (moves.calls !== 1) throw new Error(`${moves.calls} moveTasks`);
        return visibleTitles(5);
      } finally {
        moves.restore();
      }
    },
  ],
  [
    "Ziehen am Griff zieht den Block (Badge „2“)",
    async (client) => {
      const grip = (await row("W001")).querySelector<HTMLElement>(".grip")!;
      const g = grip.getBoundingClientRect();
      const top = document.querySelector('[role="listbox"]')!.getBoundingClientRect().top;
      const at = (y: number) => ({ bubbles: true, cancelable: true, button: 0, clientX: g.left + 4, clientY: y, pointerId: 1 });
      grip.dispatchEvent(new PointerEvent("pointerdown", at(g.top + g.height / 2)));
      dispatchEvent(new PointerEvent("pointermove", at(top + 2)));
      await frame();
      await frame();
      const badge = document.querySelector(".drag-badge")?.textContent;
      dispatchEvent(new PointerEvent("pointerup", at(top + 2)));
      await until("abgelegt", () => visibleTitles(5) === "W001,W003,W002,W004,W005");
      await untilAsync("gespeichert", async () => (await titlesOf(client, "Auswahl")).slice(0, 5).join() === "W001,W003,W002,W004,W005");
      key(focused(), "Escape");
      if (badge !== "2") throw new Error(`Badge ${badge}`);
      return visibleTitles(5);
    },
  ],
];

// P0-07: Messung – Repaint nach Klick/Umschalt+↓ mit vielen ausgewählten, Leerlauf im Modus,
// „Alle auswählen“ mit 10 000 Tasks. `IDLE_START`/`IDLE_END` markieren das Leerlauf-Fenster für die
// CPU-Messung von außen.
async function createMany(client: CoreClient, name: string, n: number): Promise<string> {
  const list = await client.createTaskList(name);
  for (let i = 0; i < n; i += 100) {
    await Promise.all(
      Array.from({ length: Math.min(100, n - i) }, (_, j) => client.createTask({ title: `${name} ${i + j + 1}`, listId: list.id })),
    );
  }
  return list.id;
}

/** Zeit bis zum ersten Frame nach `act`, `runs` Mal; `act` liefert, worauf vor dem Frame gewartet wird. */
async function repaint(runs: number, act: (i: number) => Promise<void> | void): Promise<string> {
  const times: number[] = [];
  for (let i = 0; i < runs; i++) {
    await frame();
    const t0 = performance.now();
    await act(i);
    times.push((await frame()) - t0);
  }
  const s = stats(times);
  if (s.p95 > 50) throw new Error(`p95 ${s.p95.toFixed(1)} ms > 50 ms`);
  return `p50 ${s.p50.toFixed(1)} / p95 ${s.p95.toFixed(1)} / max ${s.max.toFixed(1)} ms`;
}

/** Klick auf eine sichtbare Zeile im Modus; wartet (ohne Polling), bis ihr `aria-selected` umspringt. */
async function toggleRow(i: number) {
  const rows = [...document.querySelectorAll<HTMLElement>('[role="option"]')];
  const target = rows[2 + (i % 8)];
  const changed = new Promise<void>((resolve) => {
    const obs = new MutationObserver(() => {
      obs.disconnect();
      resolve();
    });
    obs.observe(target, { attributes: true, attributeFilter: ["aria-selected"] });
  });
  clickWith(target);
  await changed;
}

/** Leerlauf-Fenster für die CPU-Messung von außen (`NAME_START`/`NAME_END`). */
async function idle(name: string, ms = 30_000) {
  mark(`${name}_START`);
  await sleep(ms);
  mark(`${name}_END`);
}

const p0perf: Step[] = [
  [
    "1 000 Tasks in „Messung“",
    async (client) => {
      const id = await createMany(client, "Messung", 1000);
      location.hash = `#/list/${id}`;
      await until("h1", () => heading() === "Messung");
      await until("Zeilen", () => document.querySelectorAll('[role="option"]').length > 0);
    },
  ],
  ["Leerlauf ohne Modus (30 s, Vergleich)", async () => idle("IDLE_BASE")],
  [
    "Strg+A: 1 000 ausgewählt",
    async () => {
      (document.querySelector<HTMLElement>('[role="option"]'))!.focus();
      const t0 = performance.now();
      key(focused(), "a", { ctrlKey: true });
      await until("1000", () => selectionCount() === 1000, 5000);
      return `${((await frame()) - t0).toFixed(0)} ms bis Frame`;
    },
  ],
  ["Klick schaltet um, bis Repaint (1 000 ausgewählt)", async () => repaint(20, toggleRow)],
  [
    "Umschalt+↓ bis Repaint (1 000 ausgewählt)",
    async () => {
      (document.querySelector<HTMLElement>('[role="option"]'))!.focus();
      return repaint(20, () => key(focused(), "ArrowDown", { shiftKey: true }));
    },
  ],
  [
    "Leerlauf im Auswahlmodus (30 s)",
    async () => {
      await idle("IDLE_MODE");
      if (!multiselectable()) throw new Error("Modus verlassen");
      key(focused(), "Escape");
    },
  ],
];

const p0perf10k: Step[] = [
  [
    "10 000 Tasks in „Zehntausend“",
    async (client) => {
      const t0 = performance.now();
      const id = await createMany(client, "Zehntausend", 10_000);
      location.hash = `#/list/${id}`;
      await until("h1", () => heading() === "Zehntausend", 10_000);
      await until("Zeilen", () => document.querySelectorAll('[role="option"]').length > 0);
      return `${((performance.now() - t0) / 1000).toFixed(0)} s angelegt`;
    },
  ],
  [
    "Alle auswählen mit 10 000 (lädt 9 800 nach)",
    async () => {
      (document.querySelector<HTMLElement>('[role="option"]'))!.focus();
      const t0 = performance.now();
      key(focused(), "a", { ctrlKey: true });
      await until("10000", () => selectionCount() === 10_000, 30_000);
      return `${((await frame()) - t0).toFixed(0)} ms bis Frame`;
    },
  ],
  ["Klick schaltet um, bis Repaint (10 000 ausgewählt)", async () => repaint(10, toggleRow)],
  [
    "Esc beendet den Modus",
    async () => {
      key(focused(), "Escape");
      await until("aus", () => !multiselectable());
    },
  ],
];

// P0-06/P0-07: Sammelaktionen über zwei Profile (tools/acceptance-two-devices.sh, Abschnitt 5).
const SAMMEL = "Sammel";
const ZIEL = "Ziel";

async function p0state(client: CoreClient): Promise<string> {
  const titles = async (name: string) => (await allIn(client, await listId(client, name))).map((t) => t.title);
  const [a, z] = [await titles(SAMMEL), await titles(ZIEL)];
  return `${SAMMEL} ${a.length}: ${a.join(" ")} · ${ZIEL} ${z.length}: ${z.join(" ")}`;
}

async function shiftDown(n: number) {
  for (let i = 0; i < n; i++) key(focused(), "ArrowDown", { shiftKey: true });
  await frame();
}

const p0bulkA: Step[] = [
  [
    `Listen „${SAMMEL}“ (200 Tasks) und „${ZIEL}“`,
    async (client) => {
      const list = await client.createTaskList(SAMMEL);
      await client.createTaskList(ZIEL);
      // Nacheinander: parallele Aufrufe vergäben gleiche Sort-Keys (FIX-10).
      for (let i = 1; i <= 200; i++) await client.createTask({ title: `S${String(i).padStart(3, "0")}`, listId: list.id });
      location.hash = `#/list/${list.id}`;
      await until(`h1 ${SAMMEL}`, () => heading() === SAMMEL);
      await row("S001");
    },
  ],
  [
    "50 per Umschalt+↓ auswählen und löschen",
    async () => {
      const first = await row("S001");
      first.focus();
      first.click();
      key(focused(), "Escape");
      await shiftDown(49);
      await until("50", () => selectionCount() === 50);
      key(focused(), "Delete");
      await until("S051 oben", () => visibleTitles(1) === "S051");
    },
  ],
  [
    `50 nach „${ZIEL}“ verschieben (M)`,
    async () => {
      const first = await row("S051");
      first.focus();
      first.click();
      await shiftDown(49);
      await until("50", () => selectionCount() === 50);
      key(focused(), "m");
      await click("menuitem", ZIEL);
      await until("S101 oben", () => visibleTitles(1) === "S101");
    },
  ],
  [
    "Block S101+S103 mit Alt+↓ umsortieren",
    async () => {
      for (const t of ["S101", "S103"]) (await row(t)).click();
      await until("2", () => selectionCount() === 2);
      key(focused(), "ArrowDown", { altKey: true });
      await until("Block", () => visibleTitles(5) === "S102,S104,S101,S103,S105");
      key(focused(), "Escape");
    },
  ],
  ["Hochgeladen", async (client) => syncedNow(client)],
  ["Zustand", async (client) => p0state(client)],
];

const p0bulkB: Step[] = [
  ["Synchronisiert", async (client) => syncedNow(client)],
  ["Zustand", async (client) => p0state(client)],
];

/** Neustart ohne Sync: Zustand aus der lokalen Datenbank. */
const p0bulkLocal: Step[] = [["Zustand", async (client) => p0state(client)]];


const SUITES: Record<string, Step[]> = { p002, p0sel, p0perf, p0perf10k, p0bulkA, p0bulkB, p0bulkLocal, base, p707, p708a, p708b, p709a, p709b, p710, p711, p711live,
  p802wait, p802create, p802offA, p802offB, p802sync, p802bulk, p802auth };

/** `VITE_TASKS_SELFTEST` = Suiten mit Komma (z. B. `base,p707`); `1` = base + p707. */
export async function runSelftest(
  client: CoreClient,
  selection: string,
  report?: (line: string, exit?: boolean) => Promise<void>,
): Promise<void> {
  if (report) mark = (line) => void report(line);
  const names = selection === "1" ? ["base", "p707"] : selection.split(",").map((n) => n.trim());
  const steps = names.flatMap((n) => SUITES[n] ?? [[`Suite ${n}`, async () => Promise.reject(new Error("unbekannt"))] as Step]);
  const box = document.createElement("pre");
  box.style.cssText =
    "position:fixed;top:8px;right:8px;z-index:9999;margin:0;padding:8px 12px;max-width:520px;font:12px/1.5 monospace;background:#000c;color:#fff;border-radius:6px;white-space:pre-wrap";
  document.body.append(box);
  const lines: string[] = [];
  let failed = 0;
  for (const [name, run] of steps) {
    try {
      const detail = await run(client);
      lines.push(`PASS ${name}${detail ? ` – ${detail}` : ""}`);
    } catch (e) {
      failed++;
      lines.push(`FAIL ${name}: ${(e as Error).message}`);
    }
    await report?.(lines.at(-1)!);
    box.textContent = lines.join("\n");
  }
  lines.push(failed ? `${failed} FEHLGESCHLAGEN` : "ALLE BESTANDEN");
  box.textContent = lines.join("\n");
  box.style.background = failed ? "#a00e" : "#060e";
  await report?.(lines.at(-1)!, true);
}
