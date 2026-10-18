import { FakeCoreClient } from "@tasks/core-client";
import { cleanup, render, screen, within } from "@testing-library/svelte";
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
  await client.createTask({ title: "A" });
  const ready = vi.spyOn(client, "ready");
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  return { client, ready, user: userEvent.setup() };
}

const group = () => screen.queryByRole("group", { name: "Darstellung" });
const pressed = () =>
  within(group()!)
    .getAllByRole("button")
    .find((b) => b.getAttribute("aria-pressed") === "true")
    ?.getAttribute("aria-label");

describe("layout switch", () => {
  it("switches by button and V and remembers the choice", async () => {
    const { user } = await setup();
    await screen.findByRole("option", { name: "A" });
    expect(pressed()).toBe("Liste");
    await user.click(within(group()!).getByRole("button", { name: "Kanban" }));
    expect(pressed()).toBe("Kanban");
    expect(screen.getByRole("region", { name: "Kanban-Board" })).toBeTruthy();
    expect(screen.queryByRole("listbox", { name: "Tasks" })).toBeNull();
    expect(localStorage.getItem("tasks.layout")).toBe("board");
    await user.keyboard("v");
    expect(pressed()).toBe("Liste");
    await screen.findByRole("option", { name: "A" });
    await user.keyboard("v");
    expect(pressed()).toBe("Kanban");
  });

  it("starts in the remembered layout and still reports ready", async () => {
    localStorage.setItem("tasks.layout", "board");
    const { ready } = await setup();
    expect(screen.getByRole("region", { name: "Kanban-Board" })).toBeTruthy();
    await vi.waitFor(() => expect(ready).toHaveBeenCalledTimes(1));
    cleanup();
  });

  it("is only offered in inbox, lists and all tasks; selection is disabled on the board", async () => {
    const { user } = await setup();
    await user.click(within(group()!).getByRole("button", { name: "Kanban" }));
    const select = screen.getByRole("button", { name: /^Auswählen/ });
    expect(select.hasAttribute("disabled")).toBe(true);
    for (const [hash, heading] of [
      ["#/today", "Heute"],
      ["#/upcoming", "Geplant"],
      ["#/completed", "Erledigt"],
    ]) {
      location.hash = hash;
      await screen.findByRole("heading", { name: heading });
      expect(group(), heading).toBeNull();
      expect(screen.queryByRole("region", { name: "Kanban-Board" }), heading).toBeNull();
    }
    location.hash = "#/all";
    await screen.findByRole("heading", { name: "Alle Tasks" });
    expect(pressed()).toBe("Kanban");
  });

  it("leaves an active selection when switching to the board", async () => {
    const { user } = await setup();
    await screen.findByRole("option", { name: "A" });
    await user.click(screen.getByRole("button", { name: "Auswählen" }));
    expect(screen.getByRole("toolbar", { name: "Auswahl" })).toBeTruthy();
    await user.keyboard("v");
    expect(screen.queryByRole("toolbar", { name: "Auswahl" })).toBeNull();
    expect(screen.getByRole("region", { name: "Kanban-Board" })).toBeTruthy();
    expect(pressed()).toBe("Kanban");
  });
});
