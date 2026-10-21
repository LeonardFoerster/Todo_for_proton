// N9-02: Verknüpfung Task ↔ Notiz – Detail „Notizen“ (über Suche verknüpfen, öffnen, lösen, neue Notiz),
// Statuszeile der Notiz „Tasks“ (öffnen, neue Task aus der Notiz), Symbol in der Zeile.
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
  const task = await client.createTask({ title: "Blatt 3" });
  const note = await client.notes.create(null, "markdown");
  await client.notes.updateBody(note.id, "# Mitschrift Analysis\nGrenzwerte", null);
  render(App, { props: { client } });
  const user = userEvent.setup();
  await screen.findByRole("option", { name: /^Blatt 3/ });
  return { client, task, note, user };
}

describe("Verknüpfungen (N9-02)", () => {
  it("links a note from the detail panel via search, marks the row, opens and unlinks it", async () => {
    const { client, task, note, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Blatt 3/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Notiz verknüpfen" }));
    const picker = await screen.findByRole("dialog", { name: "Notiz verknüpfen" });
    await user.type(within(picker).getByRole("textbox", { name: "Notiz suchen" }), "Grenz");
    await user.click(await within(picker).findByRole("button", { name: /Mitschrift Analysis/ }));
    expect(await within(detail).findByRole("button", { name: "Mitschrift Analysis" })).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByRole("option", { name: /^Blatt 3/ }).getAttribute("aria-label")).toContain("verknüpft"),
    );
    expect((await client.taskLinks(task.id)).map((l) => l.noteId)).toEqual([note.id]);

    // Öffnen wechselt in den Modus „Notes“ mit der Notiz im Editor.
    await user.click(within(detail).getByRole("button", { name: "Mitschrift Analysis" }));
    expect(await screen.findByRole("textbox", { name: "Inhalt von Mitschrift Analysis" })).toBeTruthy();
    // Statuszeile: „1 Task“ → zurück zur Task.
    await user.click(await screen.findByRole("button", { name: /1 Task/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Verknüpfte Tasks" })).getByRole("menuitem", { name: "Blatt 3" }));
    const back = await screen.findByRole("region", { name: "Task-Details" });
    expect(within(back).getByRole("textbox", { name: "Titel" })).toHaveProperty("value", "Blatt 3");

    await user.click(within(back).getByRole("button", { name: "Verknüpfung lösen: Mitschrift Analysis" }));
    await waitFor(async () => expect(await client.taskLinks(task.id)).toEqual([]));
  });

  it("creates a linked note from the task and a linked task from the note", async () => {
    const { client, task, note, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Blatt 3/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Notiz verknüpfen" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Notiz verknüpfen" })).getByRole("button", { name: /Neue Notiz/ }));
    const editor = (await screen.findByRole("textbox", { name: "Inhalt von Blatt 3" })) as HTMLTextAreaElement;
    expect(editor.value).toBe("# Blatt 3\n\n");
    expect((await client.taskLinks(task.id)).map((l) => l.name)).toEqual(["Blatt 3"]);

    // Andere Notiz öffnen und daraus eine Task anlegen.
    await user.click(await screen.findByRole("treeitem", { name: "Mitschrift Analysis.md" }));
    await screen.findByRole("textbox", { name: "Inhalt von Mitschrift Analysis" });
    await user.click(screen.getByRole("button", { name: /^Tasks$/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Verknüpfte Tasks" })).getByRole("menuitem", { name: "Neue Task aus dieser Notiz" }));
    expect(await screen.findByText("Task „Mitschrift Analysis“ angelegt und verknüpft")).toBeTruthy();
    const linked = await client.noteLinks(note.id);
    expect(linked.map((l) => l.task.title)).toEqual(["Mitschrift Analysis"]);
  });
});
