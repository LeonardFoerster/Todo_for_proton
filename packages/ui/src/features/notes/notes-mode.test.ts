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

async function setup(prepare?: (client: FakeCoreClient) => Promise<void>) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  await client.createTask({ title: "Milch" });
  await client.createTask({ title: "Brot" });
  await prepare?.(client);
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^Milch$/ });
  return { client, user: userEvent.setup() };
}

const items = () => screen.queryAllByRole("treeitem").map((r) => r.getAttribute("aria-label"));
const toNotes = (user: ReturnType<typeof userEvent.setup>) =>
  user.click(screen.getByRole("button", { name: /Zu Notes wechseln/ }));

describe("Notes mode", () => {
  it("switches by click and Ctrl+1/2 and keeps the state of both modes", async () => {
    const { user, client } = await setup(async (c) => {
      const note = await c.notes.create(null, "markdown");
      await c.notes.updateBody(note.id, "# Blatt 3\nAufgabe", null);
    });
    // Tasks: eine Task öffnen.
    await user.click(screen.getByRole("option", { name: /^Brot$/ }));
    await screen.findByRole("textbox", { name: "Titel" });

    await toNotes(user);
    const row = await screen.findByRole("treeitem", { name: "Blatt 3.md" });
    expect(localStorage.getItem("tasks.mode")).toBe("notes");
    expect(screen.getByRole("button", { name: /Zu Tasks wechseln/ }).textContent).toContain("Notes");
    await user.click(row);
    const editor = await screen.findByRole("textbox", { name: "Inhalt von Blatt 3" });
    await waitFor(() => expect((editor as HTMLTextAreaElement).value).toBe("# Blatt 3\nAufgabe"));
    // Task-Kürzel wirken hier nicht (kein Fenster „Neue Task").
    expect(screen.queryByRole("dialog")).toBeNull();

    await user.keyboard("{Control>}1{/Control}");
    await waitFor(() => expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Eingang"));
    expect(screen.getByRole("textbox", { name: "Titel" })).toHaveProperty("value", "Brot");
    await user.keyboard("{Control>}2{/Control}");
    expect(await screen.findByRole("textbox", { name: "Inhalt von Blatt 3" })).toBeTruthy();
    expect(screen.getByRole("treeitem", { name: "Blatt 3.md" }).getAttribute("aria-selected")).toBe("true");
    expect((await client.notes.listItems(null, { limit: 10 })).items).toHaveLength(1);
  });

  it("starts in the remembered mode with the remembered note", async () => {
    localStorage.setItem("tasks.mode", "notes");
    const client = new FakeCoreClient();
    await client.ensureInbox();
    const note = await client.notes.create(null, "text");
    await client.notes.updateBody(note.id, "Einkauf\nMilch", null);
    localStorage.setItem("notes.open", JSON.stringify(note.id));
    render(App, { props: { client } });
    const editor = await screen.findByRole("textbox", { name: "Inhalt von Einkauf" });
    await waitFor(() => expect((editor as HTMLTextAreaElement).value).toBe("Einkauf\nMilch"));
  });

  it("navigates the tree with the keyboard and reorders with Alt+arrows", async () => {
    const { user, client } = await setup(async (c) => {
      const uni = await c.notes.createFolder(null, "Uni");
      const a = await c.notes.create(uni.id, "markdown");
      await c.notes.rename(a.id, "Analysis");
      const b = await c.notes.create(null, "text");
      await c.notes.rename(b.id, "Einkauf");
    });
    await toNotes(user);
    const uni = await screen.findByRole("treeitem", { name: "Uni" });
    uni.focus();
    expect(uni.getAttribute("aria-expanded")).toBe("false");
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(items()).toEqual(["Uni", "Analysis.md", "Einkauf.txt"]));
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement?.getAttribute("aria-label")).toBe("Analysis.md");
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement?.getAttribute("aria-label")).toBe("Uni");
    await user.keyboard("{ArrowLeft}");
    await waitFor(() => expect(items()).toEqual(["Uni", "Einkauf.txt"]));
    await user.keyboard("j{Alt>}{ArrowUp}{/Alt}");
    await waitFor(() => expect(items()).toEqual(["Einkauf.txt", "Uni"]));
    expect((await client.notes.listItems(null, { limit: 10 })).items.map((i) => i.name)).toEqual(["Einkauf", "Uni"]);
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("textbox", { name: "Inhalt von Einkauf" })).toBeTruthy();
  });

  it("moves a note into a folder via “Verschieben nach…”", async () => {
    const { user, client } = await setup(async (c) => {
      await c.notes.createFolder(null, "Uni");
      const n = await c.notes.create(null, "text");
      await c.notes.rename(n.id, "Lose");
    });
    await toNotes(user);
    (await screen.findByRole("treeitem", { name: "Lose.txt" })).focus();
    await user.keyboard("m");
    const dialog = await screen.findByRole("dialog");
    await user.keyboard("uni{Enter}");
    await waitFor(() => expect(dialog.isConnected).toBe(false));
    await waitFor(() => expect(items()).toEqual(["Uni", "Lose.txt"]));
    const uni = (await client.notes.listFolders())[0];
    expect((await client.notes.listItems(uni.id, { limit: 10 })).items.map((i) => i.name)).toEqual(["Lose"]);
  });

  it("trashes with undo and shows top items in the trash view", async () => {
    const { user, client } = await setup(async (c) => {
      const n = await c.notes.create(null, "markdown");
      await c.notes.rename(n.id, "Alt");
    });
    await toNotes(user);
    (await screen.findByRole("treeitem", { name: "Alt.md" })).focus();
    await user.keyboard("{Delete}");
    const toast = await screen.findByRole("status");
    expect(toast.textContent).toContain("„Alt“ in den Papierkorb gelegt");
    await waitFor(() => expect(items()).toEqual([]));
    await user.click(screen.getByRole("button", { name: /Papierkorb/ }));
    expect(await screen.findByRole("heading", { name: "Papierkorb" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Alt" })).toBeTruthy();

    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(items()).toEqual(["Alt.md"]));
    expect(await client.notes.listTrash()).toEqual([]);
  });

  it("creates a note with N, focuses the editor and discards it when left empty", async () => {
    const { user, client } = await setup();
    await toNotes(user);
    await screen.findByText("Keine Notiz geöffnet");
    await user.keyboard("n");
    const editor = await screen.findByRole("textbox", { name: "Inhalt von Unbenannt" });
    await waitFor(() => expect(document.activeElement).toBe(editor));
    expect((await client.notes.listItems(null, { limit: 10 })).items).toHaveLength(1);
    // Zweite neue Notiz: die erste (leer) wird verworfen.
    await user.keyboard("{Control>}n{/Control}");
    await waitFor(async () => expect((await client.notes.listItems(null, { limit: 10 })).items).toHaveLength(1));
  });
});
