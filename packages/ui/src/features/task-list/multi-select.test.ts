import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { comboOf } from "../../lib/shortcuts";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup(titles: string[] = ["A", "B", "C", "D", "E"]) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  for (const t of titles) await client.createTask({ title: t });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: titles[0] });
  return { client, user: userEvent.setup() };
}

const listbox = () => screen.getByRole("listbox");
const row = (name: string) => screen.getByRole("option", { name });
/** Titel der ausgewählten Zeilen (im Modus: `aria-selected`). */
const checked = () =>
  within(listbox())
    .queryAllByRole("option")
    .filter((o) => o.getAttribute("aria-selected") === "true")
    .map((o) => o.getAttribute("aria-label"))
    .join("");
const status = () => screen.queryByRole("status")?.textContent?.trim();

describe("multi-select mode", () => {
  it("toggles via the header button and leaves with Fertig or Esc", async () => {
    const { user } = await setup();
    expect(listbox().getAttribute("aria-multiselectable")).toBeNull();
    await user.click(row("A"));
    expect(screen.getByRole("region", { name: "Task-Details" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Auswählen" }));
    expect(screen.queryByRole("region", { name: "Task-Details" }), "Modus schließt das Detail").toBeNull();
    expect(listbox().getAttribute("aria-multiselectable")).toBe("true");
    expect(status()).toBe("0 ausgewählt");

    await user.click(row("B"));
    await user.click(row("D"));
    expect(checked()).toBe("BD");
    expect(status()).toBe("2 ausgewählt");
    expect(screen.queryByRole("region", { name: "Task-Details" }), "Klick öffnet im Modus nicht").toBeNull();
    await user.click(row("B"));
    expect(checked()).toBe("D");

    await user.click(screen.getByRole("button", { name: "Fertig" }));
    expect(listbox().getAttribute("aria-multiselectable")).toBeNull();
    expect(screen.getByRole("button", { name: "Auswählen" })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Auswählen" }));
    await user.keyboard("{Escape}");
    expect(listbox().getAttribute("aria-multiselectable")).toBeNull();
  });

  it("starts with Ctrl+click from the cursor row and adds ranges with Shift+click", async () => {
    const { user } = await setup();
    await user.click(row("B")); // Cursor, öffnet das Detail
    await user.keyboard("{Escape}");
    await user.keyboard("{Control>}");
    await user.click(row("D"));
    await user.keyboard("{/Control}");
    expect(checked()).toBe("BD");

    await user.keyboard("{Shift>}");
    await user.click(row("E"));
    await user.keyboard("{/Shift}");
    expect(checked(), "Bereich ab Anker D").toBe("BDE");
    await user.keyboard("{Control>}");
    await user.click(row("B"));
    await user.keyboard("{/Control}");
    expect(checked()).toBe("DE");
  });

  it("starts with Shift+click as a range from the cursor", async () => {
    const { user } = await setup();
    await user.click(row("B"));
    await user.keyboard("{Escape}{Shift>}");
    await user.click(row("D"));
    await user.keyboard("{/Shift}");
    expect(checked()).toBe("BCD");
    expect(status()).toBe("3 ausgewählt");
  });

  it("extends with Shift+arrows and toggles the cursor row with X and Space", async () => {
    const { user } = await setup();
    await user.keyboard("{ArrowDown}"); // Cursor auf A
    await user.keyboard("{Shift>}{ArrowDown}{ArrowDown}{/Shift}");
    expect(checked()).toBe("ABC");
    expect(document.activeElement).toBe(row("C"));
    await user.keyboard("x");
    expect(checked()).toBe("AB");
    await user.keyboard("{ArrowDown} ");
    expect(checked(), "Leertaste wählt im Modus, statt abzuhaken").toBe("ABD");
    expect(within(listbox()).queryByRole("option", { name: /erledigt/ })).toBeNull();
  });

  it("Ctrl+A selects the whole view including unloaded pages, again selects none", async () => {
    const titles = Array.from({ length: 450 }, (_, i) => `T${String(i).padStart(3, "0")}`);
    const { user } = await setup(titles);
    row("T000").focus();
    await user.keyboard("{Control>}a{/Control}");
    await waitFor(() => expect(status()).toBe("450 ausgewählt"));
    const all = screen.getByRole("checkbox", { name: /Keine auswählen/ });
    expect(all.getAttribute("aria-checked")).toBe("true");
    await user.keyboard("{Control>}a{/Control}");
    expect(status()).toBe("0 ausgewählt");
    await user.click(row("T001"));
    expect(screen.getByRole("checkbox", { name: /Alle auswählen/ }).getAttribute("aria-checked")).toBe("mixed");
    await user.click(screen.getByRole("checkbox", { name: /Alle auswählen/ }));
    await waitFor(() => expect(status()).toBe("450 ausgewählt"));
  });

  it("keeps Ctrl+A native in text fields", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("textbox", { name: "Neue Task" }));
    await user.keyboard("abc{Control>}a{/Control}");
    expect(status()).toBeUndefined();
  });

  it("drops tasks from the selection when they leave the view", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("button", { name: "Auswählen" }));
    await user.click(row("A"));
    await user.click(row("C"));
    const c = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items[2];
    await client.deleteTask(c.id);
    await waitFor(() => expect(status()).toBe("1 ausgewählt"));
    expect(checked()).toBe("A");
  });

  it("lists the selection shortcuts in the overview", async () => {
    const { user } = await setup();
    await user.keyboard("?");
    const dialog = await screen.findByRole("dialog", { name: "Tastaturkürzel" });
    expect(within(dialog).getByRole("heading", { name: "Auswahl" })).toBeTruthy();
    expect(within(dialog).getByText("Auswahl nach unten erweitern")).toBeTruthy();
  });
});

describe("selection key combos", () => {
  it("distinguishes Shift+arrows from arrows but not Ctrl+Shift+A from Ctrl+A", () => {
    const ev = (key: string, mods: Partial<KeyboardEvent> = {}) =>
      comboOf({ key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods });
    expect(ev("ArrowDown", { shiftKey: true })).toBe("shift+arrowdown");
    expect(ev("ArrowDown")).toBe("arrowdown");
    expect(ev("a", { ctrlKey: true })).toBe("ctrl+a");
    expect(ev("A", { ctrlKey: true, shiftKey: true }), "deshalb kein eigenes Kürzel „Keine auswählen“").toBe("ctrl+a");
    expect(ev(" ")).toBe("space");
    expect(ev("x")).toBe("x");
  });
});
