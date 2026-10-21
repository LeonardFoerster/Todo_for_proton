import { FakeCoreClient } from "@tasks/core-client";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
  for (const t of ["A", "B", "C", "D"]) await client.createTask({ title: t });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^A/ });
  return { client, user: userEvent.setup() };
}

const shown = () => within(screen.getByRole("listbox")).queryAllByRole("option").map((r) => r.getAttribute("aria-label"));
const stored = async (client: FakeCoreClient) =>
  (await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 50 })).items.map((t) => t.title);

describe("sorting", () => {
  it("moves the selected task with Alt+↑/↓ and persists the order", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("option", { name: "B" }));
    screen.getByRole("option", { name: "B" }).focus();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    await waitFor(() => expect(shown()).toEqual(["A", "C", "B", "D"]));
    await user.keyboard("{Alt>}{ArrowUp}{ArrowUp}{/Alt}");
    await waitFor(async () => expect(await stored(client)).toEqual(["B", "A", "C", "D"]));
    expect(shown()).toEqual(["B", "A", "C", "D"]);
  });

  it("drags a row by its grip to a new position", async () => {
    const { client } = await setup();
    // jsdom hat kein Layout: Liste 0–720 px hoch, sonst greift das Auto-Scrollen am Rand.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(
      { top: 0, bottom: 720, left: 0, right: 800, height: 720, width: 800, x: 0, y: 0 } as DOMRect,
    );
    const grip = screen.getByRole("option", { name: "A" }).querySelector(".grip")!;
    // jsdom: Liste beginnt bei y = 0, Zeilen sind 36 px hoch → y = 110 ist Einfügestelle 3 (vor D).
    await fireEvent.pointerDown(grip, { button: 0, clientY: 18 });
    await fireEvent.pointerMove(window, { clientY: 110 });
    await new Promise((r) => requestAnimationFrame(r));
    await fireEvent.pointerUp(window, { clientY: 110 });
    await waitFor(async () => expect(await stored(client)).toEqual(["B", "C", "A", "D"]));
  });
});

describe("deleting", () => {
  it("deletes with Entf and restores at the same place via undo", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("option", { name: "C" }));
    screen.getByRole("option", { name: "C" }).focus();
    await user.keyboard("{Delete}");
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "D"]));
    expect(shown()).toEqual(["A", "B", "D"]);
    const toast = await screen.findByRole("status");
    expect(toast.textContent).toContain("„C“ in den Papierkorb gelegt");
    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C", "D"]));
    await waitFor(() => expect(shown()).toEqual(["A", "B", "C", "D"]));
  });

  it("offers delete in the context menu", async () => {
    const { client, user } = await setup();
    await fireEvent.contextMenu(screen.getByRole("option", { name: "A" }), { clientX: 50, clientY: 20 });
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: /Nach oben/ })).toHaveProperty("disabled", true);
    await user.click(within(menu).getByRole("menuitem", { name: /Löschen/ }));
    await waitFor(async () => expect(await stored(client)).toEqual(["B", "C", "D"]));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("deletes from the detail panel", async () => {
    const { client, user } = await setup();
    await user.click(screen.getByRole("option", { name: "D" }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Löschen" }));
    await waitFor(async () => expect(await stored(client)).toEqual(["A", "B", "C"]));
    expect(screen.queryByRole("region", { name: "Task-Details" })).toBeNull();
  });
});
