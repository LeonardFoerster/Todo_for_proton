<script lang="ts">
  /**
   * Eine Tooltip-Schicht für die ganze App: liest `data-tooltip="Text"` und optional
   * `data-tooltip-side="bottom|top|right|left"` an beliebigen Elementen und zeigt den Text fest
   * positioniert über allem. Anders als ein CSS-`::after` schneidet ihn kein Vorfahr mit `overflow`
   * ab (z. B. die eingeklappte Seitenleiste), und er bleibt am Fensterrand im Bild.
   *
   * Erscheint verzögert beim Überfahren und sofort bei Tastaturfokus; verschwindet beim Verlassen,
   * Klicken, Scrollen und mit `Esc`. Im Leerlauf läuft nichts (nur ein Timer während des Wartens).
   * Rein visuell (`aria-hidden`): Die Elemente tragen ihren Namen schon per `aria-label`.
   *
   * Als `popover` gezeigt: so liegt der Tooltip in der obersten Ebene – auch über modalen Dialogen
   * (die sonst alles andere verdecken).
   */
  import { onMount, tick } from "svelte";

  const DELAY_MS = 450;
  /** Abstand zum Element: unter/über bzw. neben dem Element. */
  const GAP = 6;
  const SIDE_GAP = 8;
  /** Mindestabstand zum Fensterrand. */
  const MARGIN = 8;

  let text = $state<string | null>(null);
  let left = $state(0);
  let top = $state(0);
  let placed = $state(false);
  let tip = $state<HTMLDivElement>();
  let anchor: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function anchorOf(node: EventTarget | null): HTMLElement | null {
    return node instanceof Element ? node.closest<HTMLElement>("[data-tooltip]") : null;
  }

  function hide() {
    clearTimeout(timer);
    timer = undefined;
    anchor = null;
    text = null;
    placed = false;
  }

  function schedule(target: HTMLElement, delay: number) {
    hide();
    anchor = target;
    timer = setTimeout(() => void show(target), delay);
  }

  async function show(target: HTMLElement) {
    const label = target.dataset.tooltip;
    if (!label || !target.isConnected || anchor !== target) return;
    text = label;
    placed = false;
    await tick();
    if (tip && anchor === target) {
      showOnTop(tip);
      place(target, tip);
    }
  }

  function showOnTop(el: HTMLElement) {
    try {
      if (!el.matches(":popover-open")) el.showPopover();
    } catch {
      // Ohne Popover-API (ältere Engines, jsdom): fest positioniert genügt außerhalb von Dialogen.
    }
  }

  /** Neben das Element setzen (Seite aus `data-tooltip-side`), dann ins Fenster schieben. */
  function place(target: HTMLElement, el: HTMLElement) {
    const r = target.getBoundingClientRect();
    const { width, height } = el.getBoundingClientRect();
    let x: number;
    let y: number;
    switch (target.dataset.tooltipSide) {
      case "right":
        x = r.right + SIDE_GAP;
        y = r.top + r.height / 2 - height / 2;
        break;
      case "left":
        x = r.left - SIDE_GAP - width;
        y = r.top + r.height / 2 - height / 2;
        break;
      case "top":
        x = r.left + r.width / 2 - width / 2;
        y = r.top - GAP - height;
        break;
      default:
        x = r.left + r.width / 2 - width / 2;
        y = r.bottom + GAP;
    }
    left = Math.max(MARGIN, Math.min(innerWidth - width - MARGIN, x));
    top = Math.max(MARGIN, Math.min(innerHeight - height - MARGIN, y));
    placed = true;
  }

  /** Nur bei Tastaturfokus (`:focus-visible`), nicht beim Klick mit der Maus. */
  function focusVisible(el: Element): boolean {
    try {
      return el.matches(":focus-visible");
    } catch {
      return document.documentElement.dataset.input === "keyboard";
    }
  }

  onMount(() => {
    const over = (e: PointerEvent) => {
      const target = anchorOf(e.target);
      if (target && target !== anchor) schedule(target, DELAY_MS);
      else if (!target && anchor) hide();
    };
    const out = (e: PointerEvent) => {
      if (anchor && !(e.relatedTarget instanceof Node && anchor.contains(e.relatedTarget))) hide();
    };
    const focusIn = (e: FocusEvent) => {
      const target = anchorOf(e.target);
      if (target && e.target instanceof Element && focusVisible(e.target)) schedule(target, 0);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    addEventListener("pointerover", over);
    addEventListener("pointerout", out);
    addEventListener("focusin", focusIn);
    addEventListener("focusout", hide);
    addEventListener("pointerdown", hide, true);
    addEventListener("keydown", key, true);
    addEventListener("scroll", hide, true);
    addEventListener("resize", hide);
    return () => {
      hide();
      removeEventListener("pointerover", over);
      removeEventListener("pointerout", out);
      removeEventListener("focusin", focusIn);
      removeEventListener("focusout", hide);
      removeEventListener("pointerdown", hide, true);
      removeEventListener("keydown", key, true);
      removeEventListener("scroll", hide, true);
      removeEventListener("resize", hide);
    };
  });
</script>

{#if text !== null}
  <div
    bind:this={tip}
    class="tooltip"
    class:placed
    role="tooltip"
    aria-hidden="true"
    popover="manual"
    style:left="{left}px"
    style:top="{top}px"
  >
    {text}
  </div>
{/if}

<style>
  .tooltip {
    position: fixed;
    z-index: 100;
    /* Popover-Vorgaben des Browsers (zentriert, Rahmen, Scrollen) aufheben. */
    inset: auto;
    width: max-content;
    height: auto;
    margin: 0;
    border: 0;
    overflow: hidden;
    max-width: min(320px, calc(100vw - 16px));
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-inverse);
    color: var(--color-text-inverse);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: 18px;
    white-space: pre;
    pointer-events: none;
    visibility: hidden;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease);
  }
  .tooltip.placed {
    visibility: visible;
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .tooltip {
      transition: none;
    }
  }
</style>
