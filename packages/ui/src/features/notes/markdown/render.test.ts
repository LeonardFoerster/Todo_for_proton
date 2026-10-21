import { describe, expect, it } from "vitest";
import { renderMarkdown, safeUrl, toggleTaskInSource } from "./render";

/** Gerendertes HTML ohne `data-line` (für lesbare Erwartungen). */
function html(md: string): string {
  const div = document.createElement("div");
  div.append(renderMarkdown(md));
  for (const node of div.querySelectorAll("[data-line]")) node.removeAttribute("data-line");
  return div.innerHTML;
}

describe("blocks", () => {
  it.each([
    ["# Titel", "<h1>Titel</h1>"],
    ["### Drei ###", "<h3>Drei</h3>"],
    ["#Kein", "<p>#Kein</p>"],
    ["Absatz eins\nzwei\n\nAbsatz drei", "<p>Absatz eins\nzwei</p><p>Absatz drei</p>"],
    ["hart  \numbruch", "<p>hart<br>umbruch</p>"],
    ["mit\\\numbruch", "<p>mit<br>umbruch</p>"],
    ["---", "<hr>"],
    ["* * *", "<hr>"],
    ["> Zitat\n> weiter", "<blockquote><p>Zitat\nweiter</p></blockquote>"],
    ["```js\nlet a = 1 < 2;\n```", '<pre><code data-lang="js">let a = 1 &lt; 2;</code></pre>'],
    ["    eingerückt\n    code", "<pre><code>eingerückt\ncode</code></pre>"],
    ["Setext\n===", "<p>Setext\n===</p>"],
  ])("%j", (md, expected) => {
    expect(html(md)).toBe(expected);
  });

  it("renders tight, loose, ordered and nested lists", () => {
    expect(html("- a\n- b")).toBe("<ul><li>a</li><li>b</li></ul>");
    expect(html("3. drei\n4. vier")).toBe('<ol start="3"><li>drei</li><li>vier</li></ol>');
    expect(html("- a\n\n- b")).toBe('<ul class="loose"><li><p>a</p></li><li><p>b</p></li></ul>');
    expect(html("- a\n  - a1\n  - a2\n- b")).toBe("<ul><li>a<ul><li>a1</li><li>a2</li></ul></li><li>b</li></ul>");
    expect(html("1. eins\n   - unter")).toBe("<ol><li>eins<ul><li>unter</li></ul></li></ol>");
  });

  it("renders task lists with checkboxes that know their source line", () => {
    const div = document.createElement("div");
    div.append(renderMarkdown("Text\n\n- [ ] offen\n- [x] erledigt"));
    const boxes = [...div.querySelectorAll<HTMLInputElement>("input[type=checkbox]")];
    expect(boxes.map((b) => [b.checked, b.dataset.line])).toEqual([
      [false, "2"],
      [true, "3"],
    ]);
    expect(div.querySelector("li.task")?.textContent).toBe("offen");
  });

  it("renders tables with alignment", () => {
    expect(html("| A | B |\n|:--|--:|\n| 1 | 2 |\n| x \\| y |")).toBe(
      '<table><thead><tr><th style="text-align: left;">A</th><th style="text-align: right;">B</th></tr></thead>' +
        '<tbody><tr><td style="text-align: left;">1</td><td style="text-align: right;">2</td></tr>' +
        '<tr><td style="text-align: left;">x | y</td><td style="text-align: right;"></td></tr></tbody></table>',
    );
  });

  it("marks blocks with their first source line", () => {
    const div = document.createElement("div");
    div.append(renderMarkdown("# A\n\nText\n\n- x"));
    expect([...div.children].map((c) => `${c.tagName}:${(c as HTMLElement).dataset.line}`)).toEqual([
      "H1:0",
      "P:2",
      "UL:4",
    ]);
  });
});

describe("inline", () => {
  it.each([
    ["**fett** und *kursiv* und _auch_", "<p><strong>fett</strong> und <em>kursiv</em> und <em>auch</em></p>"],
    ["__fett__", "<p><strong>fett</strong></p>"],
    ["snake_case_name", "<p>snake_case_name</p>"],
    ["2 * 3 * 4", "<p>2 * 3 * 4</p>"],
    ["~~weg~~", "<p><del>weg</del></p>"],
    ["`a < b`", "<p><code>a &lt; b</code></p>"],
    ["`` a`b ``", "<p><code>a`b</code></p>"],
    ["\\*kein\\*", "<p>*kein*</p>"],
    ["![Bild](https://x/y.png)", "<p>![Bild](https://x/y.png)</p>"],
  ])("%j", (md, expected) => {
    expect(html(md)).toBe(expected);
  });

  it("links only http, https and mailto", () => {
    expect(html("[Proton](https://proton.me)")).toBe(
      '<p><a href="https://proton.me" rel="noopener noreferrer" data-external="">Proton</a></p>',
    );
    expect(html("<mailto:a@b.de>")).toBe(
      '<p><a href="mailto:a@b.de" rel="noopener noreferrer" data-external="">mailto:a@b.de</a></p>',
    );
    expect(html("siehe https://example.org/a.")).toBe(
      '<p>siehe <a href="https://example.org/a" rel="noopener noreferrer" data-external="">https://example.org/a</a>.</p>',
    );
    expect(html("www.example.org")).toBe(
      '<p><a href="https://www.example.org" rel="noopener noreferrer" data-external="">www.example.org</a></p>',
    );
    expect(html("[**fett**](https://x.de)")).toContain("<a");
    expect(html("[rel](notes/a.md)")).toBe("<p>[rel](notes/a.md)</p>");
  });
});

describe("security", () => {
  it.each([
    "<script>alert(1)</script>",
    '<img src=x onerror="alert(1)">',
    "[x](javascript:alert(1))",
    "[x](JAVASCRIPT:alert(1))",
    "[x](data:text/html,<script>alert(1)</script>)",
    "<javascript:alert(1)>",
    '[x](https://a" onmouseover="alert(1))',
    "<a href=\"javascript:alert(1)\">x</a>",
    "| <b onclick=x> | b |\n|--|--|\n| <svg onload=alert(1)> | c |",
    "```\n</code><script>alert(1)</script>\n```",
  ])("renders %j without active content", (md) => {
    const div = document.createElement("div");
    div.append(renderMarkdown(md));
    expect(div.querySelector("script, img, svg, iframe, b, [onerror], [onload], [onclick], [onmouseover]")).toBeNull();
    for (const a of div.querySelectorAll("a")) {
      expect(a.getAttribute("href")).toMatch(/^(https?:|mailto:)/i);
    }
  });

  it("validates link targets", () => {
    expect(safeUrl("https://ok")).toBe("https://ok");
    expect(safeUrl("ftp://x")).toBeNull();
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl("https://a b")).toBeNull();
  });
});

describe("toggleTaskInSource", () => {
  it("toggles exactly the box of that line", () => {
    const src = "# T\n- [ ] a\n  1. [x] b\n> - [ ] c\n- d";
    const apply = (line: number) => {
      const e = toggleTaskInSource(src, line);
      return e && src.slice(0, e.from) + e.insert + src.slice(e.to);
    };
    expect(apply(1)).toBe("# T\n- [x] a\n  1. [x] b\n> - [ ] c\n- d");
    expect(apply(2)).toBe("# T\n- [ ] a\n  1. [ ] b\n> - [ ] c\n- d");
    expect(apply(3)).toBe("# T\n- [ ] a\n  1. [x] b\n> - [x] c\n- d");
    expect(apply(4)).toBeNull();
    expect(apply(9)).toBeNull();
  });
});

describe("budget", () => {
  it("renders 20 KB in a few milliseconds", () => {
    const chunk = "## Abschnitt\n\nText mit **fett**, `code` und [Link](https://x.de).\n\n- [ ] Punkt\n- Punkt 2\n\n";
    const md = chunk.repeat(Math.ceil(20_000 / chunk.length));
    renderMarkdown(md); // aufwärmen
    const runs = Array.from({ length: 10 }, () => {
      const t = performance.now();
      renderMarkdown(md);
      return performance.now() - t;
    }).sort((a, b) => a - b);
    // jsdom ist deutlich langsamer als WebKit; Ziel dort < 8 ms (Messung im Selbsttest).
    expect(runs[5]).toBeLessThan(80);
  });
});
