import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { todayPlus } from "../../lib/format-date";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const uni = await client.createTaskList("Uni");
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  const user = userEvent.setup();
  (document.activeElement as HTMLElement | null)?.blur();
  await user.keyboard("n");
  await waitFor(() => expect(document.activeElement).toBe(title()));
  return { client, uni, user };
}

const dialog = () => screen.getByRole("dialog", { name: "Neue Task" });
const title = () => within(dialog()).getByRole("textbox", { name: "Neue Task" });
const all = async (client: FakeCoreClient) => {
  const page = await client.listTasks({ view: { kind: "all" } }, { limit: 50 });
  return Promise.all(page.items.map((t) => client.getTask(t.id)));
};

describe("new-task window", () => {
  it("recognizes due, priority and list in the title and creates in one go", async () => {
    const { client, uni, user } = await setup();
    await user.type(title(), "Blatt 3 morgen !1 @uni");
    expect(within(dialog()).getByText("Blatt 3").tagName).toBe("STRONG");
    expect(within(dialog()).getByRole("button", { name: "Priorität: Hoch" })).toBeTruthy();
    expect(within(dialog()).getByRole("button", { name: "Liste: Uni" })).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    expect(await all(client)).toMatchObject([
      { title: "Blatt 3", listId: uni.id, priority: "high", due: todayPlus(1), notes: "" },
    ]);
  });

  it("chips set by hand win over the text; notes via Strg+Enter", async () => {
    const { client, user } = await setup();
    await user.type(title(), "Bericht morgen !1");
    await user.click(within(dialog()).getByRole("button", { name: "Priorität: Hoch" }));
    await user.click(within(await screen.findByRole("menu", { name: "Priorität wählen" })).getByRole("menuitem", { name: "Niedrig" }));
    await user.click(within(dialog()).getByRole("button", { name: /Morgen/ }));
    await user.click(within(dialog()).getByRole("button", { name: "Entfernen" }));
    await user.keyboard("{Alt>}n{/Alt}");
    const notes = await within(dialog()).findByRole("textbox", { name: "Notiz" });
    await waitFor(() => expect(document.activeElement).toBe(notes));
    await user.keyboard("Kapitel 2{Control>}{Enter}{/Control}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    expect(await all(client)).toMatchObject([{ title: "Bericht", priority: "low", due: null, notes: "Kapitel 2" }]);
  });

  it("„Weitere anlegen“ keeps the window open and resets everything but the list", async () => {
    const { client, uni, user } = await setup();
    await user.click(within(dialog()).getByRole("checkbox", { name: "Weitere anlegen" }));
    await user.click(within(dialog()).getByRole("button", { name: "Liste: Eingang" }));
    await user.click(within(await screen.findByRole("menu", { name: "Liste wählen" })).getByRole("menuitem", { name: "Uni" }));
    await user.type(title(), "Blatt 1 !2{Enter}");
    await waitFor(() => expect(title()).toHaveProperty("value", ""));
    expect(within(dialog()).getByRole("button", { name: "Priorität: Keine" })).toBeTruthy();
    await user.type(title(), "Blatt 2{Enter}");
    await waitFor(async () => expect((await all(client)).map((t) => [t.title, t.listId, t.priority])).toEqual([
      ["Blatt 1", uni.id, "medium"],
      ["Blatt 2", uni.id, "none"],
    ]));
    expect(screen.getByRole("dialog", { name: "Neue Task" })).toBeTruthy();
    expect(localStorage.getItem("tasks.new-task-keep-open")).toBe("1");
  });

  it("Alt+P and Alt+D work from the title", async () => {
    const { user } = await setup();
    await user.keyboard("{Alt>}p{/Alt}");
    await screen.findByRole("menu", { name: "Priorität wählen" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    title().focus();
    await user.keyboard("{Alt>}d{/Alt}");
    expect(await within(dialog()).findByLabelText("Fällig am")).toBeTruthy();
  });
});
