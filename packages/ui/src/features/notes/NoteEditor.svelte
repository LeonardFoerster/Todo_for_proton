<script lang="ts">
  /**
   * Editor einer Notiz (`notes.md` §6): natives `<textarea>` (keine Arbeit je Taste außer dem
   * Neustart zweier Timer), Autosave 400 ms nach der letzten Eingabe, je Notiz ein Speichern
   * nach dem anderen auf der zuletzt gelieferten Basis. Remote-Änderungen kommen in den Puffer,
   * ohne den Cursor zu verlieren.
   */
  import type { NoteDto, SyncStatusDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { limitWidth, readNoteView, writeNoteView } from "../../lib/note-preferences";
  import type { Shortcut, ShortcutRegistry } from "../../lib/shortcuts";
  import {
    applyEdit,
    continueList,
    heading,
    indent,
    link,
    linkFromPaste,
    outdent,
    rebase,
    replacePreservingCursor,
    stats,
    toggleCheckbox,
    wrap,
    type Edit,
    type TextState,
  } from "./editor-commands";
  import NoteStatusBar from "./NoteStatusBar.svelte";
  import MarkdownPreview from "./markdown/MarkdownPreview.svelte";
  import { toggleTaskInSource } from "./markdown/render";
  import type { NotesState } from "./notes-state.svelte";

  interface Props {
    notes: NotesState;
    id: string;
    shortcuts: ShortcutRegistry;
    sync: () => SyncStatusDto;
  }

  let { notes, id, shortcuts, sync }: Props = $props();

  const SAVE_DELAY_MS = 400;
  const STATS_DELAY_MS = 300;
  /** Ab hier warnt die Statuszeile (Grenze 1 MiB, `notes.md` §3.4). */
  const LARGE_BYTES = 256 * 1024;

  let note = $state.raw<NoteDto | null>(null);
  let missing = $state(false);
  let textarea = $state<HTMLTextAreaElement>();
  let wide = $state(!limitWidth());
  /** Eingaben seit dem letzten Speichern (Statuszeile „Speichert…"). */
  let dirty = $state(false);
  let busy = $state(false);
  let counts = $state({ words: 0, chars: 0, selected: 0, bytes: 0 });
  /** Vorschau statt Quelltext (nur `.md`, `Strg+E`, je Notiz gemerkt). */
  let preview = $state(false);
  /** Text für die Vorschau (beim Umschalten und bei Änderungen gesetzt, nie je Taste). */
  let previewSource = $state("");
  let previewer = $state<MarkdownPreview>();

  /** Stand im Core (zuletzt gespeichert bzw. geladen) und die Basis fürs nächste Speichern. */
  let saved = "";
  let baseRev: string | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let statsTimer: ReturnType<typeof setTimeout> | null = null;
  let saving: Promise<void> | null = null;

  const readonly = $derived(!!note && notes.isTrashed(note));
  const markdown = $derived(note?.kind === "markdown");

  onMount(() => {
    void load();
    const hidden = () => {
      if (document.visibilityState === "hidden") void flush();
    };
    const prefs = () => (wide = !limitWidth());
    document.addEventListener("visibilitychange", hidden);
    addEventListener("notes-preferences", prefs);
    const offChange = notes.client.onChange((e) => {
      if (e.notes.includes(id)) void reconcile();
    });
    const offShortcuts = shortcuts.register(...editorShortcuts());
    return () => {
      document.removeEventListener("visibilitychange", hidden);
      removeEventListener("notes-preferences", prefs);
      offChange();
      offShortcuts();
      rememberView();
      if (statsTimer) clearTimeout(statsTimer);
      void flush();
    };
  });

  async function load() {
    try {
      const loaded = await notes.client.notes.get(id);
      note = loaded;
      saved = loaded.body;
      baseRev = loaded.bodyRev;
      if (textarea) {
        textarea.value = loaded.body;
        const view = readNoteView(id);
        const jump = notes.select?.id === id ? notes.select : null;
        notes.select = null;
        if (jump) {
          // Suchtreffer: auswählen und in die Mitte holen (Quelltext, nicht Vorschau).
          const end = Math.min(jump.end, loaded.body.length);
          textarea.setSelectionRange(Math.min(jump.start, end), end);
          scrollToCaret();
        } else {
          const end = Math.min(view.end ?? 0, loaded.body.length);
          textarea.setSelectionRange(Math.min(view.start ?? 0, end), end);
          textarea.scrollTop = view.scroll ?? 0;
          if (view.preview && loaded.kind === "markdown") void showPreview(true);
        }
      }
      updateStats();
    } catch {
      missing = true;
    }
  }

  /** Cursorzeile ungefähr mittig zeigen (Zeilenanteil; `<textarea>` hat keine Zeilenmaße). */
  function scrollToCaret() {
    if (!textarea) return;
    const lines = textarea.value.split("\n").length;
    const line = textarea.value.slice(0, textarea.selectionStart).split("\n").length - 1;
    textarea.scrollTop = Math.max(0, (line / Math.max(1, lines)) * textarea.scrollHeight - textarea.clientHeight / 2);
  }

  function rememberView() {
    if (!textarea || !note) return;
    writeNoteView(id, {
      preview,
      start: textarea.selectionStart,
      end: textarea.selectionEnd,
      scroll: textarea.scrollTop,
    });
  }

  // Bitte um Fokus (Enter im Baum, neue Notiz): aus der Vorschau zurück in den Quelltext.
  $effect(() => {
    if (notes.editorFocus && textarea && note && !readonly) {
      if (preview) showPreview(false);
      textarea.focus();
    }
  });

  // Sprung zum Suchtreffer, wenn die Notiz schon offen war (sonst erledigt das `load`).
  $effect(() => {
    const jump = notes.select;
    if (!jump || jump.id !== id || !textarea || !note) return;
    notes.select = null;
    if (preview) void showPreview(false);
    textarea.setSelectionRange(jump.start, Math.min(jump.end, textarea.value.length));
    scrollToCaret();
  });

  // `.txt` hat keine Vorschau (Typ gewechselt).
  $effect(() => {
    if (preview && note && note.kind !== "markdown") showPreview(false);
  });

  /** Umschalten Bearbeiten ↔ Vorschau mit Scroll-Abgleich über Quellzeilen. */
  async function showPreview(on: boolean) {
    if (!textarea || on === preview) return;
    if (on) {
      const caretLine = textarea.value.slice(0, textarea.selectionStart).split("\n").length - 1;
      previewSource = textarea.value;
      preview = true;
      await tick();
      previewer?.scrollToLine(caretLine);
      previewer?.focus();
    } else {
      const line = previewer?.topLine() ?? 0;
      preview = false;
      await tick();
      const lines = textarea.value.split("\n");
      const ratio = lines.length > 1 ? line / lines.length : 0;
      textarea.scrollTop = ratio * textarea.scrollHeight;
      if (!readonly) textarea.focus({ preventScroll: true });
    }
  }

  /** Checkbox in der Vorschau: genau `[ ]` ↔ `[x]` dieser Zeile, gespeichert wie eine Eingabe. */
  function toggleTask(line: number) {
    if (!textarea || readonly) return;
    const edit = toggleTaskInSource(textarea.value, line);
    if (!edit) return;
    textarea.setRangeText(edit.insert, edit.from, edit.to, "preserve");
    previewSource = textarea.value;
    input();
  }

  function openLink(url: string) {
    if (notes.client.openExternal) void notes.client.openExternal(url).catch(() => undefined);
    else window.open(url, "_blank", "noopener");
  }

  function input() {
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void flush(), SAVE_DELAY_MS);
    scheduleStats();
    if (notes.fresh === id && textarea?.value) notes.fresh = null;
  }

  function scheduleStats() {
    if (statsTimer) clearTimeout(statsTimer);
    statsTimer = setTimeout(updateStats, STATS_DELAY_MS);
  }

  function updateStats() {
    statsTimer = null;
    const text = textarea?.value ?? "";
    const s = stats(text);
    const selected = textarea ? Math.abs(textarea.selectionEnd - textarea.selectionStart) : 0;
    // Bytes nur grob (Warnung ab 256 KB): UTF-16-Länge reicht als untere Schranke nicht – zählen.
    counts = { ...s, selected, bytes: new TextEncoder().encode(text).length };
  }

  /** Speichert, falls der Puffer vom Core-Stand abweicht; wartet auf ein laufendes Speichern. */
  export async function flush(): Promise<void> {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    while (saving) await saving;
    const text = textarea?.value ?? saved;
    if (!note || readonly || text === saved) {
      dirty = false;
      return;
    }
    const base = baseRev;
    busy = true;
    saving = (async () => {
      try {
        const result = await notes.client.notes.updateBody(id, text, base);
        note = result;
        applyResult(text, base, result);
      } catch (e) {
        toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
      } finally {
        saving = null;
        busy = false;
      }
    })();
    await saving;
    dirty = !!saveTimer || (textarea?.value ?? saved) !== saved;
  }

  /**
   * Antwort des Speicherns übernehmen. Weicht der Inhalt ab (Remote-Revision gemergt), kommt er
   * in den Puffer; wurde inzwischen weitergetippt, wird die eigene Änderung übertragen.
   */
  function applyResult(sent: string, base: string | null, result: NoteDto) {
    if (!textarea || result.body === sent) {
      saved = result.body;
      baseRev = result.bodyRev;
      return;
    }
    const typed = textarea.value;
    if (typed === sent) {
      replacePreservingCursor(textarea, result.body);
      saved = result.body;
      baseRev = result.bodyRev;
      if (preview) previewSource = result.body;
      scheduleStats();
      return;
    }
    const rebased = rebase(sent, typed, result.body);
    if (rebased !== null) {
      replacePreservingCursor(textarea, rebased);
      saved = result.body;
      baseRev = result.bodyRev;
    } else {
      // Überschneidung: von der alten Basis aus erneut speichern – der Core mergt bzw. legt im
      // schlimmsten Fall eine Konfliktkopie an; verloren geht nichts.
      saved = sent;
      baseRev = base;
    }
    input();
  }

  /** `onChange` für diese Notiz: Remote-Revision übernehmen bzw. mit eigenen Eingaben mergen. */
  async function reconcile() {
    while (saving) await saving;
    let current: NoteDto;
    try {
      current = await notes.client.notes.get(id);
    } catch {
      missing = true;
      return;
    }
    const remote = current.bodyRev !== baseRev;
    note = current;
    if (!remote || !textarea) return;
    if (textarea.value === saved) {
      replacePreservingCursor(textarea, current.body);
      saved = current.body;
      baseRev = current.bodyRev;
      if (preview) previewSource = current.body;
      scheduleStats();
    } else {
      // Ungespeicherte Eingaben: mit der alten Basis speichern, der Core mergt (§6.3).
      await flush();
    }
  }

  // --- Editor-Hilfen (§6.4) -------------------------------------------------------------------

  const focused = () => !!textarea && document.activeElement === textarea && !readonly;
  const focusedMd = () => focused() && markdown;

  function textState(): TextState {
    const el = textarea!;
    return { value: el.value, start: el.selectionStart, end: el.selectionEnd };
  }

  function run(edit: Edit | null) {
    if (!edit || !textarea) return;
    applyEdit(textarea, edit);
  }

  function editorShortcuts(): Shortcut[] {
    const kind = () => note?.kind ?? "markdown";
    const entry = (
      keys: string[],
      description: string,
      when: () => boolean,
      exec: () => Edit | null,
      label?: string,
    ): Shortcut => ({ keys, label, description, group: "Editor", scope: "notes", inInputs: true, when, run: () => run(exec()) });
    return [
      entry(["enter"], "Liste fortsetzen (leerer Punkt beendet sie)", () => focused() && !!continueList(textState()), () => continueList(textState())),
      entry(["tab"], "Einrücken (Auswahl zeilenweise)", focused, () => indent(textState(), kind())),
      entry(["shift+tab"], "Ausrücken", focused, () => outdent(textState(), kind())),
      entry(["ctrl+b"], "Fett (.md)", focusedMd, () => wrap(textState(), "**")),
      entry(["ctrl+i"], "Kursiv (.md)", focusedMd, () => wrap(textState(), "_")),
      entry(["ctrl+shift+s"], "Durchgestrichen (.md)", focusedMd, () => wrap(textState(), "~~")),
      entry(["ctrl+l"], "Link (.md)", focusedMd, () => link(textState())),
      entry(["ctrl+alt+1"], "Überschrift 1 (.md)", focusedMd, () => heading(textState(), 1)),
      entry(["ctrl+alt+2"], "Überschrift 2 (.md)", focusedMd, () => heading(textState(), 2)),
      entry(["ctrl+alt+3"], "Überschrift 3 (.md)", focusedMd, () => heading(textState(), 3)),
      entry(["ctrl+alt+0"], "Absatz statt Überschrift (.md)", focusedMd, () => heading(textState(), 0)),
      entry(["ctrl+shift+x"], "Checkbox umschalten (.md)", focusedMd, () => toggleCheckbox(textState())),
      {
        keys: ["ctrl+e"],
        description: "Bearbeiten ↔ Vorschau (.md)",
        group: "Editor",
        scope: "notes",
        inInputs: true,
        when: () => markdown && notes.openId === id && !notes.showTrash,
        run: () => void showPreview(!preview),
      },
    ];
  }

  function paste(e: ClipboardEvent) {
    if (!markdown || readonly || !textarea) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    const edit = linkFromPaste(textState(), text);
    if (edit) {
      e.preventDefault();
      run(edit);
    }
  }

  /** Für die Vorschau: Text des Puffers (auch ungespeichert). */
  export function text(): string {
    return textarea?.value ?? saved;
  }

  export function focus() {
    textarea?.focus();
  }
</script>

{#if missing}
  <div class="empty"><p>Diese Notiz gibt es nicht mehr.</p></div>
{:else}
  <div class="editor" class:mono={note?.kind === "text"} class:wide>
    {#if preview}
      <MarkdownPreview bind:this={previewer} source={previewSource} {readonly} ontoggle={toggleTask} onlink={openLink} />
    {/if}
    <textarea
      hidden={preview}
      bind:this={textarea}
      aria-label={note ? `Inhalt von ${note.name}` : "Inhalt"}
      spellcheck="false"
      {readonly}
      oninput={input}
      onblur={() => void flush()}
      onpaste={paste}
      onselect={scheduleStats}
      onkeyup={(e) => e.shiftKey && scheduleStats()}
      onpointerup={scheduleStats}
    ></textarea>
  </div>
  {#if note}
    <NoteStatusBar
      {notes}
      {note}
      {readonly}
      {counts}
      large={counts.bytes >= LARGE_BYTES}
      saving={busy || dirty}
      sync={sync()}
    >
      {#snippet extra()}
        {#if markdown}
          <span class="view-switch" role="group" aria-label="Ansicht">
            <button type="button" aria-pressed={!preview} data-tooltip="Bearbeiten   Strg+E" onclick={() => void showPreview(false)}>
              Bearbeiten
            </button>
            <button type="button" aria-pressed={preview} data-tooltip="Vorschau   Strg+E" onclick={() => void showPreview(true)}>
              Vorschau
            </button>
          </span>
        {/if}
      {/snippet}
    </NoteStatusBar>
  {/if}
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
    tab-size: 4;
  }
  .wide textarea,
  .wide :global(.preview) {
    padding-inline: var(--space-6);
  }
  .editor {
    position: relative;
  }
  textarea[hidden] {
    display: none;
  }
  .view-switch {
    display: inline-flex;
    gap: 2px;
    padding: 1px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-hover);
  }
  .view-switch button {
    height: 20px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    cursor: pointer;
  }
  .view-switch button[aria-pressed="true"] {
    background: var(--color-bg);
    color: var(--color-text);
  }
  .view-switch button:focus-visible {
    outline: 2px solid var(--color-focus-outline);
  }
  .mono textarea {
    font-family: var(--font-mono);
  }
  textarea[readonly] {
    color: var(--color-text-muted);
  }
  .empty {
    display: grid;
    place-items: center;
    flex: 1;
    color: var(--color-text-muted);
  }
</style>
