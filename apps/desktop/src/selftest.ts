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
  ],  [
    "Navigation scrollt nicht horizontal",
    async () => {
      const nav = await until("Navigationsliste", () => document.querySelector<HTMLElement>("aside .scroll"));
      const style = getComputedStyle(nav);
      const extra = nav.scrollWidth - nav.clientWidth;
      if (extra > 0 || style.overflowX !== "hidden") {
        throw new Error(`scrollWidth ${nav.scrollWidth} > clientWidth ${nav.clientWidth}, overflow-x ${style.overflowX}`);
      }
      return `Breite ${nav.clientWidth} px, overflow-x ${style.overflowX}`;
    },
  ],  [
    "Eingeklappte Navigation: Tooltip vollständig sichtbar",
    async () => {
      await click("button", "Seitenleiste einklappen");
      const item = await until("Eingang (eingeklappt)", () => document.querySelector<HTMLElement>('aside a[aria-label="Eingang"]'));
      item.dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
      await sleep(600);
      try {
        const tip = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find((t) => t.textContent?.trim() === "Eingang");
        if (!tip) throw new Error("kein Tooltip-Element (CSS-::after wird vom Seitenleisten-Bereich abgeschnitten)");
        const r = tip.getBoundingClientRect();
        const aside = document.querySelector("aside")!.getBoundingClientRect();
        const style = getComputedStyle(tip);
        if (r.left < aside.right - 1) throw new Error(`Tooltip beginnt bei ${r.left}, Seitenleiste endet bei ${aside.right}`);
        if (r.right > innerWidth || r.bottom > innerHeight || r.width === 0) throw new Error("außerhalb des Fensters");
        if (style.visibility === "hidden" || Number(style.opacity) < 1) throw new Error(`nicht sichtbar (${style.opacity})`);
        return `bei x ${Math.round(r.left)} (Seitenleiste bis ${Math.round(aside.right)})`;
      } finally {
        item.dispatchEvent(new PointerEvent("pointerout", { bubbles: true }));
        await click("button", "Seitenleiste ausklappen");
      }
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

/** „+ Neue Task“ in der Seitenleiste öffnet das Fenster „Neue Task“; liefert dessen Eingabefeld. */
async function openNewTask(): Promise<HTMLInputElement> {
  (await until("Neue Task", () => document.querySelector<HTMLElement>('aside nav button[title^="Neue Task"], aside nav button[aria-label="Neue Task"]'))).click();
  const input = await until("Fenster Neue Task", () =>
    document.querySelector<HTMLInputElement>('dialog[open] input[aria-label="Neue Task"]'),
  );
  await until("Fokus", () => document.activeElement === input);
  return input;
}

/** Fenster ist nach dem Anlegen zu. */
const newTaskClosed = () => until("Fenster zu", () => !document.querySelector('dialog[open] input[aria-label="Neue Task"]'));

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
    "Neue Task mit „morgen“ (Fenster schließt nach Enter)",
    async () => {
      const input = await openNewTask();
      type(input, "Persistenz prüfen morgen");
      key(input, "Enter");
      await until("neue Zeile", () => byRole("option", /^Persistenz prüfen, Fällig/));
      await newTaskClosed();
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
      // P2-04: Listen-Picker statt <select>.
      (await until("Listenauswahl", () => document.querySelector<HTMLElement>("#detail-list"))).click();
      await click("menuitem", "Projekt");
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
    "N öffnet „Neue Task“, Enter legt an und schließt (zweimal)",
    async () => {
      await click("link", /^Eingang/);
      for (const title of ["Tastatur A", "Tastatur B"]) {
        press("n");
        const input = await until("Fokus Neue Task", () => document.activeElement?.getAttribute("aria-label") === "Neue Task" && (document.activeElement as HTMLInputElement));
        type(input, title);
        press("Enter");
        await until(title, () => byRole("option", title));
        await newTaskClosed();
      }
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
      const input = await openNewTask();
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
      const input = await openNewTask();
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


// P1-03: automatischer Upload nach lokalen Änderungen (ADR-022) – ohne `sync.now`.

/** Wartet auf einen erfolgreichen Lauf nach `since` (Date.now) mit leerer Outbox; liefert ms seit `since`. */
async function uploadedSince(client: CoreClient, since: number, timeoutMs = 10_000): Promise<number> {
  const end = performance.now() + timeoutMs;
  for (;;) {
    const st = await client.sync.status();
    if (st.state === "idle" && st.pendingOps === 0 && st.lastSuccess && Date.parse(st.lastSuccess) >= since) {
      return Date.now() - since;
    }
    if (performance.now() > end) throw new Error(`timeout: ${st.state}, ${st.pendingOps} ausstehend`);
    await sleep(50);
  }
}

/** Abhaken über das runde Kästchen in der Zeile; wartet, bis die Zeile die Ansicht verlassen hat (P1-01). */
async function completeInView(title: string) {
  (await until(`Haken ${title}`, () => byRole("checkbox", `Erledigt: ${title}`))).click();
  await until(`${title} verschwunden`, () => !byRole("option", title) && !byRole("option", `${title}, erledigt`), 3000);
}

const P1_TASK = "Abhaken-Test";

async function p1state(client: CoreClient): Promise<string> {
  const done = (await client.listTasks({ view: { kind: "completed" } }, { limit: 500 })).items;
  const open = (await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 500 })).items;
  return `${P1_TASK} erledigt=${done.some((t) => t.title === P1_TASK)} im Eingang=${open.some((t) => t.title === P1_TASK)}`;
}

const p1sync: Step[] = [
  ["Bereit: synchronisiert", async (client) => syncedNow(client)],
  [
    "Eine Änderung wird ohne Zutun hochgeladen",
    async (client) => {
      const t0 = Date.now();
      await client.createTask({ title: "P1 einzeln" });
      const ms = await uploadedSince(client, t0);
      if (ms > 4000) throw new Error(`${ms} ms`);
      return `${ms} ms (Debounce 1,5 s + Lauf)`;
    },
  ],
  [
    "Serie: 8 Änderungen im Sekundentakt → erster Upload ≤ 5,5 s",
    async (client) => {
      const runs: number[] = [];
      let last = "";
      const stop = client.onSyncStatus((st) => {
        if (st.state === "syncing" && last !== "syncing") runs.push(Date.now());
        last = st.state;
      });
      try {
        const t0 = Date.now();
        const task = await client.createTask({ title: "P1 Serie" });
        for (let i = 1; i < 8; i++) {
          await sleep(1000);
          if (i % 3 === 1) await client.updateTask(task.id, { title: `P1 Serie ${i}` });
          else if (i % 3 === 2) await client.completeTask(task.id, i % 2 === 0);
          else await client.createTask({ title: `P1 Serie neu ${i}` });
        }
        await uploadedSince(client, Date.now() - 100);
        const first = runs.find((r) => r >= t0);
        if (first === undefined) throw new Error("kein Lauf");
        const after = first - t0;
        if (after > 5500) throw new Error(`erster Lauf erst nach ${after} ms`);
        return `erster Lauf nach ${after} ms, ${runs.filter((r) => r >= t0).length} Läufe für 8 Änderungen in ${((Date.now() - t0) / 1000).toFixed(1)} s`;
      } finally {
        stop();
      }
    },
  ],
];

/** Abnahme Abschnitt 6, Gerät A: im Eingang abhaken → Zeile weg, ohne manuellen Sync oben. */
const p1A: Step[] = [
  ["Bereit: synchronisiert", async (client) => syncedNow(client)],
  [
    `„${P1_TASK}“ anlegen (Quick-Add)`,
    async () => {
      location.hash = "#/inbox";
      await until("h1 Eingang", () => heading() === "Eingang");
      const input = await openNewTask();
      type(input, P1_TASK);
      key(input, "Enter");
      await row(P1_TASK);
    },
  ],
  [
    "Im Eingang abhaken: Zeile verschwindet, Upload ohne Zutun",
    async (client) => {
      const t0 = Date.now();
      await completeInView(P1_TASK);
      const ms = await uploadedSince(client, t0);
      if (ms > 6000) throw new Error(`${ms} ms`);
      return `oben nach ${ms} ms`;
    },
  ],
  ["Zustand", async (client) => p1state(client)],
];

const p1B: Step[] = [
  ["Synchronisiert", async (client) => syncedNow(client)],
  ["Zustand", async (client) => p1state(client)],
];

// P2: Listen & Kanban-Board in der echten WebView.
const boardColumn = (name: string) =>
  document.querySelector<HTMLElement>(`[aria-label="Kanban-Board"] section[aria-label="${CSS.escape(name)}"]`);
const boardCards = (name: string) =>
  [...(boardColumn(name)?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])].map((o) => o.getAttribute("aria-label")).join(",");

const p2board: Step[] = [
  [
    "Listen „Einkaufen“ und „Arbeit“ mit Tasks",
    async (client) => {
      const shop = await client.createTaskList("Einkaufen");
      const work = await client.createTaskList("Arbeit");
      for (const t of ["Milch", "Brot", "Käse"]) await client.createTask({ title: t, listId: shop.id });
      await client.createTask({ title: "Bericht", listId: work.id });
      await client.createTask({ title: "Im Eingang" });
    },
  ],
  [
    "Liste anlegen, inline umbenennen, per Dialog löschen (Tasks → Eingang)",
    async (client) => {
      await click("button", "Neue Liste");
      const input = await field("Name der neuen Liste");
      type(input, "Temp");
      key(input, "Enter");
      await until("h1 Temp", () => heading() === "Temp");
      const temp = (await client.listTaskLists()).find((l) => l.name === "Temp")!;
      await client.createTask({ title: "Aus Temp", listId: temp.id });
      const link = await until("Temp-Link", () => document.querySelector<HTMLElement>(`aside a[data-list-id="${temp.id}"]`));
      link.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 300 }));
      await click("menuitem", /^Umbenennen/);
      const rename = await field("Neuer Name der Liste");
      type(rename, "Temporär");
      key(rename, "Enter");
      await until("umbenannt", () => heading() === "Temporär");
      await click("button", "Listenaktionen");
      await click("menuitem", /^Löschen/);
      await until("Dialog", () => document.querySelector("dialog[open]")?.textContent?.includes("1 Task offene"));
      await click("button", "Liste löschen");
      await until("Eingang", () => heading() === "Eingang");
      await row("Aus Temp");
      const names = (await client.listTaskLists()).map((l) => l.name).join(",");
      if (names.includes("Temp")) throw new Error(names);
      return names;
    },
  ],
  [
    "Quick-Add: Liste „Einkaufen“ wählen (Alt+L) und anlegen",
    async (client) => {
      const input = await openNewTask();
      key(input, "l", { altKey: true });
      await click("menuitem", "Einkaufen");
      type(input, "Eier");
      key(input, "Enter");
      const toastTexts = () => [...document.querySelectorAll("[role=status]")].map((t) => t.textContent?.trim()).join(" | ");
      await until("Toast", () => document.body.textContent?.includes("„Eier“ in „Einkaufen“ angelegt")).catch(async () => {
        const all = (await client.listTasks({ view: { kind: "all" } }, { limit: 100 })).items.map((t) => `${t.title}@${t.listId.slice(-4)}`);
        throw new Error(
          `Toasts: ${toastTexts()} · Picker ${document.querySelector('[aria-label^="Liste: "]')?.getAttribute("aria-label")} · h1 ${heading()} · input "${input.value}" · ${all.join(",")}`,
        );
      });
      const shop = (await client.listTaskLists()).find((l) => l.name === "Einkaufen")!;
      const titles = (await client.listTasks({ view: { kind: "list", listId: shop.id } }, { limit: 10 })).items.map((t) => t.title);
      if (!titles.includes("Eier")) throw new Error(titles.join(","));
      await client.deleteTasks([(await client.listTasks({ view: { kind: "list", listId: shop.id } }, { limit: 10 })).items.find((t) => t.title === "Eier")!.id]);
      return titles.join(",");
    },
  ],
  [
    "Tooltip im Fenster „Neue Task“ liegt über dem Dialog",
    async () => {
      const input = await openNewTask();
      const pickerButton = await until("Listenwahl", () => document.querySelector<HTMLElement>('dialog[open] button[aria-label^="Liste: "]'));
      pickerButton.dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
      await sleep(600);
      const tip = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find((t) => t.textContent?.includes("Liste wählen"));
      const onTop = tip?.matches(":popover-open") ?? false;
      pickerButton.dispatchEvent(new PointerEvent("pointerout", { bubbles: true }));
      key(input, "Escape");
      await newTaskClosed();
      if (!tip || !onTop) throw new Error(tip ? "Tooltip nicht in der obersten Ebene" : "kein Tooltip");
      return "als Popover über dem modalen Dialog";
    },
  ],
  [
    "Umschalten auf Kanban: Spalten nebeneinander, Karten mit fester Höhe",
    async () => {
      location.hash = "#/inbox";
      await until("h1 Eingang", () => heading() === "Eingang");
      await click("button", "Kanban");
      await until("Board", () => boardCards("Einkaufen") === "Milch,Brot,Käse");
      const cols = [...document.querySelectorAll<HTMLElement>('[aria-label="Kanban-Board"] > section')];
      const names = cols.map((c) => c.getAttribute("aria-label")).join(",");
      if (names !== "Eingang,Einkaufen,Arbeit") throw new Error(`Spalten ${names}`);
      const [a, b] = cols.map((c) => c.getBoundingClientRect());
      if (!(b.left >= a.right && Math.abs(a.top - b.top) < 1)) throw new Error("Spalten nicht nebeneinander");
      const boardRect = document.querySelector('[aria-label="Kanban-Board"]')!.getBoundingClientRect();
      if (a.bottom > boardRect.bottom + 1 || a.height < 200) throw new Error(`Spaltenhöhe ${a.height}`);
      const card = boardColumn("Einkaufen")!.querySelector<HTMLElement>('[role="option"]')!.getBoundingClientRect();
      if (Math.round(card.height) !== 56) throw new Error(`Kartenhöhe ${card.height}`);
      return `${cols.length} Spalten à ${Math.round(a.width)} px, Karte ${Math.round(card.height)} px`;
    },
  ],
  [
    "Tastatur: Cursor in der Eingang-Spalte, → zur Nachbarspalte, Leertaste hakt ab",
    async () => {
      (await until("Karte", () => boardColumn("Eingang")?.querySelector<HTMLElement>('[aria-label="Im Eingang"]'))).click();
      key(focused(), "Escape"); // Detail zu, Cursor bleibt
      await until("Cursor", () => document.activeElement?.getAttribute("aria-label") === "Im Eingang");
      key(focused(), "ArrowRight");
      await until("Milch", () => document.activeElement?.getAttribute("aria-label") === "Milch");
      key(focused(), " ");
      await until("Milch weg", () => boardCards("Einkaufen") === "Brot,Käse");
      return `Cursor jetzt ${document.activeElement?.getAttribute("aria-label")}`;
    },
  ],  [
    "Karte „Käse“ in Spalte „Arbeit“ ziehen → Task gehört wirklich zu „Arbeit“ (1 Aufruf)",
    async (client) => {
      const calls = countCalls(client, "moveTasks");
      try {
        const card = await until("Käse", () => boardColumn("Einkaufen")?.querySelector<HTMLElement>('[aria-label="Käse"]'));
        const from = card.getBoundingClientRect();
        const target = boardColumn("Arbeit")!.querySelector<HTMLElement>('[role="listbox"]')!.getBoundingClientRect();
        const start = { clientX: from.left + 40, clientY: from.top + 20 };
        const drop = { clientX: target.left + 100, clientY: target.top + 64 + 20 }; // hinter „Bericht“
        const ev = (type: string, at: { clientX: number; clientY: number }) =>
          new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId: 1, ...at });
        card.dispatchEvent(ev("pointerdown", start));
        dispatchEvent(ev("pointermove", { clientX: start.clientX + 10, clientY: start.clientY + 10 }));
        dispatchEvent(ev("pointermove", drop));
        await frame();
        await frame();
        const ghost = !!document.querySelector(".ghost");
        dispatchEvent(ev("pointerup", drop));
        await until("in Arbeit", () => boardCards("Arbeit") === "Bericht,Käse");
        const lists = await client.listTaskLists();
        const work = lists.find((l) => l.name === "Arbeit")!;
        const kaese = (await client.listTasks({ view: { kind: "list", listId: work.id } }, { limit: 10 })).items.find(
          (t) => t.title === "Käse",
        );
        if (!kaese) throw new Error("Core: Käse nicht in „Arbeit“");
        if (calls.calls !== 1) throw new Error(`${calls.calls} moveTasks`);
        if (!ghost) throw new Error("keine Vorschau am Zeiger");
        return `Einkaufen ${boardCards("Einkaufen")} · Arbeit ${boardCards("Arbeit")}`;
      } finally {
        calls.restore();
      }
    },
  ],
  [
    "Alt+← schiebt die Cursor-Karte zurück",
    async () => {
      const card = boardColumn("Arbeit")!.querySelector<HTMLElement>('[aria-label="Käse"]')!;
      card.click();
      key(focused(), "Escape");
      key(focused(), "ArrowLeft", { altKey: true });
      await until("zurück", () => boardCards("Einkaufen").includes("Käse"));
      return boardCards("Einkaufen");
    },
  ],  [
    "Spalte „Arbeit“ am Kopf vor „Einkaufen“ ziehen → Board, Seitenleiste und Core gleich",
    async (client) => {
      const header = boardColumn("Arbeit")!.querySelector<HTMLElement>("[data-column-header]")!;
      const h = header.getBoundingClientRect();
      const target = boardColumn("Einkaufen")!.getBoundingClientRect();
      const ev = (type: string, x: number) =>
        new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId: 1, clientX: x, clientY: h.top + 10 });
      header.dispatchEvent(ev("pointerdown", h.left + 60));
      dispatchEvent(ev("pointermove", target.left + 20));
      await frame();
      dispatchEvent(ev("pointerup", target.left + 20));
      const names = () =>
        [...document.querySelectorAll<HTMLElement>('[aria-label="Kanban-Board"] > section')]
          .map((c) => c.getAttribute("aria-label"))
          .join(",");
      await until("Board", () => names() === "Eingang,Arbeit,Einkaufen");
      const side = [...document.querySelectorAll<HTMLElement>("aside a[data-list-id]")].map((a) => a.textContent?.trim()).join(",");
      const core = (await client.listTaskLists()).map((l) => l.name).join(",");
      if (side !== "Arbeit,Einkaufen" || core !== "Inbox,Arbeit,Einkaufen") throw new Error(`Seitenleiste ${side}, Core ${core}`);
      return names();
    },
  ],
];

// P2-09: Messung im Board (Debug-Build, wie P0-07).
const p2perf: Step[] = [
  [
    "10 Listen à 200 Tasks + Liste „Lang“ mit 1 000",
    async (client) => {
      const t0 = performance.now();
      for (let l = 1; l <= 10; l++) await createMany(client, `L${String(l).padStart(2, "0")}`, 200);
      await createMany(client, "Lang", 1000);
      return `${((performance.now() - t0) / 1000).toFixed(0)} s angelegt`;
    },
  ],
  ["Leerlauf Listenansicht (30 s, Vergleich)", async () => idle("IDLE_LIST")],
  [
    "Umschalten auf Kanban bis alle Spalten gezeichnet",
    async () => {
      const t0 = performance.now();
      await click("button", "Kanban");
      await until(
        "12 Spalten mit Karten",
        () => [...document.querySelectorAll('[aria-label="Kanban-Board"] > section')].filter((c) => c.querySelector('[role="option"]')).length >= 11,
        10_000,
      );
      const ms = (await frame()) - t0;
      return `${ms.toFixed(0)} ms (12 Spalten, je erste Seite 200)`;
    },
  ],
  [
    "Umschalt + Mausrad scrollt das Board waagerecht",
    async () => {
      const board = document.querySelector<HTMLElement>('[aria-label="Kanban-Board"]')!;
      board.scrollLeft = 0;
      const card = board.querySelector<HTMLElement>('[role="option"]')!;
      const e = new WheelEvent("wheel", { bubbles: true, cancelable: true, shiftKey: true, deltaY: 300 });
      card.dispatchEvent(e);
      await frame();
      if (board.scrollLeft <= 0) throw new Error(`scrollLeft ${board.scrollLeft} (Breite ${board.scrollWidth}/${board.clientWidth})`);
      const after = board.scrollLeft;
      board.scrollLeft = 0;
      return `scrollLeft 0 → ${after}`;
    },
  ],
  [
    "Karte per Alt+→ in die Nachbarspalte bis Repaint",
    async () => {
      const times: number[] = [];
      for (let i = 0; i < 10; i++) {
        const card = boardColumn("L01")!.querySelector<HTMLElement>('[role="option"]')!;
        card.click();
        key(focused(), "Escape");
        await frame();
        const target = boardColumn("L02")!.querySelector('[role="listbox"]')!;
        const changed = new Promise<void>((resolve) => {
          const obs = new MutationObserver(() => {
            obs.disconnect();
            resolve();
          });
          obs.observe(target, { childList: true, subtree: true });
        });
        const t0 = performance.now();
        key(focused(), "ArrowRight", { altKey: true });
        await changed;
        times.push((await frame()) - t0);
        await sleep(150);
      }
      const st = stats(times);
      if (st.p95 > 50) throw new Error(`p95 ${st.p95.toFixed(1)} ms`);
      return `p50 ${st.p50.toFixed(1)} / p95 ${st.p95.toFixed(1)} / max ${st.max.toFixed(1)} ms`;
    },
  ],
  [
    "Spalte „Lang“ (1 000 Karten) durchscrollen",
    async () => {
      const viewport = boardColumn("Lang")!.querySelector<HTMLElement>('[role="listbox"]')!;
      viewport.closest("section")!.scrollIntoView({ inline: "nearest" });
      // Alle Seiten laden (Zähler ohne „+“), damit wirklich 1 000 Karten gescrollt werden.
      const more = () => boardColumn("Lang")!.querySelector("header")!.textContent!.includes("+");
      for (let i = 0; i < 50 && more(); i++) {
        viewport.scrollTop = viewport.scrollHeight;
        await sleep(100);
      }
      viewport.scrollTop = 0;
      await frame();
      const deltas: number[] = [];
      let prev = await frame();
      while (viewport.scrollTop + viewport.clientHeight < viewport.scrollHeight - 1) {
        viewport.scrollTop += 96;
        const now = await frame();
        deltas.push(now - prev);
        prev = now;
      }
      const st = stats(deltas.slice(1));
      const dom = viewport.querySelectorAll('[role="option"]').length;
      if (st.p95 > 25) throw new Error(`p95 ${st.p95.toFixed(1)} ms`);
      return `${deltas.length} Frames, p50 ${st.p50.toFixed(1)} / p95 ${st.p95.toFixed(1)} ms, ${dom} DOM-Karten`;
    },
  ],
  ["Leerlauf Board (30 s)", async () => idle("IDLE_BOARD")],
  // Nachlauf: Die CPU-Messung von außen liest ihre zweite Stichprobe, bevor die App endet.
  ["Nachlauf", async () => void (await sleep(3000))],
];

// P2-09 Abnahme Abschnitt 7: Board-Ziehen, Umbenennen und Umsortieren über zwei Profile.
async function dragCardTo(title: string, columnName: string, slot: number) {
  const card = await until(title, () => document.querySelector<HTMLElement>(`[aria-label="Kanban-Board"] [role="option"][aria-label="${CSS.escape(title)}"]`));
  const from = card.getBoundingClientRect();
  const target = boardColumn(columnName)!.querySelector<HTMLElement>('[role="listbox"]')!.getBoundingClientRect();
  const start = { clientX: from.left + 40, clientY: from.top + 20 };
  const drop = { clientX: target.left + 100, clientY: target.top + slot * 64 + 20 };
  const ev = (type: string, at: { clientX: number; clientY: number }) =>
    new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId: 1, ...at });
  card.dispatchEvent(ev("pointerdown", start));
  dispatchEvent(ev("pointermove", { clientX: start.clientX + 10, clientY: start.clientY + 10 }));
  dispatchEvent(ev("pointermove", drop));
  await frame();
  await frame();
  dispatchEvent(ev("pointerup", drop));
}

async function p2state(client: CoreClient): Promise<string> {
  const lists = await client.listTaskLists();
  const apple = (await client.listTasks({ view: { kind: "all" } }, { limit: 500 })).items.find((t) => t.title === "Äpfel");
  const where = lists.find((l) => l.id === apple?.listId)?.name ?? "fehlt";
  const names = lists.filter((l) => ["Obst", "Haushalt", "Zweite"].includes(l.name)).map((l) => l.name);
  return `Listen ${names.join(" | ")} · Äpfel in ${where}`;
}

const p2syncA: Step[] = [
  ["Bereit: synchronisiert", async (client) => syncedNow(client)],
  [
    "Listen „Obst“, „Zweite“ und Task „Äpfel“ im Eingang",
    async (client) => {
      await client.createTaskList("Obst");
      await client.createTaskList("Zweite");
      await client.createTask({ title: "Äpfel" });
      location.hash = "#/inbox";
      await until("h1 Eingang", () => heading() === "Eingang");
    },
  ],
  [
    "Board: „Äpfel“ in die Spalte „Obst“ ziehen",
    async () => {
      const board = document.querySelector<HTMLElement>('[role="group"][aria-label="Darstellung"] [aria-label="Kanban"]')!;
      if (board.getAttribute("aria-pressed") === "false") board.click();
      await until("Spalte Obst", () => boardColumn("Obst"));
      boardColumn("Obst")!.scrollIntoView({ inline: "nearest" });
      await frame();
      await dragCardTo("Äpfel", "Obst", 0);
      await until("in Obst", () => boardCards("Obst") === "Äpfel", 5000);
    },
  ],
  [
    "Seitenleiste: „Zweite“ in „Haushalt“ umbenennen und per Alt+↑ vor „Obst“",
    async (client) => {
      const zweite = (await client.listTaskLists()).find((l) => l.name === "Zweite")!;
      const link = await until("Zweite", () => document.querySelector<HTMLElement>(`aside a[data-list-id="${zweite.id}"]`));
      link.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 300 }));
      await click("menuitem", /^Umbenennen/);
      const input = await field("Neuer Name der Liste");
      type(input, "Haushalt");
      key(input, "Enter");
      const renamed = await until("Haushalt", () => {
        const a = document.querySelector<HTMLElement>(`aside a[data-list-id="${zweite.id}"]`);
        return a?.textContent?.trim() === "Haushalt" && a;
      });
      renamed.focus();
      key(renamed, "ArrowUp", { altKey: true });
      await untilAsync("vor Obst", async () => (await p2state(client)).startsWith("Listen Haushalt | Obst"));
    },
  ],
  ["Hochgeladen", async (client) => syncedNow(client)],
  ["Zustand", async (client) => p2state(client)],
];

const p2syncB: Step[] = [
  ["Synchronisiert", async (client) => syncedNow(client)],
  ["Zustand", async (client) => p2state(client)],
];

const p2syncLocal: Step[] = [["Zustand", async (client) => p2state(client)]];

/** Nach Neustart mit demselben Profil: Darstellung „Kanban“ ist gemerkt. */
const p2layout: Step[] = [
  [
    "Start in der gemerkten Darstellung „Kanban“",
    async () => {
      await until("Board", () => document.querySelector('[aria-label="Kanban-Board"] section'), 5000);
      const pressed = document.querySelector('[role="group"][aria-label="Darstellung"] [aria-pressed="true"]');
      if (pressed?.getAttribute("aria-label") !== "Kanban") throw new Error("nicht Kanban");
    },
  ],
];

// P3-03: Fenster „Neue Task“ mit erkannten Angaben und Notiz – ein `createTask` über die echte IPC.
const p3new: Step[] = [
  [
    "Neue Task mit Uhrzeit, Priorität und Notiz",
    async (client) => {
      const input = await openNewTask();
      type(input, "Sprechstunde morgen um 14:00 !1");
      await until("Titelvorschau", () => document.querySelector("dialog[open] .hint strong")?.textContent === "Sprechstunde");
      await click("button", "Priorität: Hoch");
      await click("button", /Notiz/);
      const notes = await field("Notiz");
      type(notes, "Fragen zu Blatt 3");
      key(notes, "Enter", { ctrlKey: true });
      await newTaskClosed();
      await until("neue Zeile", () => byRole("option", /^Sprechstunde/));
      const all = await client.listTasks({ view: { kind: "all" } }, { limit: 50 });
      const summary = all.items.find((t) => t.title === "Sprechstunde");
      if (!summary) throw new Error("nicht angelegt");
      const task = await client.getTask(summary.id);
      if (task.priority !== "high" || task.notes !== "Fragen zu Blatt 3" || task.due?.kind !== "dateTime") {
        throw new Error(JSON.stringify({ priority: task.priority, notes: task.notes, due: task.due }));
      }
      return `fällig ${new Date(task.due.at).toLocaleString("de-DE")} (${task.due.tz})`;
    },
  ],
];

// P4-01: Uhrzeit in der Detailansicht – Datum ändern behält sie, auch nach erneutem Laden aus dem Core.
const p4due: Step[] = [
  [
    "Uhrzeit setzen, Datum ändern, Uhrzeit bleibt",
    async (client) => {
      location.hash = "#/inbox";
      await until("Eingang", () => heading() === "Eingang");
      const input = await openNewTask();
      type(input, "Termin morgen um 14:00");
      key(input, "Enter");
      await newTaskClosed();
      await openTask(/^Termin/);
      const date = await until("Datumsfeld", () => document.querySelector<HTMLInputElement>("#detail-due"));
      date.value = "2030-01-15";
      date.dispatchEvent(new Event("change", { bubbles: true }));
      const all = await client.listTasks({ view: { kind: "all" } }, { limit: 50 });
      const id = all.items.find((t) => t.title === "Termin")?.id;
      if (!id) throw new Error("nicht angelegt");
      let due = (await client.getTask(id)).due;
      await untilAsync("gespeichert", async () => {
        due = (await client.getTask(id)).due;
        return due?.kind === "dateTime" && new Date(due.at).getFullYear() === 2030;
      });
      const at = due?.kind === "dateTime" ? new Date(due.at) : null;
      if (!at || at.getHours() !== 14 || at.getMinutes() !== 0 || at.getDate() !== 15) throw new Error(JSON.stringify(due));
      // Detail schließen und neu öffnen: Felder kommen aus dem Core.
      await click("button", "Schließen");
      location.hash = "#/all";
      await openTask(/^Termin/);
      const time = await field("Uhrzeit");
      await until("Uhrzeit im Feld", () => time.value === "14:00");
      return `${at.toLocaleString("de-DE")}`;
    },
  ],
];

// P4-02: „Heute“ mit Abschnitt „Überfällig“; „Alle auf heute“ ist ein `set_dues` über die echte IPC.
const p4today: Step[] = [
  [
    "Drei überfällige → „Alle auf heute“ → Abschnitt weg",
    async (client) => {
      const pad = (n: number) => String(n).padStart(2, "0");
      const ago = (days: number) => {
        const d = new Date();
        d.setDate(d.getDate() - days);
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      };
      for (const [title, days] of [["Ü1", 1], ["Ü2", 3], ["Ü3", 9]] as const) {
        await client.createTask({ title, due: { kind: "date", date: ago(days) } });
      }
      location.hash = "#/today";
      await until("Abschnitt Überfällig", () => [...document.querySelectorAll(".heading .label")].some((e) => e.textContent === "Überfällig"));
      const order = () =>
        [...document.querySelectorAll('[role="option"] .title')]
          .map((e) => e.textContent)
          .filter((t) => t?.startsWith("Ü"))
          .join(",");
      await until("drei Überfällige", () => order().split(",").length === 3).catch(() => undefined);
      if (order() !== "Ü3,Ü2,Ü1") throw new Error(`Reihenfolge ${order()}`);
      await click("button", "Alle auf heute");
      await until("Abschnitt weg", () => !document.querySelector(".heading"), 5000);
      const all = await client.listTasks({ view: { kind: "today", day: { date: ago(0), start: new Date(new Date().setHours(0, 0, 0, 0)).toISOString(), end: new Date(new Date().setHours(24, 0, 0, 0)).toISOString() } } }, { limit: 50 });
      const moved = all.items.filter((t) => t.title.startsWith("Ü"));
      if (moved.length !== 3 || moved.some((t) => t.due?.kind !== "date" || t.due.date !== ago(0))) throw new Error(JSON.stringify(moved.map((t) => t.due)));
      return "3 Tasks auf heute";
    },
  ],
];

// P4-03: „Geplant“ nach Tagen gruppiert – 1 000 Tasks über ~14 Monate, Scrollen und Tastatur im Budget.
const p4upcoming: Step[] = [
  [
    "1 000 geplante Tasks anlegen (IPC)",
    async (client) => {
      const pad = (n: number) => String(n).padStart(2, "0");
      const ahead = (days: number) => {
        const d = new Date();
        d.setDate(d.getDate() + days);
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      };
      for (let i = 0; i < 1000; i += 100) {
        await Promise.all(
          Array.from({ length: 100 }, (_, j) => {
            const n = i + j;
            return client.createTask({ title: `Geplant ${String(n + 1).padStart(4, "0")}`, due: { kind: "date", date: ahead(1 + ((n * 37) % 420)) } });
          }),
        );
      }
      location.hash = "#/upcoming";
      await until("Überschrift Morgen", () => document.querySelector(".heading .label")?.textContent === "Morgen", 8000);
    },
  ],
  [
    "Scrollen durch alle Abschnitte",
    async () => {
      const viewport = document.querySelector<HTMLElement>('[role="listbox"]')!;
      const deltas: number[] = [];
      const labels = new Set<string>();
      let order: string[] = [];
      let prev = await frame();
      for (let guard = 0; guard < 5000; guard++) {
        for (const h of viewport.querySelectorAll(".heading .label")) {
          const label = h.textContent ?? "";
          if (order.at(-1) !== label) order.push(label);
          labels.add(label);
        }
        if (viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - 1) {
          // Ende erreicht – noch eine Seite nachladen lassen?
          await sleep(1500);
          if (viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - 1) break;
        }
        viewport.scrollTop += 96;
        const now = await frame();
        deltas.push(now - prev);
        prev = now;
      }
      order = order.filter((l, i) => order.indexOf(l) === i);
      if (order.length !== labels.size) throw new Error(`Abschnitt doppelt: ${[...labels].join(", ")}`);
      const rows = document.querySelectorAll('[role="option"]').length;
      if (rows > 100) throw new Error(`${rows} DOM-Zeilen`);
      viewport.scrollTop = 0;
      const s = stats(deltas.slice(1));
      if (s.p95 > 25) throw new Error(`p95 ${s.p95.toFixed(1)} ms`);
      const entries = Math.round(viewport.scrollHeight / 36);
      if (entries < 1000) throw new Error(`nur ${entries} Einträge geladen (${labels.size} Abschnitte)`);
      return `${entries} Einträge, ${labels.size} Abschnitte, ${deltas.length} Frames, p50 ${s.p50.toFixed(1)} / p95 ${s.p95.toFixed(1)} / max ${s.max.toFixed(1)} ms, ${s.dropped} Drops`;
    },
  ],
  [
    "↓ über Abschnitte bis Repaint",
    async () => {
      await frame();
      (document.querySelector<HTMLElement>('[role="option"]'))!.click();
      await click("button", "Schließen").catch(() => undefined);
      (document.querySelector<HTMLElement>('[role="option"]'))!.focus();
      return repaint(30, () => key(focused(), "ArrowDown"));
    },
  ],
];

// P4-04: Fälligkeit per Kürzel – `T` auf der fokussierten Zeile ist ein `set_dues` über die echte IPC.
const p4keys: Step[] = [
  [
    "T auf einer Eingang-Task → erscheint in „Heute“",
    async (client) => {
      location.hash = "#/inbox";
      await until("Eingang", () => heading() === "Eingang");
      await client.createTask({ title: "Kürzel-Test" });
      const row = await until("Zeile", () => byRole("option", /^Kürzel-Test/));
      row.click();
      await click("button", "Schließen").catch(() => undefined);
      row.focus();
      key(row, "t");
      location.hash = "#/today";
      await until("in Heute", () => heading() === "Heute" && byRole("option", /^Kürzel-Test/), 5000);
      key(focused(), "d");
      await until("Menü Fälligkeit", () => document.querySelector('[role="menu"][aria-label="Fälligkeit"]'));
      key(focused(), "Escape");
    },
  ],
];

// P4-05: Befehlspalette – `search_tasks` über die echte IPC (FTS5), Öffnen nach dem ersten Laden < 50 ms.
const paletteInput = () => document.querySelector<HTMLInputElement>('dialog[open] input[role="combobox"]');
async function openPaletteTimed(): Promise<number> {
  const t0 = performance.now();
  key(focused(), "k", { ctrlKey: true });
  const input = await until("Palette", paletteInput);
  await until("Fokus", () => document.activeElement === input);
  return (await frame()) - t0;
}
const p4search: Step[] = [
  [
    "Strg+K, Teilwort einer Notiz → richtige Task im Detail",
    async (client) => {
      location.hash = "#/inbox";
      await until("Eingang", () => heading() === "Eingang");
      const list = await client.createTaskList("Palette");
      await client.createTask({ title: "Ablenkung", listId: list.id, notes: "nichts" });
      const target = await client.createTask({ title: "Gesuchte Task", listId: list.id, notes: "Steuererklärung abgeben" });
      const first = await openPaletteTimed();
      type(paletteInput()!, "ERKLÄR");
      await until("Treffer", () => document.querySelector('dialog[open] [role="option"] .in-notes'), 3000);
      const optionTitle = document.querySelector('dialog[open] [role="option"] .label')?.textContent?.trim();
      if (optionTitle !== "Gesuchte Task") throw new Error(`erster Treffer ${optionTitle}`);
      key(paletteInput()!, "Enter");
      await until("Detail", () => (document.querySelector<HTMLTextAreaElement>('[aria-label="Task-Details"] textarea[aria-label="Titel"]')?.value ?? "") === "Gesuchte Task");
      const detail = await client.getTask(target.id);
      if (detail.title !== "Gesuchte Task") throw new Error("falsche Task");
      return `erstes Öffnen (lädt die Palette) ${first.toFixed(0)} ms`;
    },
  ],
  [
    "Palette öffnet nach dem ersten Laden < 50 ms",
    async () => {
      await click("button", "Schließen").catch(() => undefined);
      const times: number[] = [];
      for (let i = 0; i < 10; i++) {
        (document.querySelector<HTMLElement>("h1") ?? document.body).focus();
        times.push(await openPaletteTimed());
        key(paletteInput()!, "Escape");
        await until("zu", () => !paletteInput());
      }
      const s = stats(times);
      if (s.p95 > 50) throw new Error(`p95 ${s.p95.toFixed(1)} ms`);
      return `p50 ${s.p50.toFixed(1)} / p95 ${s.p95.toFixed(1)} ms`;
    },
  ],
];

const SUITES: Record<string, Step[]> = { p002, p3new, p4due, p4today, p4upcoming, p4keys, p4search, p1sync, p2board, p2layout, p2perf, p2syncA, p2syncB, p2syncLocal, p1A, p1B, p0sel, p0perf, p0perf10k, p0bulkA, p0bulkB, p0bulkLocal, base, p707, p708a, p708b, p709a, p709b, p710, p711, p711live,
  p802wait, p802create, p802offA, p802offB, p802sync, p802bulk, p802auth };

/** `VITE_TASKS_SELFTEST` = Suiten mit Komma (z. B. `base,p707`); `1` = base + p707. */
export async function runSelftest(
  client: CoreClient,
  selection: string,
  report?: (line: string, exit?: boolean) => Promise<void>,
): Promise<void> {
  if (report) mark = (line) => void report(line);
  const names = selection === "1" ? ["base", "p707"] : selection.split(",").map((n) => n.trim());
  // Die Darstellung liegt im localStorage der WebView und gilt für alle Testprofile: Jede Suite
  // beginnt in der Listenansicht – außer der, die gerade das Merken von „Kanban“ prüft.
  if (!names.includes("p2layout")) {
    const listButton = await until("Kopfzeile", () => document.querySelector<HTMLElement>("h1"), 10_000)
      .then(() => document.querySelector<HTMLElement>('[role="group"][aria-label="Darstellung"] [aria-label="Liste"]'))
      .catch(() => null);
    if (listButton?.getAttribute("aria-pressed") === "false") listButton.click();
  }
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
