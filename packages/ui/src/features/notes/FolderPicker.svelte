<script lang="ts">
  /** „Verschieben nach…" (`M`, Kontextmenü): Ordner mit Suche, Wurzel zuerst (`notes.md` §5.3). */
  import { onMount } from "svelte";
  import Dialog from "../../components/Dialog.svelte";
  import { Folder, Inbox } from "../../icons";
  import type { NotesState } from "./notes-state.svelte";

  interface Props {
    notes: NotesState;
    /** Was verschoben wird (Name im Titel). */
    subject: string;
    /** Ein Ordner kann nicht in sich selbst oder einen Unterordner. */
    movingFolder?: string;
    /** Aktueller Ort (deaktiviert). */
    currentFolder: string | null;
    onpick: (folderId: string | null) => void;
    onclose: () => void;
  }

  let { notes, subject, movingFolder, currentFolder, onpick, onclose }: Props = $props();
  let query = $state("");
  let index = $state(0);
  let search = $state<HTMLInputElement>();

  // Nach dem Dialog (der beim Öffnen sich selbst fokussiert) ins Suchfeld.
  onMount(() => search?.focus());

  /** Ordner außerhalb des Papierkorbs (auch über Vorfahren) mit Pfad. */
  const options = $derived.by(() => {
    const trashed = (id: string | null): boolean => {
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- lokale Schleifenbremse
      const seen = new Set<string>();
      let f = notes.folder(id);
      while (f && !seen.has(f.id)) {
        if (f.trashedAt) return true;
        seen.add(f.id);
        f = notes.folder(f.parentId);
      }
      return false;
    };
    const inside = (id: string): boolean => {
      if (!movingFolder) return false;
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- lokale Schleifenbremse
      const seen = new Set<string>();
      let f = notes.folder(id);
      while (f && !seen.has(f.id)) {
        if (f.id === movingFolder) return true;
        seen.add(f.id);
        f = notes.folder(f.parentId);
      }
      return false;
    };
    const all = [
      { id: null as string | null, label: "Oberste Ebene", path: "" },
      ...notes.folders
        .filter((f) => !trashed(f.id) && !inside(f.id))
        .map((f) => ({ id: f.id as string | null, label: f.name, path: notes.path(f.parentId).join(" › ") }))
        .sort((a, b) => `${a.path} › ${a.label}`.localeCompare(`${b.path} › ${b.label}`, "de")),
    ];
    const q = query.trim().toLowerCase();
    return q ? all.filter((o) => `${o.path} ${o.label}`.toLowerCase().includes(q)) : all;
  });

  $effect(() => {
    if (index >= options.length) index = Math.max(0, options.length - 1);
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      index = Math.min(options.length - 1, index + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      index = Math.max(0, index - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const option = options[index];
      if (option && option.id !== currentFolder) pick(option.id);
    }
  }

  function pick(id: string | null) {
    onpick(id);
    onclose();
  }
</script>

<Dialog title="{subject} verschieben nach…" {onclose} width={420}>
  <input
    bind:this={search}
    class="search"
    type="search"
    placeholder="Ordner suchen"
    aria-label="Ordner suchen"
    aria-controls="folder-options"
    aria-activedescendant={options[index] ? `folder-option-${index}` : undefined}
    role="combobox"
    aria-expanded="true"
    bind:value={query}
    onkeydown={keydown}
  />
  <div class="options" id="folder-options" role="listbox" aria-label="Ordner">
    {#each options as option, i (option.id ?? "")}
      <button
        type="button"
        id="folder-option-{i}"
        class="option"
        class:active={i === index}
        role="option"
        aria-selected={i === index}
        disabled={option.id === currentFolder}
        onclick={() => pick(option.id)}
        onpointermove={() => (index = i)}
      >
        {#if option.id === null}<Inbox size={16} strokeWidth={1.75} aria-hidden="true" />{:else}<Folder
            size={16}
            strokeWidth={1.75}
            aria-hidden="true"
          />{/if}
        <span class="label">{option.label}</span>
        {#if option.path}<span class="path">{option.path}</span>{/if}
      </button>
    {:else}
      <p class="empty">Kein Ordner gefunden.</p>
    {/each}
  </div>
</Dialog>

<style>
  .search {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  .search:focus {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -1px;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 320px;
    margin-top: var(--space-2);
    overflow-y: auto;
  }
  .option {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option.active {
    background: var(--color-bg-hover);
  }
  .option:disabled {
    color: var(--color-text-disabled);
    cursor: default;
  }
  .label {
    flex: none;
    max-width: 50%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .empty {
    color: var(--color-text-muted);
  }
</style>
