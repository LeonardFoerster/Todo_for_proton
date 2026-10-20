<script lang="ts">
  /** Papierkorb im Hauptbereich (`notes.md` §5.3): oberste Elemente, Wiederherstellen, Löschen. */
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { FileCode, FileText, Folder, RotateCcw, Trash } from "../../icons";
  import { restore } from "./notes-actions";
  import type { NotesState } from "./notes-state.svelte";
  import { ago, itemRef } from "./tree-model";
  import type { NoteTrashEntryDto } from "@tasks/core-client";

  interface Props {
    notes: NotesState;
    /** Notiz im Papierkorb nur lesen. */
    onread: (id: string) => void;
  }

  let { notes, onread }: Props = $props();
  let confirm = $state<{ entry: NoteTrashEntryDto | null } | null>(null);

  async function purge(entry: NoteTrashEntryDto | null) {
    confirm = null;
    try {
      const n = entry ? await notes.client.notes.purge([itemRef(entry.item)]) : await notes.client.notes.emptyTrash();
      await notes.reload();
      toasts.show({ message: n === 1 ? "1 Element endgültig gelöscht" : `${n} Elemente endgültig gelöscht` });
    } catch (e) {
      toasts.show({ message: `Nicht gelöscht: ${(e as Error).message}`, tone: "danger" });
    }
  }
</script>

<section class="trash" aria-labelledby="trash-title">
  <header>
    <h1 id="trash-title">Papierkorb</h1>
    <span class="hint">Wird nach 30 Tagen automatisch geleert.</span>
    <Button variant="danger" disabled={notes.trash.length === 0} onclick={() => (confirm = { entry: null })}>
      Papierkorb leeren
    </Button>
  </header>
  {#if notes.trash.length === 0}
    <div class="empty">
      <Trash size={32} strokeWidth={1.5} aria-hidden="true" />
      <p>Der Papierkorb ist leer.</p>
    </div>
  {:else}
    <ul class="entries">
      {#each notes.trash as entry (`${entry.item.type}:${entry.item.id}`)}
        <li class="entry">
          <span class="icon" aria-hidden="true">
            {#if entry.item.type === "folder"}<Folder size={16} strokeWidth={1.75} />{:else if entry.item.kind === "markdown"}<FileCode
                size={16}
                strokeWidth={1.75}
              />{:else}<FileText size={16} strokeWidth={1.75} />{/if}
          </span>
          {#if entry.item.type === "note"}
            {@const id = entry.item.id}
            <button type="button" class="name link" onclick={() => onread(id)}>{entry.item.name}</button>
          {:else}
            <span class="name">{entry.item.name}</span>
          {/if}
          <span class="meta">
            {entry.path.length > 0 ? entry.path.join(" › ") : "Oberste Ebene"} · gelöscht {ago(entry.trashedAt)}
          </span>
          <IconButton label="Wiederherstellen" icon={RotateCcw} onclick={() => void restore(notes, [itemRef(entry.item)])} />
          <IconButton label="Endgültig löschen" icon={Trash} onclick={() => (confirm = { entry })} />
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if confirm}
  {@const entry = confirm.entry}
  <Dialog title={entry ? "Endgültig löschen?" : "Papierkorb leeren?"} onclose={() => (confirm = null)} width={420}>
    <p>
      {#if entry}
        „{entry.item.name}“{entry.item.type === "folder" ? " und sein Inhalt werden" : " wird"} endgültig gelöscht – auch auf
        deinen anderen Geräten. Das lässt sich nicht rückgängig machen.
      {:else}
        Alle {notes.trash.length} Elemente im Papierkorb werden endgültig gelöscht – auch auf deinen anderen Geräten. Das lässt
        sich nicht rückgängig machen.
      {/if}
    </p>
    <div class="actions">
      <Button variant="secondary" onclick={() => (confirm = null)}>Abbrechen</Button>
      <Button variant="danger" onclick={() => void purge(entry)}>Endgültig löschen</Button>
    </div>
  </Dialog>
{/if}

<style>
  .trash {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 56px;
    padding: 0 var(--space-4) 0 var(--space-6);
    flex: none;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  .hint {
    flex: 1;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .entries {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--row-height);
    padding: 0 var(--space-4) 0 var(--space-6);
    border-bottom: 1px solid var(--color-border);
  }
  .icon {
    display: grid;
    color: var(--color-text-muted);
  }
  .name {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--color-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .meta {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .empty {
    display: grid;
    place-items: center;
    gap: var(--space-2);
    margin-top: 15vh;
    color: var(--color-text-muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }
</style>
