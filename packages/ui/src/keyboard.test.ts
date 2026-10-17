import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App.svelte";
import { toasts } from "./components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup(titles: string[] = []) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  for (const t of titles) await client.createTask({ title: t });
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  if (titles.length) await screen.findByRole("option", { name: titles[0] });
  return { client, user: userEvent.setup() };
}

const listbox = () => screen.getByRole("listbox");
const selected = () => within(listbox()).queryAllByRole("option").find((o) => o.getAttribute("aria-selected") === "true");
const open = async (client: FakeCoreClient) =>
  (await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 50 })).items.map((t) => t.title);

describe("keyboard", () => {
  it("create → complete → delete without the mouse", async () => {
    const { client, user } = await setup();
    await user.keyboard("n");
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Neue Task" }));
    await user.keyboard("Erste{Enter}Zweite{Enter}");
    await screen.findByRole("option", { name: "Zweite" });
    await user.keyboard("{Escape}"); // Quick-Add verlassen
    expect(document.activeElement).toBe(document.body);

    await user.keyboard("{ArrowDown}");
    expect(selected()?.getAttribute("aria-label")).toBe("Erste");
    expect(document.activeElement).toBe(selected()); // sichtbarer Fokus auf der Zeile
    await user.keyboard(" ");
    await waitFor(async () => expect(await open(client)).toEqual(["Zweite"]));

    await user.keyboard("j");
    expect(selected()?.getAttribute("aria-label")).toBe("Zweite");
    await user.keyboard("{Delete}");
    await waitFor(async () => expect(await open(client)).toEqual([]));
  });

  it("J/K move the selection, Enter opens and Esc closes details", async () => {
    await setup(["A", "B", "C"]);
    const user = userEvent.setup();
    await user.keyboard("jjjk"); // A → B → C → B
    expect(selected()?.getAttribute("aria-label")).toBe("B");
    expect(screen.queryByRole("region", { name: "Task-Details" })).toBeNull();
    await user.keyboard("{Enter}");
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await waitFor(() => expect((within(detail).getByRole("textbox", { name: "Titel" }) as HTMLTextAreaElement).value).toBe("B"));
    // Detail folgt der Auswahl.
    selected()!.focus();
    await user.keyboard("j");
    await waitFor(() => expect((within(detail).getByRole("textbox", { name: "Titel" }) as HTMLTextAreaElement).value).toBe("C"));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Task-Details" })).toBeNull();
    expect(selected()?.getAttribute("aria-label")).toBe("C");
    await user.keyboard("{Escape}");
    expect(selected()).toBeUndefined();
  });

  it("typing in fields never triggers shortcuts", async () => {
    const { client, user } = await setup(["A"]);
    await user.click(screen.getByRole("textbox", { name: "Neue Task" }));
    await user.keyboard("jn e k{Enter}");
    await screen.findByRole("option", { name: "jn e k" });
    expect(await open(client)).toEqual(["A", "jn e k"]);
  });

  it("G then T navigates, ? shows the shortcut overview", async () => {
    const { user } = await setup();
    await user.keyboard("gt");
    await screen.findByRole("heading", { name: "Heute" });
    await user.keyboard("?");
    const dialog = await screen.findByRole("dialog", { name: "Tastaturkürzel" });
    expect(within(dialog).getByText("Erledigt umschalten")).toBeTruthy();
  });
});
