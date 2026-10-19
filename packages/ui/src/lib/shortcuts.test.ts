import { describe, expect, it, vi } from "vitest";
import { comboOf, formatKeys, ShortcutRegistry } from "./shortcuts";

function press(target: EventTarget, key: string, init: KeyboardEventInit = {}) {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
}

describe("ShortcutRegistry", () => {
  it("normalizes combos", () => {
    expect(comboOf(new KeyboardEvent("keydown", { key: "N" }))).toBe("n");
    expect(comboOf(new KeyboardEvent("keydown", { key: "n", ctrlKey: true }))).toBe("ctrl+n");
    expect(comboOf(new KeyboardEvent("keydown", { key: "ArrowUp", altKey: true }))).toBe("alt+arrowup");
    expect(comboOf(new KeyboardEvent("keydown", { key: " " }))).toBe("space");
    expect(comboOf(new KeyboardEvent("keydown", { key: "?", shiftKey: true }))).toBe("?");
    expect(comboOf(new KeyboardEvent("keydown", { key: "T", shiftKey: true }))).toBe("shift+t");
    expect(comboOf(new KeyboardEvent("keydown", { key: "T" }))).toBe("t"); // Feststelltaste
  });

  it("runs matching shortcuts and prevents the default", () => {
    const reg = new ShortcutRegistry();
    const run = vi.fn();
    reg.register({ keys: ["n", "ctrl+n"], description: "Neu", run });
    const off = reg.attach(window);
    expect(press(document.body, "n").defaultPrevented).toBe(true);
    press(document.body, "n", { ctrlKey: true });
    press(document.body, "x");
    expect(run).toHaveBeenCalledTimes(2);
    off();
    press(document.body, "n");
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("ignores text inputs unless allowed", () => {
    const reg = new ShortcutRegistry();
    const plain = vi.fn();
    const esc = vi.fn();
    reg.register({ keys: ["n"], description: "Neu", run: plain }, { keys: ["escape"], description: "Esc", inInputs: true, run: esc });
    const off = reg.attach(window);
    const input = document.createElement("input");
    document.body.append(input);
    press(input, "n");
    press(input, "Escape");
    expect(plain).not.toHaveBeenCalled();
    expect(esc).toHaveBeenCalledOnce();
    input.remove();
    off();
  });

  it("leaves space/enter on buttons native and respects `when`", () => {
    const reg = new ShortcutRegistry();
    const run = vi.fn();
    let active = false;
    reg.register({ keys: ["space"], description: "Erledigt", run }, { keys: ["e"], description: "E", when: () => active, run });
    const off = reg.attach(window);
    const button = document.createElement("button");
    document.body.append(button);
    press(button, " ");
    press(document.body, "e");
    expect(run).not.toHaveBeenCalled();
    active = true;
    press(document.body, "e");
    press(document.body, " ");
    expect(run).toHaveBeenCalledTimes(2);
    button.remove();
    off();
  });

  it("blocks all but modal-safe shortcuts while modal", () => {
    const reg = new ShortcutRegistry();
    const next = vi.fn();
    const close = vi.fn();
    let modal = true;
    reg.setModal(() => modal);
    reg.register({ keys: ["j"], description: "Nächste", run: next }, { keys: ["escape"], description: "Esc", inModal: true, run: close });
    reg.handle(new KeyboardEvent("keydown", { key: "j" }));
    reg.handle(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(next).not.toHaveBeenCalled();
    expect(close).toHaveBeenCalledOnce();
    modal = false;
    reg.handle(new KeyboardEvent("keydown", { key: "j" }));
    expect(next).toHaveBeenCalledOnce();
  });

  it("supports two-key sequences", () => {
    const reg = new ShortcutRegistry();
    const inbox = vi.fn();
    reg.register({ keys: ["g i"], description: "Eingang", run: inbox });
    const e1 = new KeyboardEvent("keydown", { key: "g", cancelable: true });
    expect(reg.handle(e1, 1000)).toBe(true);
    expect(reg.handle(new KeyboardEvent("keydown", { key: "i", cancelable: true }), 1500)).toBe(true);
    expect(inbox).toHaveBeenCalledOnce();
    reg.handle(new KeyboardEvent("keydown", { key: "g" }), 2000);
    reg.handle(new KeyboardEvent("keydown", { key: "i" }), 4000); // zu spät
    expect(inbox).toHaveBeenCalledOnce();
  });

  it("formats keys for display", () => {
    expect(formatKeys("ctrl+n")).toBe("Strg+N");
    expect(formatKeys("alt+arrowup")).toBe("Alt+↑");
    expect(formatKeys("g i")).toBe("G dann I");
  });
});
