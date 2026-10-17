import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App.svelte";

// jsdom kennt matchMedia/rAF nicht vollständig.
beforeEach(() => {
  window.matchMedia ??= ((query: string) => ({
    matches: false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  location.hash = "";
  localStorage.clear();
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const work = await client.createTaskList("Arbeit");
  render(App, { props: { client } });
  await screen.findByRole("link", { name: /Arbeit/ });
  return { client, work, user: userEvent.setup() };
}

const heading = () => screen.getByRole("heading", { level: 1 });

describe("App navigation", () => {
  it("starts in the inbox and switches views via the sidebar", async () => {
    const { user } = await setup();
    expect(heading()).toHaveProperty("textContent", "Eingang");
    await user.click(screen.getByRole("link", { name: /Heute/ }));
    await waitFor(() => expect(heading().textContent).toBe("Heute"));
    expect(location.hash).toBe("#/today");
    expect(screen.getByRole("link", { name: /Heute/ }).getAttribute("aria-current")).toBe("page");
    await user.click(screen.getByRole("link", { name: /Arbeit/ }));
    await waitFor(() => expect(heading().textContent).toBe("Arbeit"));
  });

  it("creates a list inline and opens it", async () => {
    const { user, client } = await setup();
    await user.click(screen.getByRole("button", { name: "Neue Liste" }));
    await user.keyboard("Privat{Enter}");
    await waitFor(() => expect(heading().textContent).toBe("Privat"));
    expect((await client.listTaskLists()).map((l) => l.name)).toContain("Privat");
  });

  it("falls back to the inbox for an unknown list", async () => {
    location.hash = "#/list/unknown";
    await setup();
    await waitFor(() => expect(heading().textContent).toBe("Eingang"));
  });

  it("collapses the sidebar to icons", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: "Seitenleiste einklappen" }));
    expect(screen.queryByText("Listen")).toBeNull();
    expect(screen.getByRole("link", { name: "Arbeit" })).toBeTruthy();
    expect(localStorage.getItem("tasks.sidebar-collapsed")).toBe("1");
  });
});
