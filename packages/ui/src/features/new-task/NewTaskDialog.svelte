<script lang="ts">
  /**
   * Fenster „Neue Task“ in der App (geöffnet über „+ Neue Task“, `N`, `Strg+N`). Der endgültige
   * Inhalt wird noch festgelegt; bis dahin die bisherige Eingabezeile (Titel, Fälligkeit per
   * „heute/morgen“, Listenwahl). `Enter` legt an und schließt, `Esc` schließt.
   */
  import type { DueDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import Dialog from "../../components/Dialog.svelte";
  import type { ListsState } from "../../lib/lists.svelte";
  import QuickAdd from "../quick-add/QuickAdd.svelte";

  interface Props {
    lists: ListsState;
    listId?: string;
    shownListId?: string;
    onopenlist?: (listId: string) => void;
    defaultDue?: DueDto | null;
    onclose: () => void;
  }

  let { lists, listId, shownListId, onopenlist, defaultDue = null, onclose }: Props = $props();
  let quickAdd = $state<QuickAdd>();

  onMount(() => {
    void tick().then(() => quickAdd?.focus());
  });

  /** `Alt+L`: Listenwahl öffnen. */
  export function openListPicker() {
    quickAdd?.openListPicker();
  }
</script>

<Dialog title="Neue Task" {onclose} width={560}>
  <div class="body">
    <QuickAdd
      bind:this={quickAdd}
      {lists}
      {listId}
      {shownListId}
      {onopenlist}
      {defaultDue}
      placeholder="Titel – z. B. „Zahnarzt anrufen morgen“"
      onadded={onclose}
      onescape={onclose}
    />
  </div>
</Dialog>

<style>
  .body :global(.quick-add) {
    margin: 0;
  }
</style>
