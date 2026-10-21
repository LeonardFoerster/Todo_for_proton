import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

/** App im Modus „Notes" mit einer geöffneten Notiz. */
async function open(body: string, kind: "markdown" | "text" = "markdown") {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const created = await client.notes.create(null, kind);
  const note = await client.notes.updateBody(created.id, body, null);
  localStorage.setItem("tasks.mode", "notes");
  localStorage.setItem("notes.open", JSON.stringify(note.id));
  render(App, { props: { client } });
  const editor = (await screen.findByRole("textbox", { name: /^Inhalt von / })) as HTMLTextAreaElement;
  await waitFor(() => expect(editor.value).toBe(body));
  return { client, note, editor, user: userEvent.setup() };
}

describe("NoteEditor", () => {
  it("autosaves 400 ms after typing on the last revision, one save at a time", async () => {
    const { client, note, editor, user } = await open("# Blatt");
    const spy = vi.spyOn(client.notes, "updateBody");
    await user.click(editor);
    editor.setSelectionRange(7, 7);
    await user.keyboard(" 3");
    expect(screen.getByText(/Speichert…/)).toBeTruthy();
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(spy).toHaveBeenLastCalledWith(note.id, "# Blatt 3", note.bodyRev);
    const first = await spy.mock.results[0].value;
    await user.keyboard("!");
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2), { timeout: 2000 });
    expect(spy).toHaveBeenLastCalledWith(note.id, "# Blatt 3!", first.bodyRev);
    await waitFor(() => expect(screen.getByText(/Gespeichert/)).toBeTruthy());
    expect(screen.getByRole("button", { name: "Blatt 3!.md" })).toBeTruthy();
  });

  it("saves immediately on blur and keeps the merged body from the core", async () => {
    const { client, editor, user } = await open("eins\nzwei");
    const original = client.notes.updateBody;
    vi.spyOn(client.notes, "updateBody").mockImplementation(async (id, text, base) => {
      const saved = await original(id, text, base);
      // Wie nach einem Remote-Merge: der Core liefert mehr als gesendet.
      return { ...saved, body: `${text}\ndrei (von B)` };
    });
    await user.click(editor);
    editor.setSelectionRange(4, 4);
    await user.keyboard("!");
    editor.blur();
    await waitFor(() => expect(editor.value).toBe("eins!\nzwei\ndrei (von B)"));
    expect(editor.selectionStart).toBe(5);
  });

  it("takes a remote revision into a clean buffer without losing the caret", async () => {
    const { client, note, editor, user } = await open("Zeile 1\nZeile 2");
    await user.click(editor);
    editor.setSelectionRange(3, 3);
    client.notes.simulateRemoteBody(note.id, "Zeile 1\nZeile 2\nZeile 3 von B");
    await waitFor(() => expect(editor.value).toBe("Zeile 1\nZeile 2\nZeile 3 von B"));
    expect(editor.selectionStart).toBe(3);
  });

  it("saves pending input when a remote revision arrives", async () => {
    const { client, note, editor, user } = await open("a");
    const spy = vi.spyOn(client.notes, "updateBody");
    await user.click(editor);
    editor.setSelectionRange(1, 1);
    await user.keyboard("b");
    client.notes.simulateRemoteBody(note.id, "a von B");
    await waitFor(() => expect(spy).toHaveBeenCalled(), { timeout: 300 });
    expect(spy.mock.calls[0][1]).toBe("ab");
  });

  it("continues lists, indents and formats", async () => {
    const { editor, user } = await open("- Milch");
    await user.click(editor);
    editor.setSelectionRange(7, 7);
    await user.keyboard("{Enter}Brot{Enter}{Enter}");
    expect(editor.value).toBe("- Milch\n- Brot\n");
    await user.keyboard("Wort");
    editor.setSelectionRange(15, 19);
    await user.keyboard("{Control>}b{/Control}");
    expect(editor.value).toBe("- Milch\n- Brot\n**Wort**");
    editor.setSelectionRange(8, 8);
    await user.keyboard("{Tab}");
    expect(editor.value).toBe("- Milch\n  - Brot\n**Wort**");
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(editor.value).toBe("- Milch\n- Brot\n**Wort**");
    editor.setSelectionRange(0, 0);
    await user.keyboard("{Control>}{Shift>}x{/Shift}{/Control}");
    expect(editor.value.split("\n")[0]).toBe("- [ ] Milch");
  });

  it("uses tabs and no Markdown helpers in .txt notes", async () => {
    const { editor, user } = await open("Einkauf", "text");
    expect(editor.closest(".mono")).toBeTruthy();
    await user.click(editor);
    editor.setSelectionRange(7, 7);
    await user.keyboard("{Tab}");
    expect(editor.value).toBe("Einkauf\t");
    editor.setSelectionRange(0, 7);
    await user.keyboard("{Control>}b{/Control}");
    expect(editor.value).toBe("Einkauf\t");
  });

  it("zen hides the sidebar until Esc", async () => {
    const { editor, user } = await open("Text");
    await user.click(editor);
    await user.keyboard("{Control>}{Shift>}f{/Shift}{/Control}");
    expect(document.querySelector(".app.zen")).toBeTruthy();
    expect(localStorage.getItem("notes.zen")).toBe("1");
    await user.keyboard("{Escape}");
    expect(document.querySelector(".app.zen")).toBeNull();
    expect(document.activeElement).toBe(editor);
  });

  it("creates new notes with the default kind from the settings", async () => {
    localStorage.setItem("notes.default-kind", "text");
    const { client, user, editor } = await open("x");
    editor.blur();
    await user.keyboard("n");
    await screen.findByRole("textbox", { name: "Inhalt von Unbenannt" });
    const items = (await client.notes.listItems(null, { limit: 10 })).items;
    expect(items.map((i) => (i.type === "note" ? i.kind : "folder")).sort()).toEqual(["markdown", "text"]);
  });
});
