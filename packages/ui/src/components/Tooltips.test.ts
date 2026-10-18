import { fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Tooltips from "./Tooltips.svelte";

function rect(left: number, top: number, width: number, height: number): DOMRect {
  return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top } as DOMRect;
}

/** Element mit Tooltip; jsdom hat kein Layout, also feste Maße. */
function anchor(text: string, side: string | undefined, box: DOMRect): HTMLButtonElement {
  const button = document.createElement("button");
  button.dataset.tooltip = text;
  if (side) button.dataset.tooltipSide = side;
  button.getBoundingClientRect = () => box;
  document.body.append(button);
  return button;
}

const tip = () => screen.queryByRole("tooltip", { hidden: true });

async function hover(el: Element) {
  await fireEvent.pointerOver(el);
  vi.advanceTimersByTime(450);
  await tick();
  await tick();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(rect(0, 0, 60, 20));
  render(Tooltips);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("Tooltips", () => {
  it("appears after the delay, fixed next to the element on the requested side", async () => {
    const el = anchor("Eingang", "right", rect(8, 100, 40, 34));
    await fireEvent.pointerOver(el);
    vi.advanceTimersByTime(400);
    await tick();
    expect(tip(), "noch in der Verzögerung").toBeNull();
    vi.advanceTimersByTime(50);
    await tick();
    await tick();
    const t = tip()!;
    expect(t.textContent?.trim()).toBe("Eingang");
    expect(t.getAttribute("aria-hidden")).toBe("true");
    expect(t.classList.contains("placed")).toBe(true);
    // rechts neben dem Element (48 + 8), vertikal mittig (100 + 17 − 10)
    expect([t.style.left, t.style.top]).toEqual(["56px", "107px"]);
  });

  it("places bottom (default), top and left", async () => {
    const cases: [string | undefined, [string, string]][] = [
      [undefined, ["170px", "226px"]], // mittig darunter: 200 − 30, 200 + 20 + 6
      ["top", ["170px", "174px"]], // darüber: 200 − 6 − 20
      ["left", ["132px", "200px"]], // links: 200 − 8 − 60
    ];
    for (const [side, expected] of cases) {
      const el = anchor(`Tipp ${side}`, side, rect(200, 200, 0, 20));
      await hover(el);
      expect([tip()!.style.left, tip()!.style.top], side ?? "bottom").toEqual(expected);
      await fireEvent.pointerOut(el);
    }
  });

  it("stays inside the window at its edges", async () => {
    const el = anchor("Ganz rechts", "right", rect(innerWidth - 20, innerHeight - 10, 20, 10));
    await hover(el);
    const t = tip()!;
    expect(parseFloat(t.style.left)).toBe(innerWidth - 60 - 8);
    expect(parseFloat(t.style.top)).toBe(innerHeight - 20 - 8);
  });

  it("hides on leaving, clicking and Escape; nothing without data-tooltip", async () => {
    const el = anchor("Tipp", undefined, rect(10, 10, 20, 20));
    await hover(el);
    expect(tip()).not.toBeNull();
    await fireEvent.pointerOut(el);
    expect(tip()).toBeNull();

    await hover(el);
    await fireEvent.pointerDown(el);
    expect(tip()).toBeNull();

    await hover(el);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(tip()).toBeNull();

    const plain = document.createElement("span");
    document.body.append(plain);
    await hover(plain);
    expect(tip()).toBeNull();
  });

  it("leaving before the delay cancels it", async () => {
    const el = anchor("Tipp", undefined, rect(10, 10, 20, 20));
    await fireEvent.pointerOver(el);
    await fireEvent.pointerOut(el);
    vi.advanceTimersByTime(1000);
    await tick();
    expect(tip()).toBeNull();
  });
});
