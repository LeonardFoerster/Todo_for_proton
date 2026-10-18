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

async function setup(titles: string[] = ["A", "B", "C", "D", "E"]) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const work = await client.createTaskList("Arbeit");
  for (let i = 0; i < titles.length; i += 100) {
    await Promise.all(titles.slice(i, i + 100).map((title) => client.createTask({ title })));
  }
  render(App, { props: { client } });
  await screen.findByRole("option", { name: titles[0] });
  const spies = {
    getTask: vi.spyOn(client, "getTask"),
    deleteTasks: vi.spyOn(client, "deleteTasks"),
    restoreTasks: vi.spyOn(client, "restoreTasks"),
    completeTasks: vi.spyOn(client, "completeTasks"),
    moveTasks: vi.spyOn(client, "moveTasks"),
  };
  return { client, work, spies, user: userEvent.setup() };
}

const listbox = () => screen.getByRole("listbox");
const shown = () =>
  within(listbox())
    .queryAllByRole("option")
    .map((o) => o.getAttribute("aria-label"));
const stored = async (client: FakeCoreClient, view: "inbox" | { listId: string } = "inbox") =>
  (
    await client.listTasks(
      { view: view === "inbox" ? { kind: "inbox" } : { kind: "list", listId: view.listId } },
      { limit: 500 },
    )
  ).items.map((t) => `${t.title}${t.status === "done" ? "✓" : ""}`);

async function select(user: ReturnType<typeof userEvent.setup>, ...names: string[]) {
  await user.click(screen.getByRole("button", { name: "Auswählen" }));
  for (const name of names) await user.click(screen.getByRole("option", { name }));
}

const undo = () => screen.findByRole("button", { name: "Rückgängig" });
const counter = () => within(screen.getByRole("toolbar", { name: "Auswahl" })).getByRole("status").textContent;
/** Button der Aktionsleiste (Zeilen haben eigene „Löschen“-Buttons). */
const action = (name: string) => within(screen.getByRole("toolbar", { name: "Auswahl" })).getByRole("button", { name });

describe("bulk actions", () => {
  it("deletes the selection with one call and restores it at the old places via undo", async () => {
    const { client, spies, user } = await setup();
    await select(user, "B", "D");
    await user.click(action("Löschen"));
    expect(shown()).toEqual(["A", "C", "E"]);
    expect(spies.deleteTasks).toHaveBeenCalledTimes(1);
    expect(counter()).toContain("0 ausgewählt");
    expect(screen.getByText("2 Tasks gelöscht")).toBeTruthy();

    await user.click(await undo());
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C", "D", "E"]));
    await waitFor(() => expect(shown()).toEqual(["A", "B", "C", "D", "E"]));
    expect(spies.restoreTasks).toHaveBeenCalledTimes(1);
  });

  it("500 selected: one IPC call, the list is empty at once, undo brings all back in order", async () => {
    const titles = Array.from({ length: 500 }, (_, i) => `T${String(i).padStart(3, "0")}`);
    const { client, spies, user } = await setup(titles);
    screen.getByRole("option", { name: "T000" }).focus();
    await user.keyboard("{Control>}a{/Control}");
    await waitFor(() => expect(counter()).toContain("500 ausgewählt"));
    spies.getTask.mockClear();

    await user.keyboard("{Delete}");
    expect(screen.queryByRole("option")).toBeNull();
    expect(spies.deleteTasks).toHaveBeenCalledTimes(1);
    expect(spies.deleteTasks.mock.calls[0][0]).toHaveLength(500);

    await user.click(await undo());
    await waitFor(() => expect(screen.getAllByRole("option").length).toBeGreaterThan(0));
    expect(spies.restoreTasks).toHaveBeenCalledTimes(1);
    expect(await stored(client)).toEqual(titles);
    expect(shown().slice(0, 3)).toEqual(["T000", "T001", "T002"]);
    expect(spies.getTask, "großes Event lädt Seiten statt je Task").not.toHaveBeenCalled();
  });

  it("completes the selection with one call; E and undo work on the whole selection", async () => {
    const { client, spies, user } = await setup();
    await select(user, "A", "C");
    await user.keyboard("e");
    await waitFor(async () => expect(await stored(client)).toEqual(["A✓", "B", "C✓", "D", "E"]));
    expect(spies.completeTasks).toHaveBeenCalledTimes(1);
    expect(within(listbox()).getByRole("option", { name: "A, erledigt" })).toBeTruthy();
    expect(screen.getByText("2 Tasks erledigt")).toBeTruthy();

    await user.click(await undo());
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C", "D", "E"]));
    expect(spies.completeTasks).toHaveBeenLastCalledWith(expect.arrayContaining([]), false);
  });

  it("moves the selection to another list and undo puts every task back at its old place", async () => {
    const { client, work, spies, user } = await setup();
    await select(user, "B", "D");
    await user.click(action("Verschieben nach…"));
    const menu = await screen.findByRole("menu", { name: "Verschieben nach" });
    expect(within(menu).getByRole("menuitem", { name: "Eingang" }).hasAttribute("disabled")).toBe(true);
    await user.click(within(menu).getByRole("menuitem", { name: "Arbeit" }));

    expect(shown()).toEqual(["A", "C", "E"]);
    await waitFor(async () => expect(await stored(client, { listId: work.id })).toEqual(["B", "D"]));
    expect(spies.moveTasks).toHaveBeenCalledTimes(1);
    expect(screen.getByText("2 Tasks nach „Arbeit“ verschoben")).toBeTruthy();

    await user.click(await undo());
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C", "D", "E"]));
    await waitFor(() => expect(shown()).toEqual(["A", "B", "C", "D", "E"]));
    expect(spies.moveTasks, "B und D lagen zwischen verschiedenen Nachbarn").toHaveBeenCalledTimes(3);
  });

  it("M opens the move menu; the context menu of a selected row acts on the selection", async () => {
    const { client, work, user } = await setup();
    await select(user, "A", "B");
    await user.keyboard("m");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();

    await fireEvent.contextMenu(screen.getByRole("option", { name: "B" }));
    const menu = await screen.findByRole("menu", { name: "Aktionen für 2 Tasks" });
    expect(within(menu).getByRole("menuitem", { name: /^2 Tasks löschen/ })).toBeTruthy();
    await user.click(within(menu).getByRole("menuitem", { name: /^Verschieben nach…/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Verschieben nach" })).getByRole("menuitem", { name: "Arbeit" }));
    await waitFor(async () => expect(await stored(client, { listId: work.id })).toEqual(["A", "B"]));

    await fireEvent.contextMenu(screen.getByRole("option", { name: "D" }));
    expect(await screen.findByRole("menu", { name: "Aktionen für D" })).toBeTruthy();
  });

  it("puts rows back when deleting fails – also for a single task (FIX-08)", async () => {
    const { client, spies, user } = await setup();
    spies.deleteTasks.mockRejectedValue(new CoreClientError("storage", "Platte voll"));
    await select(user, "B", "C");
    await user.click(action("Löschen"));
    await waitFor(() => expect(shown()).toEqual(["A", "B", "C", "D", "E"]));
    expect(screen.getByText(/Nicht gelöscht: Platte voll/)).toBeTruthy();

    await user.click(action("Fertig"));
    await user.click(screen.getByRole("option", { name: "D" }));
    await user.keyboard("{Escape}");
    screen.getByRole("option", { name: "D" }).focus();
    await user.keyboard("{Delete}");
    await waitFor(() => expect(shown()).toEqual(["A", "B", "C", "D", "E"]));
    expect(await stored(client)).toEqual(["A", "B", "C", "D", "E"]);
  });

  it("resets the rows when completing fails", async () => {
    const { spies, user } = await setup();
    spies.completeTasks.mockRejectedValue(new CoreClientError("storage", "kaputt"));
    await select(user, "A");
    await user.click(action("Erledigt"));
    await waitFor(() => expect(screen.getByText(/Nicht gespeichert: kaputt/)).toBeTruthy());
    expect(within(listbox()).getByRole("option", { name: "A" })).toBeTruthy();
  });

  it("single delete uses the bulk path and keeps all fields on undo (FIX-09)", async () => {
    const { client, spies, user } = await setup();
    const c = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items[2];
    await client.updateTask(c.id, { notes: "Notiz", priority: "high" });
    const createdAt = (await client.getTask(c.id)).createdAt;
    await user.click(screen.getByRole("option", { name: "C" }));
    await user.keyboard("{Escape}");
    screen.getByRole("option", { name: "C" }).focus();
    await user.keyboard("{Delete}");
    expect(spies.deleteTasks).toHaveBeenCalledWith([c.id]);
    await user.click(await undo());
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C", "D", "E"]));
    const copy = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items[2];
    expect(await client.getTask(copy.id)).toMatchObject({ notes: "Notiz", priority: "high", createdAt });
  });
});
