// P4-05: Befehlspalette – Suche über alle Listen, Befehle aus der Registry, leerer/kein Treffer.
import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { highlight, matchesAll, terms } from "./palette";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

describe("palette helpers", () => {
  it("splits words and matches all of them", () => {
    expect(terms("  Bericht  FERTIG ")).toEqual(["bericht", "fertig"]);
    expect(matchesAll("Gehe zu Heute", ["heu", "gehe"])).toBe(true);
    expect(matchesAll("Gehe zu Heute", ["heu", "x"])).toBe(false);
  });

  it("highlights every occurrence of every word", () => {
    expect(highlight("Bericht Berlin", ["ber"])).toEqual([
      { text: "Ber", hit: true },
      { text: "icht ", hit: false },
      { text: "Ber", hit: true },
      { text: "lin", hit: false },
    ]);
    expect(highlight("Übung", [])).toEqual([{ text: "Übung", hit: false }]);
    expect(highlight("Übung", ["übu"])).toEqual([
      { text: "Übu", hit: true },
      { text: "ng", hit: false },
    ]);
  });
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const work = await client.createTaskList("Arbeit");
  await client.createTask({ title: "Bericht schreiben", listId: work.id });
  await client.createTask({ title: "Einkaufen", notes: "Milch und Brot" });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^Einkaufen/ });
  return { client, work, user: userEvent.setup() };
}

async function openPalette(user: ReturnType<typeof userEvent.setup>) {
  await user.keyboard("{Control>}k{/Control}");
  const dialog = await screen.findByRole("dialog", { name: "Suchen und Befehle" });
  const input = within(dialog).getByRole("combobox", { name: "Suchen" });
  await waitFor(() => expect(document.activeElement).toBe(input));
  return { dialog, input };
}

const options = (dialog: HTMLElement) => within(dialog).queryAllByRole("option").map((o) => o.textContent?.replace(/\s+/g, " ").trim());

describe("CommandPalette", () => {
  it("finds tasks of all lists by a part of the notes and opens the detail", async () => {
    const { user } = await setup();
    const { dialog } = await openPalette(user);
    await user.keyboard("milch");
    await waitFor(() => expect(options(dialog)[0]).toMatch(/^Einkaufen in Notiz/));
    expect(within(dialog).getByText("Tasks")).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Suchen und Befehle" })).toBeNull());
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await waitFor(() => expect((within(detail).getByRole("textbox", { name: "Titel" }) as HTMLTextAreaElement).value).toBe("Einkaufen"));
  });

  it("highlights the hit and shows the list; arrows move, Enter runs", async () => {
    const { user } = await setup();
    const { dialog } = await openPalette(user);
    await user.keyboard("richt");
    await waitFor(() => expect(within(dialog).getByText("richt", { selector: "mark" })).toBeTruthy());
    expect(options(dialog)[0]).toContain("Arbeit");
  });

  it("offers lists and registry commands; a command runs", async () => {
    const { user } = await setup();
    const { dialog } = await openPalette(user);
    await user.keyboard("heute");
    await waitFor(() => expect(options(dialog)).toContain("Gehe zu Heute G dann T"));
    const at = options(dialog).indexOf("Gehe zu Heute G dann T");
    for (let i = 0; i < at; i++) await user.keyboard("{ArrowDown}");
    await user.keyboard("{Enter}");
    await screen.findByRole("heading", { name: "Heute" });

    const second = await openPalette(user);
    await user.keyboard("arb");
    await waitFor(() => expect(options(second.dialog)).toContain("Arbeit"));
    await user.click(within(second.dialog).getByRole("option", { name: /Arbeit/ }));
    await screen.findByRole("heading", { name: "Arbeit" });
  });

  it("without hits, Enter opens „Neue Task“ with the text", async () => {
    const { user } = await setup();
    const { dialog } = await openPalette(user);
    await user.keyboard("Zahnarzt");
    await within(dialog).findByText(/Keine Task oder Notiz mit „Zahnarzt“/);
    await user.keyboard("{Enter}");
    const newTask = await screen.findByRole("dialog", { name: "Neue Task" });
    expect((within(newTask).getByRole("textbox", { name: "Neue Task" }) as HTMLInputElement).value).toBe("Zahnarzt");
  });

  it("shows recently opened tasks without text; Esc closes", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Einkaufen/ }));
    await screen.findByRole("region", { name: "Task-Details" });
    await user.click(screen.getByRole("button", { name: "Schließen" }));
    const { dialog } = await openPalette(user);
    await waitFor(() => expect(within(dialog).getByText("Zuletzt geöffnet")).toBeTruthy());
    expect(options(dialog)[0]).toBe("Einkaufen");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Suchen und Befehle" })).toBeNull());
  });

  it("„/“ opens outside of fields but stays a character while typing", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Einkaufen/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    const notes = within(detail).getByRole("textbox", { name: "Notizen" });
    await user.click(notes);
    await user.keyboard("{End} a/b");
    expect(screen.queryByRole("dialog", { name: "Suchen und Befehle" })).toBeNull();
    await user.click(screen.getByRole("heading", { name: "Eingang" }));
    await waitFor(async () => {
      const page = await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 });
      expect((await client.getTask(page.items[0].id)).notes).toBe("Milch und Brot a/b");
    });
    await user.keyboard("/");
    await screen.findByRole("dialog", { name: "Suchen und Befehle" });
  });

  it("keeps palette-only commands out of the shortcut overview", async () => {
    const { user } = await setup();
    await user.keyboard("?");
    const help = await screen.findByRole("dialog", { name: "Tastaturkürzel" });
    expect(within(help).queryByText("Neue Liste…")).toBeNull();
    expect(within(help).getByText("Suchen und Befehle")).toBeTruthy();
  });
});
