import { describe, expect, it } from "vitest";
import { THEME_INIT_SCRIPT, resolveTheme } from "./theme";

describe("theme", () => {
  it("resolves system against the OS preference", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("init script behaves like resolveTheme", () => {
    for (const stored of [null, "light", "dark", "bogus"]) {
      for (const dark of [true, false]) {
        const html = { dataset: {} as Record<string, string> };
        const run = new Function("localStorage", "matchMedia", "document", THEME_INIT_SCRIPT);
        run({ getItem: () => stored }, () => ({ matches: dark }), { documentElement: html });
        const pref = stored === "light" || stored === "dark" ? stored : "system";
        expect(html.dataset.theme).toBe(resolveTheme(pref, dark));
      }
    }
  });
});
