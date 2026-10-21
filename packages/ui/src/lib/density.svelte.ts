/**
 * Dichte (P5-09, `ui-guidelines.md` §2): Zeilen 36 px am Desktop, 44 px bei Touch oder schmalem Fenster –
 * dieselbe Media-Query wie `--row-height` in `tokens.css`. Die virtuelle Liste braucht die Zahl in JS;
 * `narrow` schaltet die Bedienung der schmalen Ansicht (Button „Neue Task“ unten, Menü statt Kopfzeilen-Icons).
 * Nur Media-Query-Ereignisse, kein Polling.
 */
export const COMFORTABLE_QUERY = "(pointer: coarse), (max-width: 699px)";
export const NARROW_QUERY = "(max-width: 699px)";
export const ROW_COMPACT = 36;
export const ROW_COMFORTABLE = 44;

export class Density {
  rowHeight = $state(ROW_COMPACT);
  narrow = $state(false);

  constructor() {
    if (typeof matchMedia !== "function") return;
    const comfortable = matchMedia(COMFORTABLE_QUERY);
    const narrow = matchMedia(NARROW_QUERY);
    const update = () => {
      this.rowHeight = comfortable.matches ? ROW_COMFORTABLE : ROW_COMPACT;
      this.narrow = narrow.matches;
    };
    update();
    comfortable.addEventListener?.("change", update);
    narrow.addEventListener?.("change", update);
  }
}

export const density = new Density();
