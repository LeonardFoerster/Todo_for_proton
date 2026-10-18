import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup() {
  const client = new FakeCoreClient();
  const inbox = await client.ensureInbox();
  const shop = await client.createTaskList("Einkaufen");
  const work = await client.createTaskList("Arbeit");
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  return { client, inbox, shop, work, user: userEvent.setup() };
}

const quickAdd = () => within(screen.getByRole("dialog", { name: "Neue Task" })).getByRole("textbox", { name: "Neue Task" });
const picker = () => quickAdd().parentElement!.querySelector<HTMLButtonElement>("button[aria-haspopup=menu]")!;
/** Fenster „Neue Task“ öffnen (Taste N) und warten, bis das Feld fokussiert ist. */
async function openNewTask(user: ReturnType<typeof userEvent.setup>) {
  (document.activeElement as HTMLElement | null)?.blur();
  await user.keyboard("n");
  await screen.findByRole("dialog", { name: "Neue Task" });
  await waitFor(() => expect(document.activeElement).toBe(quickAdd()));
}
const pick = async (user: ReturnType<typeof userEvent.setup>, name: string) => {
  await user.click(picker());
  await user.click(within(await screen.findByRole("menu", { name: "Liste wählen" })).getByRole("menuitem", { name }));
};
const tasksIn = async (client: FakeCoreClient, listId: string) =>
  (await client.listTasks({ view: { kind: "list", listId } }, { limit: 50 })).items.map((t) => t.title);

describe("choosing the list when adding a task", () => {
  it("preselects the list of the view", async () => {
    const { shop, user } = await setup();
    await openNewTask(user);
    expect(picker().getAttribute("aria-label")).toBe("Liste: Eingang");
    await user.keyboard("{Escape}");
    location.hash = `#/list/${shop.id}`;
    await screen.findByRole("heading", { name: "Einkaufen" });
    await openNewTask(user);
    expect(picker().getAttribute("aria-label")).toBe("Liste: Einkaufen");
    await user.keyboard("{Escape}");
    location.hash = "#/today";
    await screen.findByRole("heading", { name: "Heute" });
    await openNewTask(user);
    expect(picker().getAttribute("aria-label")).toBe("Liste: Eingang");
  });

  it("adds into the chosen list, says where and offers to open it", async () => {
    const { client, shop, user } = await setup();
    await openNewTask(user);
    await pick(user, "Einkaufen");
    expect(document.activeElement, "zurück ins Eingabefeld").toBe(quickAdd());
    await user.type(quickAdd(), "Milch{Enter}");
    await waitFor(async () => expect(await tasksIn(client, shop.id)).toEqual(["Milch"]));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    const toast = await screen.findByText("„Milch“ in „Einkaufen“ angelegt");
    await user.click(within(toast.closest("[role=status]") as HTMLElement).getByRole("button", { name: "Öffnen" }));
    await screen.findByRole("heading", { name: "Einkaufen" });
    await screen.findByRole("option", { name: "Milch" });
  });

  it("says nothing when the task lands in the shown list", async () => {
    const { user } = await setup();
    await openNewTask(user);
    await user.type(quickAdd(), "Brot{Enter}");
    await screen.findByRole("option", { name: "Brot" });
    expect(screen.queryByText(/angelegt/)).toBeNull();
  });

  it("Alt+L opens the picker from the input; Neue Liste… creates and selects", async () => {
    const { client, user } = await setup();
    await openNewTask(user);
    await user.keyboard("{Alt>}l{/Alt}");
    const menu = await screen.findByRole("menu", { name: "Liste wählen" });
    await user.click(within(menu).getByRole("menuitem", { name: /^Neue Liste/ }));
    const dialog = await screen.findByRole("dialog", { name: "Neue Liste" });
    await user.type(within(dialog).getByRole("textbox", { name: "Name der neuen Liste" }), "Garten{Enter}");
    await waitFor(() => expect(picker().getAttribute("aria-label")).toBe("Liste: Garten"));
    const garten = (await client.listTaskLists()).find((l) => l.name === "Garten")!;
    await user.click(quickAdd());
    await user.type(quickAdd(), "Rasen mähen{Enter}");
    await waitFor(async () => expect(await tasksIn(client, garten.id)).toEqual(["Rasen mähen"]));
  });
});
