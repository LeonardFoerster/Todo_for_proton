import { CoreClientError, FakeCoreClient } from "@tasks/core-client";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup(names = ["Arbeit", "Einkaufen", "Privat"]) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const ids: Record<string, string> = {};
  for (const n of names) ids[n] = (await client.createTaskList(n)).id;
  render(App, { props: { client } });
  await screen.findByRole("link", { name: names[0] });
  return { client, ids, user: userEvent.setup() };
}

const nav = () => screen.getByRole("navigation", { name: "Navigation" });
const sidebarLists = () =>
  [...nav().querySelectorAll<HTMLElement>("a[data-list-id]")].map((a) => a.textContent?.trim());
const stored = async (client: FakeCoreClient) => (await client.listTaskLists()).map((l) => l.name);
const menuItem = async (name: RegExp) => within(await screen.findByRole("menu")).getByRole("menuitem", { name });

describe("managing lists", () => {
  it("renames inline from the context menu; Esc cancels, empty keeps the name", async () => {
    const { client, user } = await setup();
    await fireEvent.contextMenu(screen.getByRole("link", { name: "Einkaufen" }));
    await user.click(await menuItem(/^Umbenennen/));
    const input = screen.getByRole("textbox", { name: "Neuer Name der Liste" });
    expect((input as HTMLInputElement).value).toBe("Einkaufen");
    await user.clear(input);
    await user.type(input, "Supermarkt{Enter}");
    await waitFor(async () => expect(await stored(client)).toContain("Supermarkt"));
    expect(sidebarLists()).toEqual(["Arbeit", "Supermarkt", "Privat"]);

    await fireEvent.contextMenu(screen.getByRole("link", { name: "Privat" }));
    await user.click(await menuItem(/^Umbenennen/));
    await user.keyboard("Egal{Escape}");
    expect(sidebarLists()).toEqual(["Arbeit", "Supermarkt", "Privat"]);

    await fireEvent.contextMenu(screen.getByRole("link", { name: "Privat" }));
    await user.click(await menuItem(/^Umbenennen/));
    await user.clear(screen.getByRole("textbox", { name: "Neuer Name der Liste" }));
    await user.keyboard("{Enter}");
    expect(await stored(client)).toContain("Privat");
  });

  it("shows an error toast and keeps the old name when renaming fails", async () => {
    const { client, user } = await setup();
    vi.spyOn(client, "renameTaskList").mockRejectedValueOnce(new CoreClientError("storage", "kaputt"));
    await fireEvent.contextMenu(screen.getByRole("link", { name: "Arbeit" }));
    await user.click(await menuItem(/^Umbenennen/));
    await user.type(screen.getByRole("textbox", { name: "Neuer Name der Liste" }), "X{Enter}");
    await screen.findByText(/Nicht umbenannt: kaputt/);
    await waitFor(() => expect(sidebarLists()[0]).toBe("Arbeit"));
  });

  it("deletes after confirming; tasks move to the inbox and the view falls back to it", async () => {
    const { client, ids, user } = await setup();
    await client.createTask({ title: "Milch", listId: ids.Einkaufen });
    const done = await client.createTask({ title: "Brot", listId: ids.Einkaufen });
    await client.completeTask(done.id, true);
    location.hash = `#/list/${ids.Einkaufen}`;
    await screen.findByRole("heading", { name: "Einkaufen" });

    await user.click(screen.getByRole("button", { name: "Listenaktionen" }));
    await user.click(await menuItem(/^Löschen/));
    const dialog = await screen.findByRole("dialog", { name: "Liste löschen?" });
    await within(dialog).findByText(/1 Task offene und 1 Task erledigte/);
    await user.click(within(dialog).getByRole("button", { name: "Abbrechen" }));
    expect(await stored(client)).toContain("Einkaufen");

    await user.click(screen.getByRole("button", { name: "Listenaktionen" }));
    await user.click(await menuItem(/^Löschen/));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Liste löschen" }));
    await waitFor(async () => expect(await stored(client)).not.toContain("Einkaufen"));
    await screen.findByRole("heading", { name: "Eingang" });
    await screen.findByRole("option", { name: "Milch" });
    expect(screen.getByText(/Liste „Einkaufen“ gelöscht · 2 Tasks im Eingang/)).toBeTruthy();
  });

  it("reorders with Alt+arrows on a focused list and by dragging its grip; the inbox stays first", async () => {
    const { client, user } = await setup();
    screen.getByRole("link", { name: "Arbeit" }).focus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    await waitFor(async () => expect(await stored(client)).toEqual(["Inbox", "Einkaufen", "Arbeit", "Privat"]));
    expect(sidebarLists()).toEqual(["Einkaufen", "Arbeit", "Privat"]);
    expect(document.activeElement?.textContent?.trim(), "Fokus bleibt auf der Liste").toBe("Arbeit");

    // jsdom: Einträge je 35 px ab y = 0 (Mitten 17,5 / 52,5 / 87,5).
    const items = nav().querySelectorAll<HTMLElement>("li[data-index]");
    items.forEach((li, i) => {
      li.getBoundingClientRect = () => ({ top: i * 35, bottom: i * 35 + 34, height: 34 }) as DOMRect;
    });
    const grip = screen.getByRole("link", { name: "Privat" }).querySelector(".grip")!;
    await fireEvent.pointerDown(grip, { button: 0, clientY: 87 });
    await fireEvent.pointerMove(window, { clientY: 5 });
    await fireEvent.pointerUp(window, { clientY: 5 });
    await waitFor(async () => expect(await stored(client)).toEqual(["Inbox", "Privat", "Einkaufen", "Arbeit"]));
  });

  it("renames via dialog when the sidebar is collapsed", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("button", { name: "Seitenleiste einklappen" }));
    await fireEvent.contextMenu(screen.getByRole("link", { name: "Privat" }));
    await user.click(await menuItem(/^Umbenennen/));
    const dialog = await screen.findByRole("dialog", { name: "Liste umbenennen" });
    const input = within(dialog).getByRole("textbox", { name: "Neuer Name der Liste" });
    await user.clear(input);
    await user.click(within(dialog).getByRole("button", { name: "Umbenennen" }));
    expect(within(dialog).getByRole("alert").textContent).toContain("leer");
    await user.type(input, "Familie{Enter}");
    await waitFor(async () => expect(await stored(client)).toContain("Familie"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("Esc while naming a new list creates nothing", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("button", { name: "Neue Liste" }));
    await user.keyboard("Abbruch{Escape}");
    await new Promise((r) => setTimeout(r, 50));
    expect(await stored(client)).not.toContain("Abbruch");
  });

  it("has no actions for the inbox", async () => {
    await setup();
    expect(screen.getByRole("link", { name: /^Eingang/ }).dataset.listId).toBeUndefined();
    location.hash = "#/inbox";
    await screen.findByRole("heading", { name: "Eingang" });
    expect(screen.queryByRole("button", { name: "Listenaktionen" })).toBeNull();
  });
});
