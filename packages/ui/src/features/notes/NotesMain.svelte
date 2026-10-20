<script lang="ts">
  /** Hauptbereich im Modus „Notes" (`notes.md` §5.4): Editor, Papierkorb oder leerer Zustand. */
  import type { SyncStatusDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import Button from "../../components/Button.svelte";
  import type { ShortcutRegistry } from "../../lib/shortcuts";
  import IconButton from "../../components/IconButton.svelte";
  import { FileText, PanelLeft, Plus } from "../../icons";
  import NoteEditor from "./NoteEditor.svelte";
  import type { NotesState } from "./notes-state.svelte";
  import NoteTrash from "./NoteTrash.svelte";

  interface Props {
    notes: NotesState;
    shortcuts: ShortcutRegistry;
    sync: () => SyncStatusDto;
    /** Schmale Fenster: Navigation (Schublade) öffnen. */
    onopennav: () => void;
  }

  let { notes, shortcuts, sync, onopennav }: Props = $props();
  let editor = $state<NoteEditor>();
  /** Zen: Statuszeile nur kurz nach einer Mausbewegung (oder mit Fokus darin). */
  let pointerActive = $state(false);
  let pointerTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => () => {
    if (pointerTimer) clearTimeout(pointerTimer);
  });

  function moved() {
    if (!notes.zen) return;
    pointerActive = true;
    if (pointerTimer) clearTimeout(pointerTimer);
    pointerTimer = setTimeout(() => (pointerActive = false), 2000);
  }

  /** Ausstehende Eingaben sofort speichern (Modus-/Notizwechsel, Schließen). */
  export async function flush() {
    await editor?.flush();
  }
</script>

<div class="notes-main" class:zen={notes.zen} class:pointer-active={pointerActive} onpointermove={moved} role="presentation">
  <span class="nav-button">
    <IconButton label="Navigation öffnen" icon={PanelLeft} onclick={onopennav} />
  </span>
  {#if notes.showTrash}
    <NoteTrash {notes} onread={(id) => notes.open(id)} />
  {:else if notes.openId}
    {#key notes.openId}
      <NoteEditor bind:this={editor} {notes} id={notes.openId} {shortcuts} {sync} />
    {/key}
  {:else}
    <div class="empty">
      <FileText size={32} strokeWidth={1.5} aria-hidden="true" />
      <p class="title">Keine Notiz geöffnet</p>
      <p class="hint">Notiz im Baum wählen oder neu anlegen. <kbd>N</kbd> neue Notiz · <kbd>Strg+1</kbd> zu Tasks</p>
      <Button variant="primary" icon={Plus} onclick={() => void notes.createNote()}>Neue Notiz</Button>
    </div>
  {/if}
</div>

<style>
  .notes-main {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .zen :global(.status) {
    opacity: 0;
  }
  .zen.pointer-active :global(.status),
  .zen :global(.status:focus-within) {
    opacity: 1;
  }
  .nav-button {
    display: none;
    position: absolute;
    top: var(--space-2);
    left: var(--space-2);
    z-index: 2;
  }
  @media (max-width: 699px) {
    .nav-button {
      display: block;
    }
  }
  .empty {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: var(--space-2);
    flex: 1;
    color: var(--color-text-muted);
    text-align: center;
  }
  .title {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-lg);
    font-weight: var(--weight-medium);
  }
  .hint {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
  }
  kbd {
    padding: 0 4px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: inherit;
    font-size: var(--text-xs);
  }
</style>
