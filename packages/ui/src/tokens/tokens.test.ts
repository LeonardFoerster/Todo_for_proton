// @vitest-environment node
// Kontrast der Token-Paare in beiden Themes (WCAG 2.x): Text ≥ 4.5:1, Bedienelemente ≥ 3:1.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  const body = css.slice(start, css.indexOf("\n}", start));
  const vars: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) vars[m[1]] = m[2].trim();
  return vars;
}

const light = block(":root");
const dark = { ...light, ...block('[data-theme="dark"]') };

function hex(vars: Record<string, string>, name: string): string {
  let value = vars[name];
  while (value?.startsWith("var(")) value = vars[value.slice(6, -1)];
  if (!/^#[0-9a-f]{6}$/i.test(value ?? "")) throw new Error(`--${name} is not a hex color: ${value}`);
  return value;
}

function luminance(color: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(color.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [l1, l2] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (l1 + 0.05) / (l2 + 0.05);
}

const TEXT: [string, string][] = [
  ["color-text", "color-bg"],
  ["color-text", "color-bg-subtle"],
  ["color-text", "color-bg-selected"],
  ["color-text-muted", "color-bg"],
  ["color-text-muted", "color-bg-subtle"],
  ["color-text-muted", "color-bg-hover"],
  ["color-accent-text", "color-bg"],
  ["color-accent-text", "color-bg-selected"],
  ["color-on-accent", "color-accent-fill"],
  ["color-on-accent", "color-accent-fill-hover"],
  ["color-danger", "color-bg"],
  ["color-danger", "color-bg-subtle"],
  ["color-danger", "color-bg-selected"],
  ["color-danger", "color-bg-hover"],
  ["color-on-accent", "color-danger-fill"],
  ["color-on-accent", "color-danger-fill-hover"],
  ["color-warning", "color-bg"],
  ["color-success", "color-bg"],
  ["color-text-inverse", "color-bg-inverse"],
  ["color-accent-inverse", "color-bg-inverse"],
  ["color-text", "color-bg-raised"],
];

const CONTROLS: [string, string][] = [
  ["color-control-border", "color-bg"],
  ["color-control-border", "color-bg-hover"],
  ["color-accent", "color-bg"],
  ["color-accent-fill", "color-bg"],
  ["color-focus-outline", "color-bg"],
  ["color-focus-outline", "color-bg-subtle"],
  ["color-focus-outline", "color-bg-raised"],
  ["color-focus-outline", "color-bg-hover"],
  ["color-focus-outline", "color-bg-selected"],
  // Cursor-Balken auf der Auswahlfläche (Zeile, Karte – P5-04).
  ["color-accent", "color-bg-selected"],
];

describe.each([
  ["light", light],
  ["dark", dark],
])("%s theme", (_, vars) => {
  it.each(TEXT)("text %s on %s ≥ 4.5:1", (fg, bg) => {
    expect(contrast(hex(vars, fg), hex(vars, bg))).toBeGreaterThanOrEqual(4.5);
  });
  it.each(CONTROLS)("control %s on %s ≥ 3:1", (fg, bg) => {
    expect(contrast(hex(vars, fg), hex(vars, bg))).toBeGreaterThanOrEqual(3);
  });
});
