import { afterEach, describe, expect, it, vi } from "vitest";
import { COMFORTABLE_QUERY, Density, NARROW_QUERY } from "./density.svelte";

const original = window.matchMedia;
afterEach(() => {
  window.matchMedia = original;
});

/** matchMedia-Attrappe: `matches` je Query umschaltbar, `change` wird an die Hörer gemeldet. */
function mockMedia(initial: Record<string, boolean>) {
  const state = { ...initial };
  const listeners: Record<string, (() => void)[]> = {};
  window.matchMedia = vi.fn((query: string) => ({
    get matches() {
      return state[query] ?? false;
    },
    media: query,
    addEventListener: (_: string, cb: () => void) => (listeners[query] ??= []).push(cb),
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  return (query: string, value: boolean) => {
    state[query] = value;
    for (const cb of listeners[query] ?? []) cb();
  };
}

describe("Density", () => {
  it("uses 36 px rows on a desktop and 44 px on touch or narrow windows", () => {
    const set = mockMedia({ [COMFORTABLE_QUERY]: false, [NARROW_QUERY]: false });
    const density = new Density();
    expect(density.rowHeight).toBe(36);
    expect(density.narrow).toBe(false);
    set(COMFORTABLE_QUERY, true);
    expect(density.rowHeight).toBe(44);
    set(NARROW_QUERY, true);
    expect(density.narrow).toBe(true);
    set(COMFORTABLE_QUERY, false);
    expect(density.rowHeight).toBe(36);
  });
});
