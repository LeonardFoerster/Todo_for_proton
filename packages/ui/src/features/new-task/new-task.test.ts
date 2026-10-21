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
    // P6-01: erkannte Stellen sind im Titelfeld markiert (Ebene hinter dem Feld).
    expect([...dialog().querySelectorAll("mark")].map((m) => [m.textContent, m.classList.contains("priority")])).toEqual([
      ["morgen", false],
      ["!1", true],
      ["@uni", false],
    ]);
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
    // P6-02: Bestätigung im Fenster statt Toast.
    expect(within(dialog()).getByRole("status").textContent).toContain("„Blatt 1“ in „Uni“ angelegt");
    expect(toasts.items).toHaveLength(0);
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

  it("is a bare dialog: notes as second line, chips with ×, due popover closes alone on Esc (P6)", async () => {
    const { client, user } = await setup();
    expect(dialog().querySelector("h2")).toBeNull();
    expect(within(dialog()).queryByRole("button", { name: /Notiz/ })).toBeNull();
    await user.type(title(), "Bericht morgen !2");
    await user.click(within(dialog()).getByRole("button", { name: "Priorität entfernen" }));
    expect(within(dialog()).getByRole("button", { name: "Priorität: Keine" })).toBeTruthy();
    // Von Hand übersteuert → die Markierung von „!2“ ist weg, die von „morgen“ bleibt.
    expect([...dialog().querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["morgen"]);
    await user.click(within(dialog()).getByRole("button", { name: /Morgen/ }));
    const popover = await within(dialog()).findByRole("dialog", { name: "Fälligkeit wählen" });
    await waitFor(() => expect(document.activeElement).toBe(within(popover).getByRole("textbox", { name: "Fällig am" })));
    await user.keyboard("{Escape}");
    await waitFor(() => expect(within(dialog()).queryByRole("dialog", { name: "Fälligkeit wählen" })).toBeNull());
    expect(dialog()).toBeTruthy();
    await user.click(within(dialog()).getByRole("button", { name: "Fälligkeit entfernen" }));
    await user.click(within(dialog()).getByRole("textbox", { name: "Notiz" }));
    await user.keyboard("Seite 4{Control>}{Enter}{/Control}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    expect(await all(client)).toMatchObject([{ title: "Bericht", priority: "none", due: null, notes: "Seite 4" }]);
  });

  it("recognizes a rule in the text, shows it and creates a series (N4-05)", async () => {
    const { client, user } = await setup();
    await user.type(title(), "Sport jeden Mo und Do");
    expect([...dialog().querySelectorAll("mark.repeat")].map((m) => m.textContent)).toEqual(["jeden Mo und Do"]);
    expect(within(dialog()).getByRole("button", { name: "Wiederholung: wöchentlich, Mo und Do" })).toBeTruthy();
    expect(within(dialog()).getByText("Wiederholt sich wöchentlich, Mo und Do")).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    const [task] = await all(client);
    expect(task).toMatchObject({ title: "Sport", recurrence: { freq: "weekly", weekdays: ["mo", "th"] } });
    expect(task.due?.kind).toBe("date");
  });

  it("the chip menu and the custom dialog win over the text", async () => {
    const { client, user } = await setup();
    await user.type(title(), "Gießen täglich");
    await user.click(within(dialog()).getByRole("button", { name: /^Wiederholung: täglich/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Wiederholung wählen" })).getByRole("menuitem", { name: "Benutzerdefiniert…" }));
    const custom = await screen.findByRole("dialog", { name: "Wiederholung" });
    const interval = within(custom).getByRole("textbox", { name: "Alle" });
    await user.clear(interval);
    await user.type(interval, "3");
    await user.click(within(custom).getByRole("radio", { name: "Nach Erledigung" }));
    expect(within(custom).getByText("alle 3 Tage nach Erledigung")).toBeTruthy();
    await user.click(within(custom).getByRole("button", { name: "Übernehmen" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Wiederholung" })).toBeNull());
    // Von Hand gesetzt: die Markierung im Text entfällt, „täglich“ bleibt dennoch erkannt aus dem Titel heraus.
    expect(dialog().querySelectorAll("mark.repeat")).toHaveLength(0);
    expect(within(dialog()).getByRole("button", { name: "Wiederholung: alle 3 Tage nach Erledigung" })).toBeTruthy();
    await user.click(within(dialog()).getByRole("button", { name: "Anlegen" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    expect(await all(client)).toMatchObject([
      { title: "Gießen", recurrence: { freq: "daily", interval: 3, anchor: "completion" }, due: todayPlus(0) },
    ]);
  });

  it("Alt+W opens the menu; „Keine“ removes the rule", async () => {
    const { client, user } = await setup();
    await user.type(title(), "Miete monatlich");
    await user.keyboard("{Alt>}w{/Alt}");
    await user.click(within(await screen.findByRole("menu", { name: "Wiederholung wählen" })).getByRole("menuitem", { name: "Keine" }));
    expect(within(dialog()).getByRole("button", { name: "Wiederholung: keine" })).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());
    expect(await all(client)).toMatchObject([{ title: "Miete", recurrence: null, due: null }]);
  });
});
