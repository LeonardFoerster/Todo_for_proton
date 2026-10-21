import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../../App.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
});

async function open(body: string) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const created = await client.notes.create(null, "markdown");
  const note = await client.notes.updateBody(created.id, body, null);
  localStorage.setItem("tasks.mode", "notes");
  localStorage.setItem("notes.open", JSON.stringify(note.id));
  render(App, { props: { client } });
  const editor = (await screen.findByRole("textbox", { name: /^Inhalt von / })) as HTMLTextAreaElement;
  await waitFor(() => expect(editor.value).toBe(body));
  return { client, note, editor, user: userEvent.setup() };
}

describe("Markdown preview", () => {
  it("toggles with Ctrl+E and the status bar, and is remembered per note", async () => {
    const { editor, user } = await open("# Blatt 3\n\nText mit **fett**");
    await user.click(editor);
    await user.keyboard("{Control>}e{/Control}");
    const preview = await screen.findByLabelText("Vorschau");
    expect(preview.querySelector("h1")?.textContent).toBe("Blatt 3");
    expect(preview.querySelector("strong")?.textContent).toBe("fett");
    expect(editor.hidden).toBe(true);
    expect(screen.getByRole("button", { name: "Vorschau" }).getAttribute("aria-pressed")).toBe("true");
    await user.click(screen.getByRole("button", { name: "Bearbeiten" }));
    expect(editor.hidden).toBe(false);
    await user.keyboard("{Control>}e{/Control}");
    expect(JSON.parse(localStorage.getItem("notes.view") ?? "[]")).toEqual([]); // erst beim Verlassen gemerkt
  });

  it("checks a task in the preview and saves like an edit", async () => {
    const { client, note, editor, user } = await open("- [ ] Milch\n- [ ] Brot");
    const spy = vi.spyOn(client.notes, "updateBody");
    await user.click(editor);
    await user.keyboard("{Control>}e{/Control}");
    const boxes = await screen.findAllByRole("checkbox");
    await user.click(boxes[1]);
    expect(editor.value).toBe("- [ ] Milch\n- [x] Brot");
    await waitFor(() => expect(spy).toHaveBeenCalledWith(note.id, "- [ ] Milch\n- [x] Brot", expect.anything()), {
      timeout: 2000,
    });
    expect((await screen.findAllByRole("checkbox"))[1]).toHaveProperty("checked", true);
  });

  it("opens links outside the app", async () => {
    const { editor, user } = await open("[Seite](https://example.org/a)");
    const windowOpen = vi.spyOn(window, "open").mockReturnValue(null);
    await user.click(editor);
    await user.keyboard("{Control>}e{/Control}");
    await user.click(await screen.findByRole("link", { name: "Seite" }));
    expect(windowOpen).toHaveBeenCalledWith("https://example.org/a", "_blank", "noopener");
  });

  it("stays on after a focus request (new note)", async () => {
    const client = new FakeCoreClient();
    await client.ensureInbox();
    render(App, { props: { client } });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /Zu Notes wechseln/ }));
    await user.click((await screen.findAllByRole("button", { name: "Neue Notiz" }))[0]);
    const editor = await screen.findByRole("textbox", { name: /^Inhalt/ });
    await waitFor(() => expect(document.activeElement).toBe(editor));
    await user.keyboard("# Titel");
    await user.keyboard("{Control>}e{/Control}");
    await waitFor(() => expect(document.querySelector(".preview h1")?.textContent).toBe("Titel"));
    await new Promise((r) => setTimeout(r, 50));
    expect(document.querySelector(".preview h1")).toBeTruthy();
  });
});
