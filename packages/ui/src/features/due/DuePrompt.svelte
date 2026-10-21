<script lang="ts">
  /**
   * „Datum wählen…“ (P4-04, P5-08): `DuePicker` im Dialog – Text wie in der Titelzeile von „Neue Task“
   * („fr“, „24.10.“, „morgen 14:00“, derselbe Parser) oder ein Tag im Raster. Jede Wahl setzt und schließt;
   * die Uhrzeit je Task behält `applyDue`.
   */
  import type { DueDto } from "@tasks/core-client";
  import Dialog from "../../components/Dialog.svelte";
  import DuePicker from "./DuePicker.svelte";

  interface Props {
    /** Für den Titel, z. B. „3 Tasks“. */
    subject: string;
    onsubmit: (due: DueDto) => void;
    onclose: () => void;
  }

  let { subject, onsubmit, onclose }: Props = $props();
</script>

<Dialog title="Fälligkeit für {subject}" {onclose} width={340}>
  <DuePicker
    value={null}
    removable={false}
    withTime={false}
    autofocus
    onchange={(due) => {
      if (!due) return;
      onsubmit(due);
      onclose();
    }}
  />
</Dialog>
