// Ganze App im Browser mit FakeCoreClient: http://localhost:5180/app.html (?n=1000 für viele Tasks,
// ?latency=50 für langsame Aufrufe).
import { FakeCoreClient } from "@tasks/core-client";
import { mount } from "svelte";
import { App, setThemePreference } from "../src";
import { seed } from "./seed";

const params = new URLSearchParams(location.search);
// Für Screenshots: ?theme=dark|light, ?collapsed=1
const theme = params.get("theme");
if (theme === "dark" || theme === "light") setThemePreference(theme);
// ?layout=board|list: Darstellung (P5-Screenshots); ?mode=notes|tasks: Modus.
for (const [param, key] of [["layout", "tasks.layout"], ["mode", "tasks.mode"]]) {
  const value = params.get(param);
  if (value) localStorage.setItem(key, value);
}
if (params.has("collapsed")) localStorage.setItem("tasks.sidebar-collapsed", params.get("collapsed") === "1" ? "1" : "0");
const client = new FakeCoreClient({ latencyMs: Number(params.get("latency") ?? 0), loginPolls: Number(params.get("polls") ?? 1) });
if (params.get("seed") === "0") await client.ensureInbox();
else await seed(client, Number(params.get("n") ?? 0));
mount(App, { target: document.getElementById("app")!, props: { client } });

// ?open=1: erste Task öffnen (Detail-Panel), ?type=Text: Quick-Add vorbelegen (Screenshots).
if (params.has("open")) setTimeout(() => document.querySelector<HTMLElement>('[role="option"]')?.click(), 300);
// ?due=1 (mit ?open=1): Fälligkeits-Popover im Detail öffnen (P5-08).
if (params.has("due")) setTimeout(() => document.querySelector<HTMLElement>("#detail-due")?.click(), 600);
const typed = params.get("type");
if (typed) {
  setTimeout(() => {
    const input = document.querySelector<HTMLInputElement>('input[aria-label="Neue Task"]')!;
    input.value = typed;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  }, 300);
}
// ?menu=1: Kontextmenü der zweiten Zeile öffnen (Screenshots).
if (params.has("menu")) {
  setTimeout(() => {
    const row = document.querySelectorAll<HTMLElement>('[role="option"]')[1];
    const r = row.getBoundingClientRect();
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 220, clientY: r.top + 24 }));
  }, 400);
}
// ?settings=1: Einstellungen öffnen; &login=1: Anmeldung starten (Fake: wartet einen Poll, dann angemeldet).
if (params.has("settings")) {
  setTimeout(async () => {
    document.querySelector<HTMLElement>('[aria-label="Einstellungen"]')?.click();
    if (params.has("login")) {
      for (let i = 0; i < 20; i++) {
        const button = [...document.querySelectorAll<HTMLButtonElement>("dialog button")].find((b) => /anmelden/i.test(b.textContent ?? ""));
        if (button && !button.disabled) {
          button.click();
          break;
        }
        await new Promise((r) => setTimeout(r, 50));
      }
    }
  }, 400);
}
