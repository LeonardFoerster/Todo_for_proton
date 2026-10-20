<script lang="ts">
  /**
   * Vorschau einer `.md`-Notiz (`notes.md` §6.5). Rendert nur beim Öffnen und bei Änderungen;
   * große Notizen zuerst mit Hinweis „Vorschau wird erstellt…". Links öffnen extern, Checklisten
   * sind anklickbar (ändern genau ihre Quellzeile).
   */
  import { renderMarkdown } from "./render";

  interface Props {
    source: string;
    readonly: boolean;
    ontoggle: (line: number) => void;
    onlink: (url: string) => void;
  }

  let { source, readonly, ontoggle, onlink }: Props = $props();

  /** Ab dieser Größe erst den Hinweis zeigen, dann rendern (Budget 1 MiB < 150 ms). */
  const LARGE = 200_000;
  let container = $state<HTMLDivElement>();
  let pending = $state(false);

  $effect(() => {
    const text = source;
    const el = container;
    if (!el) return;
    const render = () => {
      el.replaceChildren(renderMarkdown(text));
      for (const box of el.querySelectorAll<HTMLInputElement>("input[type=checkbox]")) box.disabled = readonly;
      pending = false;
    };
    if (text.length > LARGE) {
      pending = true;
      const frame = requestAnimationFrame(() => setTimeout(render));
      return () => cancelAnimationFrame(frame);
    }
    render();
  });

  function click(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const box = target.closest<HTMLInputElement>("input[type=checkbox]");
    if (box) {
      e.preventDefault();
      if (!readonly && box.dataset.line) ontoggle(Number(box.dataset.line));
      return;
    }
    const a = target.closest<HTMLAnchorElement>("a[data-external]");
    if (a) {
      // Nie in der Oberfläche navigieren – immer der Standardbrowser.
      e.preventDefault();
      onlink(a.href);
    }
  }

  /** Erstes Element ab Zeile `line` an den Anfang scrollen (Abgleich mit dem Quelltext). */
  export function scrollToLine(line: number) {
    if (!container) return;
    const blocks = [...container.querySelectorAll<HTMLElement>("[data-line]")];
    let best: HTMLElement | undefined;
    for (const b of blocks) {
      if (Number(b.dataset.line) <= line) best = b;
      else break;
    }
    container.scrollTop = best ? best.offsetTop - container.offsetTop : 0;
  }

  export function focus() {
    container?.focus({ preventScroll: true });
  }

  /** Quellzeile des obersten sichtbaren Blocks. */
  export function topLine(): number {
    if (!container) return 0;
    const top = container.scrollTop + container.offsetTop;
    for (const b of container.querySelectorAll<HTMLElement>("[data-line]")) {
      if (b.offsetTop + b.offsetHeight > top) return Number(b.dataset.line);
    }
    return 0;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="preview" bind:this={container} onclick={click} tabindex="-1" aria-label="Vorschau" aria-busy={pending}></div>
{#if pending}
  <p class="pending" role="status">Vorschau wird erstellt…</p>
{/if}

<style>
  .preview {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-8) max(var(--space-6), calc((100% - 72ch) / 2));
    color: var(--color-text);
    font-size: var(--font-size-editor, 15px);
    line-height: 1.6;
    outline: none;
    overflow-wrap: anywhere;
  }
  .pending {
    position: absolute;
    top: var(--space-4);
    left: 50%;
    margin: 0;
    color: var(--color-text-muted);
    transform: translateX(-50%);
  }
  .preview :global(:is(h1, h2, h3, h4, h5, h6)) {
    margin: 1.2em 0 0.4em;
    line-height: 1.3;
  }
  .preview :global(h1) {
    font-size: 1.6em;
  }
  .preview :global(h2) {
    font-size: 1.3em;
  }
  .preview :global(h3) {
    font-size: 1.1em;
  }
  .preview :global(> :first-child) {
    margin-top: 0;
  }
  .preview :global(p) {
    margin: 0 0 0.8em;
  }
  .preview :global(a) {
    color: var(--color-accent-text);
  }
  .preview :global(code) {
    padding: 0.1em 0.3em;
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    font-family: var(--font-mono);
    font-size: 0.9em;
  }
  .preview :global(pre) {
    margin: 0 0 0.8em;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    overflow-x: auto;
  }
  .preview :global(pre code) {
    padding: 0;
    background: none;
  }
  .preview :global(blockquote) {
    margin: 0 0 0.8em;
    padding-left: var(--space-3);
    border-left: 3px solid var(--color-border-strong);
    color: var(--color-text-muted);
  }
  .preview :global(:is(ul, ol)) {
    margin: 0 0 0.8em;
    padding-left: 1.6em;
  }
  .preview :global(li.task) {
    list-style: none;
    margin-left: -1.4em;
  }
  .preview :global(li.task > input) {
    margin: 0 0.5em 0 0;
    vertical-align: -0.1em;
    accent-color: var(--color-accent);
  }
  .preview :global(hr) {
    margin: 1.2em 0;
    border: 0;
    border-top: 1px solid var(--color-border);
  }
  .preview :global(table) {
    margin: 0 0 0.8em;
    border-collapse: collapse;
  }
  .preview :global(:is(th, td)) {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--color-border);
  }
  .preview :global(th) {
    background: var(--color-bg-subtle);
    font-weight: var(--weight-semibold);
  }
</style>
