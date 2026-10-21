<script lang="ts">
  import type { SyncStatusDto, TaskListDto } from "@tasks/core-client";
  import type { Snippet } from "svelte";
  import logo from "../../assets/logo.png";
  import Button from "../../components/Button.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import { ArrowLeftRight, Hash, NotebookPen, PanelLeft, Plus, Search, Tag, Trash } from "../../icons";
  import { MODE_LABELS, type Mode } from "../../lib/app-mode.svelte";
  import { formatCount, type ListsState } from "../../lib/lists.svelte";
  import { routeHash, type Route } from "../../lib/router.svelte";
  import { SMART_VIEWS, TRASH_VIEW } from "../../lib/views";
  import NewListInput from "./NewListInput.svelte";
  import SidebarItem from "./SidebarItem.svelte";
  import SyncFooter from "./SyncFooter.svelte";

  interface Props {
    route: Route;
    lists: ListsState;
    collapsed: boolean;
    status: SyncStatusDto;
    account: string | null;
    onnewtask: () => void;
    oncreatelist: (name: string) => Promise<void>;
    ontogglecollapse: () => void;
    onnavigate?: () => void;
    onlogin?: () => void;
    onsync?: () => void;
    onsettings?: () => void;
    /** Suche und Befehle (Palette, P5-05) – sichtbarer Einstieg zu `Strg+K`. */
    onsearch?: () => void;
    /** Liste, die gerade inline umbenannt wird. */
    renamingId?: string | null;
    /** Kontextmenü einer Benutzerliste an Position (Viewport). */
    onlistmenu?: (list: TaskListDto, x: number, y: number) => void;
    onrename?: (list: TaskListDto, name: string) => Promise<void>;
    onrenamecancel?: () => void;
    /** Benutzerliste an Einfügeposition `to` (Index unter den Benutzerlisten) schieben. */
    onmovelist?: (id: string, to: number) => void;
    /** Modus (N5): Der Name neben dem Logo schaltet um. */
    mode?: Mode;
    onswitchmode?: () => void;
    /** Inhalt im Modus „Notes" (Baum); Kopf und Fußzeile bleiben stehen. */
    notesBody?: Snippet;
  }

  let {
    route,
    lists,
    collapsed,
    status,
    account,
    onnewtask,
    oncreatelist,
    ontogglecollapse,
    onnavigate,
    onlogin,
    onsync,
    onsettings,
    onsearch,
    renamingId = null,
    onlistmenu,
    onrename,
    onrenamecancel,
    onmovelist,
    mode = "tasks",
    onswitchmode,
    notesBody,
  }: Props = $props();

  const other = $derived(mode === "tasks" ? "notes" : "tasks");
  const switchLabel = $derived(`Zu ${MODE_LABELS[other]} wechseln (Strg+${other === "tasks" ? 1 : 2})`);

  let addingList = $state(false);
  let listsEl = $state<HTMLUListElement>();
  /** Ziehen einer Liste: ID, Startindex, Einfügeposition, Linie (px im `<ul>`). */
  let drag = $state<{ id: string; from: number; to: number; line: number } | null>(null);

  function menuAt(list: TaskListDto, e: MouseEvent) {
    e.preventDefault();
    onlistmenu?.(list, e.clientX, e.clientY);
  }

  /** Einfügeposition aus der Zeiger-Höhe: vor der ersten Zeile, deren Mitte darunter liegt. */
  function dropIndex(y: number): { to: number; line: number } {
    const items = [...(listsEl?.querySelectorAll<HTMLElement>(":scope > li[data-index]") ?? [])];
    const top = listsEl?.getBoundingClientRect().top ?? 0;
    for (const [i, li] of items.entries()) {
      const r = li.getBoundingClientRect();
      if (y < r.top + r.height / 2) return { to: i, line: r.top - top };
    }
    const last = items.at(-1)?.getBoundingClientRect();
    return { to: items.length, line: last ? last.bottom - top : 0 };
  }

  function startDrag(e: PointerEvent, list: TaskListDto, from: number) {
    if (e.button !== 0 || !onmovelist) return;
    e.preventDefault();
    e.stopPropagation();
    drag = { id: list.id, from, to: from, line: 0 };
    const move = (ev: PointerEvent) => {
      if (drag) drag = { ...drag, ...dropIndex(ev.clientY) };
    };
    const end = (ev: PointerEvent) => {
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      const done = drag;
      drag = null;
      if (done && ev.type === "pointerup" && done.to !== done.from && done.to !== done.from + 1) {
        onmovelist(done.id, done.to);
      }
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
  }

  function countFor(kind: string): string | undefined {
    if (kind === "inbox") return formatCount(lists.counts.inbox);
    if (kind === "today") return formatCount(lists.counts.today);
    return undefined;
  }
</script>

<nav class="sidebar" class:collapsed aria-label="Navigation">
  <div class="brand">
    {#if !collapsed}
      <img class="mark" src={logo} alt="" width="22" height="22" />
      {#if onswitchmode}
        <button type="button" class="name mode" aria-label={switchLabel} data-tooltip={switchLabel} onclick={onswitchmode}>
          <span>{MODE_LABELS[mode]}</span>
          <ArrowLeftRight size={14} strokeWidth={1.75} aria-hidden="true" />
        </button>
      {:else}
        <span class="name">Tasks</span>
      {/if}
    {:else if onswitchmode}
      <IconButton label={switchLabel} icon={NotebookPen} size={28} tooltipSide="right" onclick={onswitchmode} />
    {/if}
    <IconButton
      label={collapsed ? "Seitenleiste ausklappen" : "Seitenleiste einklappen"}
      icon={PanelLeft}
      size={28}
      tooltipSide="right"
      onclick={ontogglecollapse}
    />
  </div>

  {#if onsearch}
    <div class="search">
      {#if collapsed}
        <IconButton label="Suchen" shortcut="Strg+K" icon={Search} tooltipSide="right" onclick={onsearch} />
      {:else}
        <button type="button" class="search-field" aria-label="Suchen" aria-keyshortcuts="Control+K" onclick={onsearch}>
          <Search size={16} strokeWidth={1.75} aria-hidden="true" />
          <span class="search-text">Suchen</span>
          <kbd>Strg K</kbd>
        </button>
      {/if}
    </div>
  {/if}

  {#if notesBody}
    <!-- Einmal geladen, bleibt der Notes-Baum gemountet (Zustand, notes.md §5.2). -->
    <div class="mode-body" hidden={mode !== "notes"}>{@render notesBody()}</div>
  {/if}
  <div class="tasks-body" hidden={mode === "notes"}>
  <div class="primary">
    {#if collapsed}
      <button type="button" class="new-compact" aria-label="Neue Task" data-tooltip="Neue Task   N" data-tooltip-side="right" onclick={onnewtask}>
        <Plus size={20} strokeWidth={2} aria-hidden="true" />
      </button>
    {:else}
      <Button variant="primary" size="lg" block icon={Plus} onclick={onnewtask} title="Neue Task (N)">Neue Task</Button>
    {/if}
  </div>

  <div class="scroll">
    <ul class="group">
      {#each SMART_VIEWS as view (view.kind)}
        <li>
          <SidebarItem
            href={routeHash({ view: view.kind })}
            icon={view.icon}
            label={view.label}
            count={countFor(view.kind)}
            overdue={view.kind === "today" ? formatCount(lists.counts.overdue) : undefined}
            active={route.view === view.kind}
            {collapsed}
            {onnavigate}
          />
        </li>
      {/each}
    </ul>

    <div class="section">
      {#if !collapsed}
        <span class="section-label" id="lists-label">Listen</span>
        <IconButton label="Neue Liste" icon={Plus} size={28} tooltipSide="left" onclick={() => (addingList = true)} />
      {:else}
        <span class="divider"></span>
      {/if}
    </div>
    <ul class="group lists" aria-labelledby={collapsed ? undefined : "lists-label"} bind:this={listsEl}>
      {#each lists.userLists as list, i (list.id)}
        <li data-index={i}>
          {#if renamingId === list.id && !collapsed}
            <NewListInput
              initial={list.name}
              label="Neuer Name der Liste"
              oncreate={(name) => onrename?.(list, name).catch(() => {})}
              oncancel={() => onrenamecancel?.()}
            />
          {:else}
            <SidebarItem
              href={routeHash({ view: "list", listId: list.id })}
              icon={Hash}
              label={list.name}
              monogram={[...list.name][0]}
              active={route.view === "list" && route.listId === list.id}
              {collapsed}
              {onnavigate}
              listId={list.id}
              oncontextmenu={onlistmenu ? (e) => menuAt(list, e) : undefined}
              ondragstart={onmovelist ? (e) => startDrag(e, list, i) : undefined}
              dragging={drag?.id === list.id}
            />
          {/if}
        </li>
      {/each}
      {#if drag && drag.to !== drag.from && drag.to !== drag.from + 1}
        <li class="drop-line" style:top="{drag.line - 1}px" aria-hidden="true"></li>
      {/if}
      {#if addingList && !collapsed}
        <li>
          <NewListInput
            oncreate={async (name) => {
              await oncreatelist(name);
              addingList = false;
            }}
            oncancel={() => (addingList = false)}
          />
        </li>
      {:else if lists.loaded && lists.userLists.length === 0 && !collapsed}
        <li class="hint">Eigene Listen für Projekte oder Bereiche – mit + anlegen.</li>
      {/if}
    </ul>

    {#if lists.tags.length > 0}
      <!-- Tags (ADR-035): angelegt über „#name“ im Fenster „Neue Task“ oder im Detail. -->
      <div class="section">
        {#if !collapsed}<span class="section-label" id="tags-label">Tags</span>{:else}<span class="divider"></span>{/if}
      </div>
      <ul class="group" aria-labelledby={collapsed ? undefined : "tags-label"}>
        {#each lists.tags as tag (tag.id)}
          <li>
            <SidebarItem
              href={routeHash({ view: "tag", tagId: tag.id })}
              icon={Tag}
              label={tag.name}
              monogram="#"
              active={route.view === "tag" && route.tagId === tag.id}
              {collapsed}
              {onnavigate}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <!-- Papierkorb unten, wie im Modus „Notes“ (ADR-031). -->
  <div class="footer-nav">
    <SidebarItem
      href={routeHash({ view: "trash" })}
      icon={Trash}
      label={TRASH_VIEW.label}
      count={formatCount(lists.counts.trash)}
      active={route.view === "trash"}
      {collapsed}
      {onnavigate}
    />
  </div>
  </div>

  <SyncFooter {status} {account} {collapsed} {onlogin} {onsync} {onsettings} />
</nav>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 52px;
    padding: 0 var(--space-2) 0 var(--space-4);
  }
  .collapsed .brand {
    justify-content: center;
    padding: 0;
  }
  .search {
    padding: 0 var(--space-3) var(--space-2);
  }
  .collapsed .search {
    display: flex;
    justify-content: center;
    padding: 0 0 var(--space-2);
  }
  .search-field {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2) 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    cursor: text;
    transition: border-color var(--duration-fast) var(--ease);
  }
  .search-field:hover {
    border-color: var(--color-border-strong);
  }
  .search-text {
    flex: 1;
    text-align: left;
  }
  kbd {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-xs);
  }
  .mark {
    display: block;
    width: 22px;
    height: 22px;
  }
  .name {
    flex: 1;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
    letter-spacing: -0.01em;
  }
  .mode {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    margin-left: calc(-1 * var(--space-1));
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font-family: inherit;
    text-align: left;
    cursor: pointer;
  }
  .mode :global(svg) {
    color: var(--color-text-muted);
  }
  .mode:hover {
    background: var(--color-bg-hover);
  }
  .mode:focus-visible {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -2px;
  }
  .tasks-body,
  .mode-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    animation: mode-fade var(--duration-base) var(--ease);
  }
  .tasks-body[hidden],
  .mode-body[hidden] {
    display: none;
  }
  @keyframes mode-fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tasks-body,
    .mode-body {
      animation: none;
    }
  }
  .primary {
    padding: var(--space-1) var(--space-3) var(--space-3);
  }
  .collapsed .primary {
    display: flex;
    justify-content: center;
    padding: var(--space-1) 0 var(--space-3);
  }
  .new-compact {
    display: grid;
    place-items: center;
    width: 40px;
    height: 36px;
    border: 0;
    border-radius: var(--radius-md);
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
    cursor: pointer;
  }
  .new-compact:hover {
    background: var(--color-accent-fill-hover);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* Feste Breite: ohne das würde overflow-y auch horizontal scrollbar machen (unsichtbare
       Tooltips am rechten Rand ragten ein paar Pixel hinaus). */
    overflow-x: hidden;
    padding: 0 var(--space-2);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    margin-top: var(--space-4);
    padding: 0 var(--space-1) 0 10px;
  }
  .collapsed .section {
    justify-content: center;
    height: 16px;
    padding: 0;
  }
  .section-label {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .divider {
    width: 24px;
    height: 1px;
    background: var(--color-border);
  }
  .lists {
    position: relative;
  }
  .drop-line {
    position: absolute;
    left: var(--space-2);
    right: var(--space-2);
    height: 2px;
    border-radius: 1px;
    background: var(--color-accent);
    pointer-events: none;
  }
  .hint {
    padding: var(--space-1) 10px;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }
  .footer-nav {
    padding: var(--space-2);
    border-top: 1px solid var(--color-border);
  }
  .collapsed .footer-nav {
    display: flex;
    justify-content: center;
    border-top: 0;
  }
</style>
