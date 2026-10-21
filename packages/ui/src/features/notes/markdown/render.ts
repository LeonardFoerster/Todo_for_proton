/**
 * Eigener, kleiner Markdown-Renderer für die Vorschau (`notes.md` §6.5). Erzeugt **DOM-Knoten**
 * (nie `innerHTML` aus dem Text), daher kann kein HTML aus einer Notiz ausgeführt werden.
 *
 * Umfang: CommonMark-Teilmenge (ATX-Überschriften, Absätze, harte Umbrüche, `**`/`*`/`_`,
 * Inline-Code, Codeblöcke ``` und eingerückt, Zitate, verschachtelte Listen, Trennlinie, Links
 * `[t](u)` und `<u>`) plus GFM (Checklisten, Tabellen mit Ausrichtung, `~~`, Auto-Links).
 * Bewusst nicht: Raw-HTML, Bilder, Fußnoten, Referenz-Links, Setext – sie bleiben als Text.
 *
 * Jeder Block trägt `data-line` (erste Quellzeile, 0-basiert) für den Scroll-Abgleich; jede
 * Checkbox `data-line` ihrer Zeile, damit ein Klick genau dort `[ ]` ↔ `[x]` tauscht.
 */

/** Nur diese Links werden zu `<a>`; alles andere bleibt Text. */
const SAFE_URL = /^(https?:\/\/|mailto:)[^\s]+$/i;

export function safeUrl(url: string): string | null {
  const trimmed = url.trim();
  if (/^www\./i.test(trimmed)) return `https://${trimmed}`;
  return SAFE_URL.test(trimmed) ? trimmed : null;
}

interface Line {
  text: string;
  /** Quellzeile im Gesamttext. */
  no: number;
}

const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const HR = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const ITEM = /^( {0,12})([-*+]|\d{1,9}[.)])([ \t]+(.*)|$)/;
const TASK = /^\[([ xX])\][ \t]+(.*)$/;
const TABLE_DELIM = /^ *\|? *:?-+:? *(\| *:?-+:? *)*\|? *$/;

function blank(text: string): boolean {
  return text.trim() === "";
}

/** Startet die Zeile einen anderen Block (beendet einen Absatz)? */
function interrupts(text: string): boolean {
  return FENCE.test(text) || HEADING.test(text) || HR.test(text) || QUOTE.test(text) || ITEM.test(text);
}

export function renderMarkdown(source: string, doc: Document = document): DocumentFragment {
  const lines = source.split("\n").map((text, no) => ({ text: text.replace(/\t/g, "    "), no }));
  const fragment = doc.createDocumentFragment();
  blocks(lines, fragment, doc);
  return fragment;
}

function el<K extends keyof HTMLElementTagNameMap>(doc: Document, tag: K, line?: number): HTMLElementTagNameMap[K] {
  const node = doc.createElement(tag);
  if (line !== undefined) node.dataset.line = String(line);
  return node;
}

function blocks(lines: Line[], parent: Node, doc: Document) {
  let i = 0;
  while (i < lines.length) {
    const { text, no } = lines[i];
    if (blank(text)) {
      i++;
      continue;
    }
    const fence = FENCE.exec(text);
    if (fence) {
      const marker = fence[1];
      const body: string[] = [];
      i++;
      while (i < lines.length && !new RegExp(`^ {0,3}${marker[0]}{${marker.length},}\\s*$`).test(lines[i].text)) {
        body.push(lines[i].text);
        i++;
      }
      i++; // schließender Zaun (oder Ende)
      const pre = el(doc, "pre", no);
      const code = el(doc, "code");
      code.textContent = body.join("\n");
      const lang = fence[2].trim().split(/\s+/)[0];
      if (lang) code.dataset.lang = lang;
      pre.append(code);
      parent.appendChild(pre);
      continue;
    }
    const heading = HEADING.exec(text);
    if (heading) {
      const h = el(doc, `h${heading[1].length}` as "h1", no);
      inline(heading[2] ?? "", h, doc);
      parent.appendChild(h);
      i++;
      continue;
    }
    if (HR.test(text)) {
      parent.appendChild(el(doc, "hr", no));
      i++;
      continue;
    }
    if (QUOTE.test(text)) {
      const inner: Line[] = [];
      while (i < lines.length && QUOTE.test(lines[i].text)) {
        inner.push({ text: QUOTE.exec(lines[i].text)![1], no: lines[i].no });
        i++;
      }
      const quote = el(doc, "blockquote", no);
      blocks(inner, quote, doc);
      parent.appendChild(quote);
      continue;
    }
    if (ITEM.test(text)) {
      i = list(lines, i, parent, doc);
      continue;
    }
    if (/^ {4,}/.test(text)) {
      const body: string[] = [];
      while (i < lines.length && (/^ {4,}/.test(lines[i].text) || (blank(lines[i].text) && /^ {4,}/.test(lines[i + 1]?.text ?? "")))) {
        body.push(lines[i].text.slice(4));
        i++;
      }
      const pre = el(doc, "pre", no);
      const code = el(doc, "code");
      code.textContent = body.join("\n");
      pre.append(code);
      parent.appendChild(pre);
      continue;
    }
    if (text.includes("|") && i + 1 < lines.length && TABLE_DELIM.test(lines[i + 1].text) && lines[i + 1].text.includes("-")) {
      i = table(lines, i, parent, doc);
      continue;
    }
    // Absatz bis zur Leerzeile oder einem anderen Block.
    const para: Line[] = [lines[i]];
    i++;
    while (i < lines.length && !blank(lines[i].text) && !interrupts(lines[i].text)) {
      para.push(lines[i]);
      i++;
    }
    const p = el(doc, "p", no);
    para.forEach((line, k) => {
      const last = k === para.length - 1;
      const hard = !last && (/ {2,}$/.test(line.text) || /\\$/.test(line.text));
      const content = last ? line.text.trim() : line.text.replace(/(\\| {2,})$/, "").trim();
      inline(content, p, doc);
      if (!last) p.append(hard ? el(doc, "br") : doc.createTextNode("\n"));
    });
    parent.appendChild(p);
  }
}

/** Liste ab `lines[start]` bis zum ersten Element, das nicht mehr dazugehört; liefert den Index danach. */
function list(lines: Line[], start: number, parent: Node, doc: Document): number {
  const first = ITEM.exec(lines[start].text)!;
  const indent = first[1].length;
  const ordered = /\d/.test(first[2]);
  const node = ordered ? el(doc, "ol", lines[start].no) : el(doc, "ul", lines[start].no);
  if (ordered) {
    const n = parseInt(first[2], 10);
    if (n !== 1) node.setAttribute("start", String(n));
  }
  let i = start;
  let loose = false;
  const items: Line[][] = [];
  while (i < lines.length) {
    const m = ITEM.exec(lines[i].text);
    if (!m || m[1].length !== indent || /\d/.test(m[2]) !== ordered) break;
    const content = indent + m[2].length + Math.max(1, Math.min(4, (m[3] ?? " ").length - (m[4] ?? "").length));
    const itemLines: Line[] = [{ text: m[4] ?? "", no: lines[i].no }];
    i++;
    // Fortsetzung: eingerückte Zeilen und Leerzeilen, denen eingerückter Inhalt folgt.
    while (i < lines.length) {
      const t = lines[i].text;
      if (blank(t)) {
        const next = lines[i + 1]?.text ?? "";
        if (!blank(next) && next.length - next.trimStart().length >= content) {
          loose = true;
          itemLines.push({ text: "", no: lines[i].no });
          i++;
          continue;
        }
        break;
      }
      const lead = t.length - t.trimStart().length;
      if (lead >= content) {
        itemLines.push({ text: t.slice(content), no: lines[i].no });
        i++;
        continue;
      }
      // Faule Fortsetzung eines Absatzes (keine neue Struktur).
      if (lead > indent || (!interrupts(t) && itemLines.length > 0 && !blank(itemLines[itemLines.length - 1].text))) {
        if (ITEM.test(t)) break;
        itemLines.push({ text: t.trim(), no: lines[i].no });
        i++;
        continue;
      }
      break;
    }
    items.push(itemLines);
    // Zwischen Punkten eine Leerzeile: locker.
    if (i < lines.length && blank(lines[i].text)) {
      const after = lines[i + 1];
      const nm = after && ITEM.exec(after.text);
      if (nm && nm[1].length === indent && /\d/.test(nm[2]) === ordered) {
        loose = true;
        i++;
      }
    }
  }
  // Erst jetzt steht fest, ob die Liste locker ist (dann Absätze mit <p>).
  for (const itemLines of items) {
    const li = el(doc, "li", itemLines[0].no);
    const task = TASK.exec(itemLines[0].text);
    if (task) {
      li.classList.add("task");
      const box = el(doc, "input", itemLines[0].no);
      box.type = "checkbox";
      box.checked = task[1] !== " ";
      li.append(box);
      itemLines[0] = { text: task[2], no: itemLines[0].no };
    }
    const holder = doc.createDocumentFragment();
    blocks(itemLines, holder, doc);
    if (!loose) {
      for (const child of [...holder.childNodes]) {
        if (child.nodeName === "P") (child as Element).replaceWith(...child.childNodes);
      }
    }
    li.append(holder);
    node.append(li);
  }
  if (loose) node.classList.add("loose");
  parent.appendChild(node);
  return i;
}

function cells(row: string): string[] {
  let t = row.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1);
  const out: string[] = [];
  let cur = "";
  for (let k = 0; k < t.length; k++) {
    if (t[k] === "\\" && t[k + 1] === "|") {
      cur += "|";
      k++;
    } else if (t[k] === "|") {
      out.push(cur.trim());
      cur = "";
    } else cur += t[k];
  }
  out.push(cur.trim());
  return out;
}

function table(lines: Line[], start: number, parent: Node, doc: Document): number {
  const head = cells(lines[start].text);
  const align = cells(lines[start + 1].text).map((c) =>
    c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : c.startsWith(":") ? "left" : "",
  );
  const tableEl = el(doc, "table", lines[start].no);
  const thead = el(doc, "thead");
  const tr = el(doc, "tr");
  head.forEach((c, k) => {
    const th = el(doc, "th");
    if (align[k]) th.style.textAlign = align[k];
    inline(c, th, doc);
    tr.append(th);
  });
  thead.append(tr);
  tableEl.append(thead);
  const tbody = el(doc, "tbody");
  let i = start + 2;
  while (i < lines.length && !blank(lines[i].text) && lines[i].text.includes("|")) {
    const row = el(doc, "tr", lines[i].no);
    const values = cells(lines[i].text);
    head.forEach((_, k) => {
      const td = el(doc, "td");
      if (align[k]) td.style.textAlign = align[k];
      inline(values[k] ?? "", td, doc);
      row.append(td);
    });
    tbody.append(row);
    i++;
  }
  tableEl.append(tbody);
  parent.appendChild(tableEl);
  return i;
}

const PUNCT = /[!"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~]/;
const BARE_URL = /^(?:https?:\/\/|www\.)[^\s<]*[^\s<.,:;"')\]!?*_~]/i;

/** Inline-Inhalt von `text` an `parent` anhängen. */
export function inline(text: string, parent: Node, doc: Document = document) {
  let buf = "";
  const flush = () => {
    if (buf) parent.appendChild(doc.createTextNode(buf));
    buf = "";
  };
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const rest = text.slice(i);
    if (c === "\\" && PUNCT.test(text[i + 1] ?? "")) {
      buf += text[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      const run = /^`+/.exec(rest)![0];
      const close = text.indexOf(run, i + run.length);
      if (close > 0) {
        flush();
        const code = el(doc, "code");
        let inner = text.slice(i + run.length, close);
        if (/^ .* $/.test(inner)) inner = inner.slice(1, -1);
        code.textContent = inner;
        parent.appendChild(code);
        i = close + run.length;
        continue;
      }
      buf += run;
      i += run.length;
      continue;
    }
    if (c === "<") {
      const auto = /^<((?:https?:\/\/|mailto:)[^\s<>]+)>/i.exec(rest);
      if (auto) {
        flush();
        parent.appendChild(anchor(doc, auto[1], auto[1]));
        i += auto[0].length;
        continue;
      }
    }
    if (c === "!" && text[i + 1] === "[") {
      // Bilder sind nicht im Umfang: bleiben als Text.
      const image = /^!\[[^\]]*\]\([^)]*\)/.exec(rest);
      buf += image ? image[0] : "![";
      i += image ? image[0].length : 2;
      continue;
    }
    if (c === "[") {
      const link = parseLink(text, i);
      if (link) {
        const url = safeUrl(link.url);
        if (url) {
          flush();
          const a = anchor(doc, url);
          inline(link.label, a, doc);
          parent.appendChild(a);
        } else {
          buf += text.slice(i, link.end);
        }
        i = link.end;
        continue;
      }
    }
    if (rest.startsWith("~~")) {
      const close = text.indexOf("~~", i + 2);
      if (close > i + 2) {
        flush();
        const del = el(doc, "del");
        inline(text.slice(i + 2, close), del, doc);
        parent.appendChild(del);
        i = close + 2;
        continue;
      }
    }
    if (c === "*" || c === "_") {
      const em = emphasis(text, i);
      if (em) {
        flush();
        const node = el(doc, em.strong ? "strong" : "em");
        inline(em.inner, node, doc);
        parent.appendChild(node);
        i = em.end;
        continue;
      }
    }
    if ((c === "h" || c === "H" || c === "w" || c === "W") && (i === 0 || /[\s(]/.test(text[i - 1]))) {
      const url = BARE_URL.exec(rest);
      const safe = url && safeUrl(url[0]);
      if (url && safe) {
        flush();
        parent.appendChild(anchor(doc, safe, url[0]));
        i += url[0].length;
        continue;
      }
    }
    buf += c;
    i++;
  }
  flush();
}

function anchor(doc: Document, href: string, text?: string): HTMLAnchorElement {
  const a = el(doc, "a");
  a.href = href;
  a.rel = "noopener noreferrer";
  a.dataset.external = "";
  if (text !== undefined) a.textContent = text;
  return a;
}

/** `[label](url "title")` ab `start`; Klammern im Label dürfen verschachtelt sein. */
function parseLink(text: string, start: number): { label: string; url: string; end: number } | null {
  let depth = 0;
  let k = start;
  for (; k < text.length; k++) {
    const ch = text[k];
    if (ch === "\\") {
      k++;
      continue;
    }
    if (ch === "`") {
      const close = text.indexOf("`", k + 1);
      if (close > 0) k = close;
      continue;
    }
    if (ch === "[") depth++;
    else if (ch === "]" && --depth === 0) break;
  }
  if (depth !== 0 || text[k + 1] !== "(") return null;
  const m = /^\(\s*(<[^>]*>|[^\s()]*(?:\([^\s()]*\)[^\s()]*)*)(?:\s+"[^"]*")?\s*\)/.exec(text.slice(k + 1));
  if (!m) return null;
  const url = m[1].startsWith("<") ? m[1].slice(1, -1) : m[1];
  return { label: text.slice(start + 1, k), url, end: k + 1 + m[0].length };
}

/** `*x*`, `_x_`, `**x**`, `__x__` (bzw. `***x***`) – vereinfachte Delimiter-Regeln von CommonMark. */
function emphasis(text: string, start: number): { strong: boolean; inner: string; end: number } | null {
  const ch = text[start];
  const run = new RegExp(`^\\${ch}+`).exec(text.slice(start))![0].length;
  const prev = text[start - 1] ?? " ";
  const next = text[start + run] ?? " ";
  if (/\s/.test(next)) return null;
  // `_` innerhalb eines Worts ist keine Betonung (snake_case).
  if (ch === "_" && /[\p{L}\p{N}]/u.test(prev)) return null;
  for (const size of run >= 2 ? [2, 1] : [1]) {
    const marker = ch.repeat(size);
    let k = start + size;
    while (k < text.length) {
      const close = text.indexOf(marker, k);
      if (close < 0) break;
      const before = text[close - 1];
      const after = text[close + size] ?? " ";
      const okClose = !/\s/.test(before) && close > start + size && (ch !== "_" || !/[\p{L}\p{N}]/u.test(after));
      // Nicht mitten in einem längeren Lauf schließen (`**a*` …).
      if (okClose && !(size === 1 && text[close + 1] === ch && text[close - 1] !== ch)) {
        const inner = text.slice(start + size, close);
        return { strong: size === 2, inner, end: close + size };
      }
      k = close + size;
    }
  }
  return null;
}

/** Zeile `line` im Quelltext: `[ ]` ↔ `[x]` der Checkliste tauschen; `null`, wenn keine. */
export function toggleTaskInSource(source: string, line: number): { from: number; to: number; insert: string } | null {
  let pos = 0;
  for (let n = 0; n < line; n++) {
    const next = source.indexOf("\n", pos);
    if (next < 0) return null;
    pos = next + 1;
  }
  const end = source.indexOf("\n", pos);
  const text = source.slice(pos, end < 0 ? source.length : end);
  const m = /^(\s*(?:>\s?)*\s*(?:[-*+]|\d{1,9}[.)])[ \t]+)\[([ xX])\]/.exec(text);
  if (!m) return null;
  const at = pos + m[1].length + 1;
  return { from: at, to: at + 1, insert: m[2] === " " ? "x" : " " };
}
