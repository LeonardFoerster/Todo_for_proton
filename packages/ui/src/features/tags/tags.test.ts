// N8-03: Tags in der Oberfläche – „#tag“ im Fenster „Neue Task“, Chips in Zeile und Detail, Ansicht je Tag
// in der Seitenleiste, Umbenennen/Löschen, Palette.
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
  await client.ensureInbox();
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  const user = userEvent.setup();
  return { client, user };
}

const nav = () => screen.getByRole("navigation", { name: "Navigation" });

async function newTask(user: ReturnType<typeof userEvent.setup>, text: string) {
  (document.activeElement as HTMLElement | null)?.blur();
  await user.keyboard("n");
  const dialog = await screen.findByRole("dialog", { name: "Neue Task" });
  const input = within(dialog).getByRole("textbox", { name: "Neue Task" });
  await waitFor(() => expect(document.activeElement).toBe(input));
  await user.type(input, text);
  return dialog;
}

describe("Tags (N8-03)", () => {
  it("#tag in the new-task window creates and assigns it; row, sidebar and tag view show it", async () => {
    const { client, user } = await setup();
    const dialog = await newTask(user, "Blatt 3 #uni");
    expect([...dialog.querySelectorAll("mark.tag")].map((m) => m.textContent)).toEqual(["#uni"]);
    expect(within(dialog).getByRole("button", { name: "Tags: #uni" })).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    const [tag] = await client.listTags();
    expect(tag.name).toBe("uni");
    const row = await screen.findByRole("option", { name: /^Blatt 3/ });
    await waitFor(() => expect(row.textContent).toContain("#uni"));

    await user.click(await within(nav()).findByRole("link", { name: /uni/ }));
    expect(await screen.findByRole("heading", { name: "#uni", level: 1 })).toBeTruthy();
    expect(screen.getByRole("option", { name: /^Blatt 3/ })).toBeTruthy();

    // Neue Task in der Tag-Ansicht bekommt den Tag.
    await newTask(user, "Blatt 4");
    await user.keyboard("{Enter}");
    await screen.findByRole("option", { name: /^Blatt 4/ });
    const all = (await client.listTasks({ view: { kind: "all" }, tag: tag.id }, { limit: 10 })).items;
    expect(all.map((t) => t.title).sort()).toEqual(["Blatt 3", "Blatt 4"]);
  });

  it("detail panel adds a tag from the picker and removes it again", async () => {
    const { client, user } = await setup();
    const task = await client.createTask({ title: "Einkauf" });
    await user.click(await screen.findByRole("option", { name: /^Einkauf/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Tag" }));
    const picker = await screen.findByRole("dialog", { name: "Tags wählen" });
    await user.type(within(picker).getByRole("textbox", { name: "Tag suchen" }), "zu Hause{Enter}");
    await waitFor(async () => expect((await client.getTask(task.id)).tags).toHaveLength(1));
    expect((await client.listTags()).map((t) => t.name)).toEqual(["zu-Hause"]);
    await user.keyboard("{Escape}");
    await user.click(await within(detail).findByRole("button", { name: "Tag entfernen: zu-Hause" }));
    await waitFor(async () => expect((await client.getTask(task.id)).tags).toEqual([]));
  });

  it("renames and deletes a tag from its view; palette jumps to it", async () => {
    const { client, user } = await setup();
    const tag = await client.ensureTag("uni");
    await client.createTask({ title: "Blatt", tags: [tag.id] });
    await user.click(await within(nav()).findByRole("link", { name: /uni/ }));
    await screen.findByRole("heading", { name: "#uni", level: 1 });

    await user.click(screen.getByRole("button", { name: "Tag-Aktionen" }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Umbenennen" }));
    const rename = await screen.findByRole("dialog", { name: "Tag umbenennen" });
    const field = within(rename).getByRole("textbox");
    await user.clear(field);
    await user.type(field, "studium{Enter}");
    expect(await screen.findByRole("heading", { name: "#studium", level: 1 })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Tag-Aktionen" }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: /Löschen/ }));
    const confirm = await screen.findByRole("dialog", { name: "Tag löschen?" });
    await user.click(within(confirm).getByRole("button", { name: "Tag löschen" }));
    expect(await screen.findByText("Tag „#studium“ gelöscht · von 1 Task entfernt")).toBeTruthy();
    await screen.findByRole("heading", { name: "Eingang", level: 1 });
    expect(await client.listTags()).toEqual([]);

    const other = await client.ensureTag("bio");
    await waitFor(() => expect(within(nav()).getByRole("link", { name: /bio/ })).toBeTruthy());
    await user.keyboard("{Control>}k{/Control}");
    const palette = await screen.findByRole("dialog", { name: /Suchen/ });
    await user.type(within(palette).getByRole("combobox"), "#bio");
    await user.click(await within(palette).findByRole("option", { name: /Gehe zu #bio/ }));
    expect(await screen.findByRole("heading", { name: "#bio", level: 1 })).toBeTruthy();
    expect(location.hash).toBe(`#/tag/${encodeURIComponent(other.id)}`);
  });
});
