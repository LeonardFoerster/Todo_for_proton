<script lang="ts">
  /**
   * `DuePicker` als Popover an einem Anker (P5-08, ausgelagert in P6-03): unter dem Anker, im Fenster gehalten;
   * `Esc` und Klick daneben schließen, `Tab` läuft im Kreis, Tasten erreichen keine globalen Kürzel. Jede Wahl
   * ruft `onpick` – schließen muss der Aufrufer (`onclose`).
   */
  import type { DueDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import { POPOVER, showOnTop } from "../../lib/popover";
  import DuePicker from "./DuePicker.svelte";

  interface Props {
    anchor: HTMLElement;
    value: DueDto | null;
    onpick: (due: DueDto | null) => void;
    /** `refocus`: Fokus zurück auf den Anker (nicht bei Klick daneben). */
    onclose: (refocus: boolean) => void;
  }

  let { anchor, value, onpick, onclose }: Props = $props();
  let popover: HTMLDivElement;
  let pos = $state({ left: 0, top: 0 });

  onMount(() => {
    // Als Popover in der obersten Ebene: auch über einem modalen Dialog (und unabhängig von dessen
    // Einblend-Transform). Ohne Popover-API (jsdom) genügt die feste Position.
    showOnTop(popover);
    // Unter dem Anker; passt es nicht ganz, so weit nach oben wie nötig.
    const a = anchor.getBoundingClientRect();
    const rect = popover.getBoundingClientRect();
    pos = {
      left: Math.max(8, Math.min(a.left, innerWidth - rect.width - 8)),
      top: Math.max(8, Math.min(a.bottom + 4, innerHeight - rect.height - 8)),
    };
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!popover.contains(target) && !anchor.contains(target)) onclose(false);
    };
    addEventListener("pointerdown", outside, true);
    return () => removeEventListener("pointerdown", outside, true);
  });

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      // Auch das `cancel` eines umgebenden modalen Dialogs verhindern: Esc schließt nur das Popover.
      e.preventDefault();
      onclose(true);
    } else if (e.key === "Tab") {
      const items = [...popover.querySelectorAll<HTMLElement>("input, button:not(:disabled):not([tabindex='-1'])")];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (i <= 0 ? items.length - 1 : -1) : i === items.length - 1 ? 0 : -1;
      if (next >= 0) {
        e.preventDefault();
        items[next].focus();
      }
    }
  }
</script>

<div
  bind:this={popover}
  class="popover"
  role="dialog"
  aria-label="Fälligkeit wählen"
  tabindex="-1"
  popover={POPOVER}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  <DuePicker {value} onchange={onpick} autofocus />
</div>

<style>
  .popover {
    position: fixed;
    z-index: 60;
    /* Popover-Vorgaben des Browsers (zentriert, Rahmen, Scrollen) aufheben. */
    inset: auto;
    margin: 0;
    overflow: visible;
    width: 296px;
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-popover);
  }
  .popover:focus {
    outline: none;
  }
</style>
