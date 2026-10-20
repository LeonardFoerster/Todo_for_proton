import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  await client.createTask({ title: "Grenzwert nachrechnen" });
  const uni = await client.notes.createFolder(null, "Uni");
  const note = await client.notes.create(uni.id, "markdown");
  await client.notes.updateBody(note.id, "# Analysis\n\nDer Grenzwert der Folge ist 1.", null);
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /Grenzwert nachrechnen/ });
  return { client, note, user: userEvent.setup() };
}

describe("palette searches notes", () => {
  it("shows tasks and notes, and opens a note at the hit in notes mode", async () => {
    const { user } = await setup();
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByRole("combobox", { name: "Suchen" });
    await user.keyboard("grenzwert");
    const notesGroup = await screen.findByRole("group", { name: "Notizen" }, { timeout: 4000 });
    const tasksGroup = screen.getByRole("group", { name: "Tasks" });
    // Im Modus „Tasks" stehen Tasks zuerst.
    expect(tasksGroup.compareDocumentPosition(notesGroup) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const hit = within(notesGroup).getByRole("option");
    expect(hit.textContent).toContain("Analysis");
    expect(hit.textContent).toContain("Uni");
    expect(hit.querySelector("mark")?.textContent?.toLowerCase()).toBe("grenzwert");

    await user.click(hit);
    const editor = (await screen.findByRole("textbox", { name: "Inhalt von Analysis" })) as HTMLTextAreaElement;
    await waitFor(() => expect(editor.value).toContain("Grenzwert"));
    await waitFor(() => expect(editor.selectionStart).toBe(16));
    expect(editor.selectionEnd).toBe(25);
    expect(localStorage.getItem("tasks.mode")).toBe("notes");
    expect(screen.getByRole("treeitem", { name: "Analysis.md" })).toBeTruthy();
  });

  it("lists notes first in notes mode and offers the mode commands", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: /Zu Notes wechseln/ }));
    await screen.findByRole("treeitem", { name: "Uni" });
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByRole("combobox", { name: "Suchen" });
    await user.keyboard("grenzwert");
    const notesGroup = await screen.findByRole("group", { name: "Notizen" }, { timeout: 4000 });
    const tasksGroup = screen.getByRole("group", { name: "Tasks" });
    expect(notesGroup.compareDocumentPosition(tasksGroup) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.clear(screen.getByRole("combobox", { name: "Suchen" }));
    await user.keyboard("zu tasks");
    expect(await screen.findByRole("option", { name: /Zu Tasks wechseln/ })).toBeTruthy();
    expect(screen.queryByRole("option", { name: /Zu Notes wechseln/ })).toBeNull();
  });
});
