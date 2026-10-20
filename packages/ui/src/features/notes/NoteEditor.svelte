<script lang="ts">
  /**
   * Editor einer Notiz (`notes.md` §6): natives `<textarea>`, Autosave 400 ms nach der letzten
   * Eingabe, je Notiz ein Speichern nach dem anderen auf der zuletzt gelieferten Basis.
   */
  import type { NoteDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import type { NotesState } from "./notes-state.svelte";

  interface Props {
    notes: NotesState;
    id: string;
  }

  let { notes, id }: Props = $props();

  const SAVE_DELAY_MS = 400;
  let note = $state.raw<NoteDto | null>(null);
  let missing = $state(false);
  let textarea = $state<HTMLTextAreaElement>();
  /** Stand im Core (zuletzt gespeichert bzw. geladen). */
  let saved = "";
  let baseRev: string | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let saving: Promise<void> | null = null;

  const readonly = $derived(!!note && notes.isTrashed(note));

  onMount(() => {
    void load();
    const hidden = () => {
      if (document.visibilityState === "hidden") void flush();
    };
    document.addEventListener("visibilitychange", hidden);
    return () => {
      document.removeEventListener("visibilitychange", hidden);
      void flush();
    };
  });

  async function load() {
    try {
      const loaded = await notes.client.notes.get(id);
      note = loaded;
      saved = loaded.body;
      baseRev = loaded.bodyRev;
      if (textarea) textarea.value = loaded.body;
    } catch {
      missing = true;
    }
  }

  // Bitte um Fokus (Enter im Baum, neue Notiz).
  $effect(() => {
    if (notes.editorFocus && textarea && note) textarea.focus();
  });

  function input() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void flush(), SAVE_DELAY_MS);
    if (notes.fresh === id && textarea?.value) notes.fresh = null;
  }

  /** Speichert, falls der Puffer vom Core-Stand abweicht; wartet auf ein laufendes Speichern. */
  export async function flush(): Promise<void> {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    while (saving) await saving;
    const text = textarea?.value ?? saved;
    if (!note || readonly || text === saved) return;
    saving = (async () => {
      try {
        const result = await notes.client.notes.updateBody(id, text, baseRev);
        saved = result.body;
        baseRev = result.bodyRev;
        note = result;
      } catch (e) {
        toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
      } finally {
        saving = null;
      }
    })();
    await saving;
  }
</script>

{#if missing}
  <div class="empty"><p>Diese Notiz gibt es nicht mehr.</p></div>
{:else}
  <div class="editor" class:mono={note?.kind === "text"}>
    <textarea
      bind:this={textarea}
      aria-label={note ? `Inhalt von ${note.name}` : "Inhalt"}
      spellcheck="false"
      readonly={readonly}
      oninput={input}
      onblur={() => void flush()}
    ></textarea>
  </div>
{/if}

<style>
  .editor {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  textarea {
    flex: 1;
    width: 100%;
    min-height: 0;
    padding: var(--space-8) max(var(--space-6), calc((100% - 72ch) / 2));
    border: 0;
    outline: none;
    resize: none;
    background: var(--color-bg);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--font-size-editor, 15px);
    line-height: 1.6;
  }
  .mono textarea {
    font-family: var(--font-mono);
  }
  .empty {
    display: grid;
    place-items: center;
    flex: 1;
    color: var(--color-text-muted);
  }
</style>
