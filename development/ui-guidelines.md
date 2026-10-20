# UI-Richtlinien (Proton-nah)

Ziel: Eine App, die sich neben Proton Mail und Proton Drive „zu Hause" anfühlt – ruhig, dicht, aufgeräumt –
ohne Protons Code, Assets oder Marke zu übernehmen.

> **Rechtliches:** Protons Web-Clients (inkl. Design-System) sind GPL-3.0; Name und Logo sind Marken.
> Wir übernehmen **keinen** Code, keine Icons, Logos oder Schriftdateien aus Proton-Repos. Gestaltungsprinzipien
> dürfen inspirieren; Farben und Maße sind eigene Werte.

## 1. Gestaltungsprinzipien

1. **Ruhe vor Dekoration.** Flache Flächen, feine 1-px-Trennlinien, kaum Schatten, keine Verläufe.
2. **Inhalt ist dicht, Chrome ist leicht.** Viele Tasks auf einen Blick; Sidebar/Toolbar zurückhaltend.
3. **Eine Akzentfarbe.** Violett als Primärfarbe – nur für primäre Aktionen, Auswahl, Fokus, aktive Navigation.
4. **Tastatur first.** Alles ohne Maus bedienbar; Shortcuts sichtbar in Tooltips und Menüs.
5. **Sofortige Reaktion.** Optimistische Updates, keine Spinner für lokale Aktionen.
6. **Gleiche Muster wie Proton:** linke Sidebar mit Navigation + großem Primärbutton oben, Listen-/Detail-Split,
   Hover-Aktionen in Zeilen, Kontextmenüs, dezente Toasts unten links mit „Rückgängig".

## 2. Layout

```
┌──────────────┬───────────────────────────────────────────┬──────────────────────┐
│ [+ Neue Task]│  Heute                        🔍  ⋯        │ Detail (optional)    │
│              │───────────────────────────────────────────│                      │
│ ▸ Eingang 12 │ ○ Steuererklärung abgeben     📅 Mo   ●   │ Titel                │
│ ▸ Heute    3 │ ○ Zahnarzt anrufen            📅 Heute    │ Notizen …            │
│ ▸ Geplant    │ ○ PR reviewen                 #arbeit     │ Fällig · Priorität   │
│ ▸ Erledigt   │ ✓ Milch kaufen  (durchgestrichen, gedimmt)│ Liste · Tags         │
│ ── Listen ── │                                           │ Subtasks             │
│ ▸ Privat     │                                           │                      │
│ ▸ Arbeit     │                                           │                      │
│ ── ── ── ──  │                                           │                      │
│ ⟳ Synchron.  │                                           │                      │
└──────────────┴───────────────────────────────────────────┴──────────────────────┘
  Sidebar 240px       Liste (flexibel, min. 360px)              Detail 360–420px
```

- Breakpoints: ≥ 1100 px drei Spalten · 700–1100 px Detail als Overlay-Panel · < 700 px Ein-Spalten-Navigation
  (vorbereitet für Android).
- Zeilenhöhe Listen: **36 px kompakt** (Standard Desktop), **44 px komfortabel** bei Touch oder Fenster < 700 px
  (P5-09: `@media (pointer: coarse), (max-width: 699px)` in `tokens.css`, dieselbe Query in `lib/density.svelte.ts`
  für die virtuelle Liste).
- Sidebar einklappbar (Icon-only, 56 px).
- **Lesespalte (P5-02):** Kopfzeile und Liste teilen eine zentrierte Spalte von höchstens `--content-max` (820 px);
  Zeilen ohne Trennlinien, Hover/Auswahl/Cursor als gerundete Fläche mit 8 px Einzug, Akzentbalken innerhalb der
  Rundung. Das Board bleibt bei voller Breite.
- **Kopfzeile (P5-05):** Titel 24 px (`--text-2xl`) mit gedämpfter Unterzeile – „Heute": „Mittwoch, 21. Oktober ·
  3 offen · 1 überfällig", Eingang „4 offen", sonst leer (Zählen ohne neuen Core-Aufruf; im Board keine). Rechts nur
  Darstellung, Auswählen und Listen-/Erledigt-Aktionen; Einstellungen in der Sync-Fußzeile (beide Modi),
  Tastaturkürzel über `?` und Palette.
- **Suche (P5-05):** Feld „Suchen … Strg K" unter der Brand-Zeile der Seitenleiste (beide Modi; eingeklappt Icon),
  öffnet die Befehlspalette.
- **Schmal < 700 px (P5-09):** runder Button „Neue Task" (56 px) unten rechts, Toasts darüber; die Kopfzeilen-Icons
  wandern in ein Menü „Ansicht und Aktionen" (`⋯`); unter der letzten Zeile 80 px Platz für den Button.
- **Kanban-Board (P2):** Umschalter „Liste | Kanban" in der Kopfzeile links neben „Auswählen" (nur Eingang, Listen,
  „Alle Tasks"; gemerkt pro Gerät in `localStorage` `tasks.layout`). Das Board zeigt **alle** Listen als Spalten
  nebeneinander (280 px, Abstand 12 px, Eingang zuerst, horizontal scrollend), darin die offenen Tasks als Karten
  (Platz 64 px: Karte 56 px + Abstand; virtualisiert). Die gewählte Liste wird als Spalte hervorgehoben (Akzentrahmen)
  und ins Bild gescrollt. Waagerecht scrollen auch mit `Umschalt` + Mausrad (WebKitGTK tut das nicht von selbst).

```
┌─ Eingang ─────┐ ┌─ Einkaufen ──── ⋯ ┐ ┌─ Arbeit ─────── ⋯ ┐
│ ○ Zahnarzt    │ │ ○ Milch            │ │ ○ Bericht  Mo ⚑   │
│ ○ Paket       │ │ ○ Brot             │ │                    │
└───────────────┘ └────────────────────┘ └────────────────────┘
```

**Notes-Modus (N5-07, `notes.md` §5/§6):** Der Name neben dem Logo ist ein Button („Tasks ⇄“ bzw. „Notes ⇄“,
Tooltip „Zu Notes wechseln (Strg+2)“; eingeklappt ein Icon-Button) und wechselt den Modus, ebenso `Strg+1`/`Strg+2`
(auch in Feldern) und die Palette („Zu Tasks/Notes wechseln“). Kopf der Seitenleiste und Sync-Fußzeile bleiben stehen,
Inhalt und Hauptbereich blenden in 160 ms über (nur `opacity`, bei `prefers-reduced-motion` nicht). 2 Spalten: Baum
+ Editor, kein Detail-Panel. Beide Teilbäume bleiben nach dem ersten Laden gemountet (`hidden`); der Notes-Teil ist
ein eigener Lazy-Chunk (10,6 KB gzip mit Baum und Papierkorb). Gemerkt pro Gerät: `tasks.mode`, `notes.open`,
`notes.expanded`.

```
┌─ [◆] Notes ⇄ ───────┐
│ [+ Neue Notiz]   ⋯   │  ⋯ = Neuer Ordner, Ordner importieren…
│ ▾ Uni                │  Zeilen 32 px, Einrückung 16 px je Ebene, virtualisiert
│   ▾ Analysis         │  Ordner-Inhalt erst beim Aufklappen geladen
│     ▤ Blatt 3     ◀  │  ▤ .txt / ⌗ .md (Icon), offene Notiz = aktive Navigation
│ ─────────────────── │
│ 🗑 Papierkorb      3 │  öffnet die Liste der obersten Elemente im Hauptbereich
│ ⟳ Synchronisiert     │
└──────────────────────┘
```

Baum: `role="tree"`, Zeilen `treeitem` mit `aria-level`/`aria-expanded`; Klick öffnet die Notiz bzw. klappt den Ordner,
Doppelklick benennt inline um, Ziehen (ab 4 px) verschiebt (mittlere Hälfte eines Ordners = hinein, sonst
Einfügelinie), Rechtsklick/`Umschalt+F10` = Kontextmenü (Neue Notiz hier, Neuer Ordner hier, Umbenennen, Typ,
Verschieben nach…, In den Papierkorb). Gleichnamige Elemente im selben Ordner sind erlaubt (Hinweis-Toast). Papierkorb:
Name, Herkunft, „gelöscht vor 3 Tagen“, Wiederherstellen, Endgültig löschen und „Papierkorb leeren“ (je mit
Bestätigung); Notizen darin sind nur lesbar.

Editor (N5-08, `NoteEditor`): natives `<textarea>` über die volle Fläche, `.md` in `--font-sans`, `.txt` in
`--font-mono`, 15 px / Zeilenhöhe 1,6, Textspalte 72ch zentriert (Einstellung „Zeilenbreite begrenzen“),
Rechtschreibprüfung aus. Autosave 400 ms nach der letzten Eingabe, sofort bei Blur, Fenster verborgen, Notiz- und
Moduswechsel und vor dem Schließen (die Shell wartet bis 1,5 s); je Notiz ein Speichern nach dem anderen.
Remote-Revisionen kommen in den Puffer (gemeinsamer Anfang/Ende, `setRangeText`, Cursor und Scroll bleiben); mit
ungespeicherten Eingaben wird sofort gespeichert und der gemergte Text übernommen. Statuszeile 28 px: Pfad › Name.ext
(Klick benennt um) · Gespeichert/Speichert…/Nicht synchronisiert/Synchronisiert · Wörter · Zeichen · ausgewählt
(gezählt 300 ms nach der Eingabe), ab 256 KB „Große Notiz“, `⋯` (Umbenennen, Typ, Verschieben nach…, Papierkorb, Zen).
Zen (`Strg+Umschalt+F`): ohne Seitenleiste, Statuszeile nur nach Mausbewegung oder mit Fokus; das erste `Esc`
beendet Zen. Konfliktkopien melden ein Toast „Konflikt – Kopie „…“ angelegt“ mit „Anzeigen“. Einstellungen →
„Notizen“: Typ neuer Notizen (Markdown/Text), Zeilenbreite begrenzen.

Ordner importieren (N5-11, nur Desktop, `⋯` und Palette): System-Dialog des Portals, danach Dialog „Ordner
importiert“ mit „412 Notizen in 37 Ordnern importiert, 9 übersprungen.“ und aufklappbarer Liste der übersprungenen
Pfade mit Grund; der neue Ordner wird aufgeklappt und markiert. Im Web fehlt der Eintrag.

Vorschau (N5-09, `.md`): `Strg+E` bzw. „Bearbeiten | Vorschau“ in der Statuszeile, je Notiz gemerkt
(`notes.view`). Eigener Renderer (`markdown/render.ts`, ~4 KB gzip) erzeugt DOM-Knoten, nie `innerHTML` aus dem
Text; Links nur `http:`/`https:`/`mailto:`, ein Klick öffnet den Standardbrowser (`open_external`), nie die
WebView. Checklisten sind anklickbar und ändern genau ihre Quellzeile (wie eine Eingabe gespeichert). Beim Umschalten
springt die Vorschau zur Zeile des Cursors und zurück zur obersten sichtbaren Zeile. Ab ~200 KB erst „Vorschau wird
erstellt…“. Typografie: Überschriften 1,6/1,3/1,1 em, Code in `--font-mono` auf `--color-bg-subtle`, Zitate mit
3-px-Linie, Tabellen mit 1-px-Rahmen.

## 3. Design-Tokens

Als CSS Custom Properties in `packages/ui/src/tokens/tokens.css`. Komponenten nutzen **nur** semantische Tokens.

```css
:root {
  /* Basis-Palette (eigene Werte, Proton-ähnliche Anmutung) */
  --violet-500: #6d4aff;  --violet-600: #5b3ae6;  --violet-100: #ede8ff;

  /* Semantisch – Light */
  --color-bg:            #ffffff;
  --color-bg-subtle:     #f5f5f7;   /* Sidebar */
  --color-bg-hover:      #ebebef;
  --color-bg-selected:   #ede8ff;
  --color-border:        #e3e3e8;
  --color-text:          #1b1a24;
  --color-text-muted:    #6c6b78;
  --color-text-disabled: #a8a7b3;
  --color-accent:        var(--violet-500);
  --color-accent-hover:  var(--violet-600);
  --color-on-accent:     #ffffff;
  --color-danger:        #c42340;   /* Text, ≥ 4.5:1 auch auf Hover/Auswahl (P4-02); Fläche: --color-danger-fill #d42b4c */
  --color-warning:       #c77700;
  --color-success:       #1e8a5a;
  --color-focus-outline: var(--color-accent);   /* Fokus-Umriss, deckend ≥ 3:1 (WCAG 1.4.11) */
  --color-focus-ring:    color-mix(in srgb, var(--violet-500) 55%, transparent);  /* nur Halo an Eingabefeldern, nie als alleiniger Umriss */

  /* Raster & Form */
  --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-4: 16px; --space-6: 24px; --space-8: 32px;
  --radius-sm: 4px; --radius-md: 8px; --radius-lg: 12px; --radius-full: 999px;
  --row-height: 36px;

  /* Typografie */
  --font-sans: system-ui, "Inter", "Cantarell", "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, "JetBrains Mono", monospace;
  --text-xs: 12px; --text-sm: 13px; --text-md: 14px; --text-lg: 16px; --text-xl: 20px;
  --weight-regular: 400; --weight-medium: 500; --weight-semibold: 600;

  /* Bewegung */
  --duration-fast: 100ms; --duration-base: 160ms; --ease: cubic-bezier(.2,.0,.0,1);
  --shadow-popover: 0 4px 16px rgb(0 0 0 / .12);   /* nur für Menüs/Dialoge */
}

[data-theme="dark"] {
  --color-bg:            #191927;
  --color-bg-subtle:     #14141f;
  --color-bg-hover:      #25253a;
  --color-bg-selected:   #2e2752;
  --color-border:        #2c2c40;
  --color-text:          #ececf3;
  --color-text-muted:    #9a99ad;
  --color-text-disabled: #5d5c70;
  --color-accent:        #8a6cff;   /* heller für Kontrast auf dunkel */
  --color-accent-hover:  #9d84ff;
  --shadow-popover: 0 4px 16px rgb(0 0 0 / .45);
}
```

**P5 (2026-10-21):** Schrift „Inter Variable" (Latin-Subset, Achse `wght`, 48 KB WOFF2 im Bundle, OFL; ADR-030) vor
dem System-Stack; `--text-2xl: 24px` (Ansichtstitel); `--content-max: 820px` und `--content-inset` (Rand der
Lesespalte); `--row-height` 44 px bei Touch/schmal. `tabular-nums` gezielt für Zähler, Daten, Uhrzeiten, Statuszeilen.

Umsetzung (P7-05, `packages/ui/src/tokens/tokens.css`) – Abweichungen nur für Kontrast, automatisch geprüft
(`tokens.test.ts`, Text ≥ 4.5:1, Bedienelemente ≥ 3:1, beide Themes):
- Hell: `--color-text-muted` `#65646f` (statt `#6c6b78`, 4.4:1 auf Hover), `--color-warning` `#a35f00` (statt `#c77700`,
  3.5:1), `--color-success` `#18794e` (statt `#1e8a5a`, 4.3:1).
- Gefüllte Flächen mit weißem Text getrennt: `--color-accent-fill` (in **beiden** Themes `#6d4aff`; Weiß auf `#8a6cff`
  hätte nur 3.7:1) und `--color-danger-fill`; `--color-accent`/`--color-danger` bleiben für Text, Icons, Fokus.
- Neu: `--color-bg-active`, `--color-bg-raised`, `--color-bg-inverse`/`--color-text-inverse`/`--color-accent-inverse`
  (Toasts, Tooltips), `--color-border-strong` (Eingabefelder), `--color-control-border` (Checkbox-Ring ≥ 3:1),
  `--color-accent-text`, `--color-danger-subtle`, `--control-height(-lg)`, `--space-5`.
- Tooltips global per `data-tooltip` (nur CSS, 450 ms Verzögerung, sofort bei Tastaturfokus).

Theme-Regeln:
- Drei Modi: **System** (Standard, via `prefers-color-scheme`), Hell, Dunkel. Attribut `data-theme` auf `<html>`.
- Theme vor dem ersten Paint setzen (Inline-Snippet im `<head>`), sonst weißer Blitz beim Dark-Start.
- Kontrast: Text ≥ 4.5:1, große Texte/Icons ≥ 3:1 – in beiden Themes prüfen.
- Dunkel ist nicht invertiert: leicht blau-violetter Grundton, Flächen-Hierarchie über Helligkeit, nicht über Schatten.

## 4. Komponenten (v1-Bestand)

| Komponente | Hinweise |
|---|---|
| `Button` | Varianten `primary` (Akzent, gefüllt), `secondary` (Rahmen), `ghost`, `danger`; Höhe 32 px (36 px groß) |
| `IconButton` | 28/32 px, immer mit `aria-label` + Tooltip |
| Tooltips (`Tooltips.svelte`) | als `popover` gezeigt (oberste Ebene, auch über modalen Dialogen); per `data-tooltip` (+ `data-tooltip-side` bottom/top/right/left) an jedem Element; **eine** fest positionierte Schicht in `App` statt CSS-`::after` – nie von `overflow` abgeschnitten (eingeklappte Seitenleiste), am Fensterrand im Bild gehalten; 450 ms Verzögerung bei Hover, sofort bei Tastaturfokus; `aria-hidden` (Name kommt aus `aria-label`) |
| `Checkbox` (Task) | rund (Proton-artig); **die Priorität steht nur im Ring** (hoch rot, mittel orange, niedrig violett, zart gefüllt; Tooltip und Ende des Zeilennamens nennen sie – kein Fähnchen mehr, P5-03). Hover zeigt Haken angedeutet, Abhaken animiert ≤ 160 ms; danach (250 ms, bei `prefers-reduced-motion` sofort) verlässt die Zeile jede offene Ansicht – erledigte Tasks stehen nur unter „Erledigt", Rückgängig im Toast (P1-01). Der Cursor springt auf die nächste Zeile |
| Abschnitte (P4, `GroupHeading`) | „Heute": **Überfällig · n** (rot, älteste zuerst, Aktion „Alle auf heute" – ein `set_dues`, Uhrzeit bleibt, Rückgängig) über **Heute · n**; ohne Überfällige keine Überschrift. „Geplant": Morgen, Einzeltage („Mi, 21. Okt.") bis 6 Tage, „Später in diesem Monat", Monate (fremdes Jahr mit Jahreszahl). Überschrift = Zeilenhöhe in der virtuellen Liste, `role="presentation"`, nicht fokussierbar (↑/↓ überspringen). Zeilen unter einem Tagesabschnitt zeigen nur die Uhrzeit. Seitenleiste „Heute": Zahl der Überfälligen rot vor dem Zähler. „Erledigt" (P4-06): Heute, Gestern, Wochentage dieser Woche, Letzte Woche, Monate; „⋯" → „Erledigte älter als 30 Tage löschen…" (Bestätigung, Rückgängig) |
| Befehlspalette (P4-05, `CommandPalette`, lazy) | Modal oben (12 vh), Eingabe als `combobox` + `aria-activedescendant`, Ergebnisse als `listbox` mit Gruppen „Tasks" (Core-Suche, 100 ms entprellt, ≤ 50, Treffer im Titel `<mark>`, sonst „in Notiz"; Fälligkeit, Liste), „Notizen" (N5-10: parallel zur Task-Suche, Name mit `<mark>`, bei Inhaltstreffern der Ausschnitt ±40 Zeichen darunter, rechts der Ordnerpfad; im Modus „Notes" vor „Tasks"; Enter wechselt in den Modus „Notes", öffnet die Notiz und markiert den Treffer im Quelltext), „Listen", „Befehle" (Registry-Einträge mit `palette`, z. B. „Gehe zu Heute", „Neue Liste…"). Ohne Text: „Zuletzt geöffnet" (Sitzung) + Befehle. Kein Treffer: Enter öffnet „Neue Task" mit dem Text. `↑/↓`, `Enter`, `Esc` |
| `ListPicker` (P2) | Rundlicher Button „Liste ▾" mit Icon (Eingang/`#`), Menü aller Listen (Eingang zuerst, aktuelle mit Häkchen) + „Neue Liste…" (Dialog, wählt die neue sofort). Im Fenster „Neue Task" (Vorauswahl = Liste der Ansicht, `Alt+L`) und im Detail-Panel |
| Listenaktionen (P2) | Kontextmenü eines Listeneintrags (Rechtsklick, `Umschalt+F10`), Button „⋯" im Kopf einer Listenansicht und einer Board-Spalte: Umbenennen (inline in der ausgeklappten Seitenleiste, sonst Dialog), Nach oben/unten, Löschen… (Bestätigungsdialog nennt, wie viele offene/erledigte Tasks in den Eingang wandern). Eingang ohne Aktionen |
| Board (P2) | `Board` → `BoardColumn` (Kopf: Name, Zahl offener Tasks, „⋯"; Kopf ist Griff zum Umsortieren) → `BoardCard` (runder Haken, Titel einzeilig, Meta-Zeile). Ziehen einer Karte nach 4 px mit Vorschau am Zeiger und Einfügelinie; Spalten ziehen mit senkrechter Linie. Spalten `listbox`, Karten `option` (`aria-selected` = Cursor), Live-Region meldet Verschiebungen |
| `SelectBox` (Auswahl, P0) | **eckig** (16 px, Radius 3 px) – bewusst anders als der runde Erledigt-Haken; ausgewählt = Akzentfläche mit Häkchen, Tri-State „–" für „Alle auswählen"; in Zeilen nur dekorativ (die Zeile trägt `aria-selected`), in der Aktionsleiste ein `role="checkbox"` |
| `SelectionBar` (Aktionsleiste, P0) | ersetzt im Auswahlmodus die Kopfzeilen-Aktionen (gleiche Höhe, keine zweite Zeile): Tri-State „Alle auswählen" · Zähler „3 ausgewählt" (Live-Region) · Erledigt · Wieder öffnen · Verschieben nach… (Menü, Eingang zuerst, aktuelle Liste deaktiviert) · Löschen · „Fertig". Aktionen bei leerer Auswahl deaktiviert; Erledigt/Wieder öffnen je nach offenen/erledigten in der Auswahl |
| `TaskMeta` (P5-03) | Meta-Angaben in Zeile und Board-Karte, gleiche Reihenfolge: `#Liste` (nur Sammelansichten) · Fälligkeit (rot nur überfällig, Akzent heute) · Notiz-Icon (`sticky-note`) |
| Zähler der Seitenleiste (P5-06) | eine Zahl je Eintrag; mit Überfälligen als rote Pille (`--color-danger-subtle`), Aufteilung im Tooltip und im Namen („3, davon 1 überfällig") |
| `TextInput`, `Textarea` | 32 px, Rahmen `--color-border`, Fokus: Akzent-Rahmen + Focus-Ring |
| `Menu` / Kontextmenü | Popover mit `--shadow-popover`, Tastaturnavigation, Shortcuts rechtsbündig gedimmt; über die Popover-API in der obersten Ebene (`lib/popover.ts`), damit es auch in und über modalen Dialogen bedienbar ist (P6-02) |
| `Dialog` | für destruktive Bestätigungen, Einstellungen und Eingabefenster; Esc schließt. `bare`: ohne Kopf (Name nur über `aria-label`), `placement="top"`: fest bei 15 vh, wächst nur nach unten; unter 700 px ein Bereich, der unten aufgeht (volle Breite, max. 85 dvh) |
| `Toaster` (+ `toasts.svelte.ts`) | unten links (schmal über dem runden Button „Neue Task"), 4 s, „Rückgängig"-Aktion bei Löschen/Abhaken/Verschieben – bei Sammelaktionen eine Meldung für die ganze Auswahl („3 Tasks gelöscht") |
| `VirtualList` | eigene, kleine Implementierung (feste Zeilenhöhe → trivial und schnell) |
| `DuePicker` / `DueField` (P5-08) | eigene Umsetzung statt `<input type=date/time>`: Texteingabe „Fällig am" mit demselben Parser wie „Neue Task" (`parseDue`: „morgen", „fr 14:00", „24.10."; Vorschau darunter, Fehler als `alert`), Schnellwahl (Heute, Morgen, Nächster Montag, In einer Woche), Monatsraster (Montag zuerst, `role="grid"`, Pfeile/Pos1/Ende/Bild↑↓, gewählter Tag gefüllt, heute mit Punkt), Uhrzeit als Text („14:00", leer = ganztägig), „Entfernen". Ein reiner Tag behält die Uhrzeit. `DueField` im Detail: Button mit Fälligkeit im Ton der Zeile → Popover (Esc, Klick daneben, Tab bleibt drin). Im Fenster „Neue Task" klappt der Chip „Fällig" den Wähler auf; „Datum wählen…" (`D`) zeigt ihn im Dialog |
| `NewTaskDialog` (Fenster „Neue Task", P3-03, Schnelleingabe seit P6) | Öffnet über „+ Neue Task" (Seitenleiste, leere Ansicht, runder Button schmal), `N`, `Strg+N`; `Dialog` `bare` + `placement="top"`. Eine Karte: Titelzeile (16 px) mit Erkennung (Tag, Wochentag, `24.10.`, `14:00`, `!1`–`!3`, `@liste`; `parseNewTask`) – erkannte Stellen sind im Text getönt markiert (`recognizedMarks`, Ebene hinter dem transparenten Feld; Datum Akzent, Priorität Ringfarbe, Liste neutral; ein von Hand gesetzter Chip hebt die Markierung seiner Art auf); darunter randlos „Notiz hinzufügen …" (wächst mit). Chips: leer nur Kontur, gesetzt getönt mit vollem Wert („Fr, 23. Okt. 14:00") und × („Fälligkeit/Priorität entfernen"); Priorität als Ring; „Fällig" öffnet `DuePopover` am Chip. Fußleiste: „Weitere anlegen" · „?" (Syntax im Tooltip) · `Esc` · „Anlegen ↵" (leer gedämpft). Mit „Weitere anlegen" bestätigt eine Zeile im Fenster statt eines Toasts. `Enter` bzw. `Strg+Enter` (Notiz) legt an (ein `createTask`), `Alt+D/P/L/N` für Fälligkeit/Priorität/Liste/Notiz. Schmal: Chips waagerecht scrollbar, „Anlegen" groß |

Icons: **Lucide** (ISC-Lizenz), einzeln als SVG importiert (Tree-Shaking), 16 px in Listen, 20 px in Toolbars,
`stroke-width: 1.75`, Farbe `currentColor`.

## 5. Interaktion & Tastatur

| Aktion | Shortcut |
|---|---|
| Neue Task (öffnet das Fenster „Neue Task") | `N` bzw. `Strg+N` global |
| Suchen und Befehle (Palette, P4-05) | `Strg+K` (auch in Feldern), `/` (außerhalb von Feldern) |
| Navigieren | `↑/↓` oder `J/K` |
| Erledigt umschalten | `Leertaste` bzw. `E` |
| Bearbeiten / Detail öffnen | `Enter` |
| Löschen | `Entf` (mit Rückgängig-Toast, kein Dialog) |
| Fälligkeit: heute / morgen (Uhrzeit bleibt, Rückgängig) | `T` / `Umschalt+T` (P4-04; Zeile, Karte oder Auswahl) |
| Fälligkeit wählen… (Menü: Heute, Morgen, Nächster Montag, In einer Woche, Datum wählen…, Entfernen) | `D` (auch Auswahlleiste und Kontextmenü der Zeile) |
| Kürzel-Übersicht | `?` |
| Schließen / Auswahl aufheben | `Esc` |
| Ansichten | `G` dann `I` (Eingang), `T` (Heute), `U` (Geplant) |

**Mehrfachauswahl (P0).** Modus an per Button „Auswählen" (Kopfzeile), `Strg`/`Umschalt`+Klick (ab Cursor-Zeile, wie
im Dateimanager), `Umschalt+↑/↓` oder `Strg+A`; aus per „Fertig", `Esc` oder Ansichtswechsel. Im Modus schaltet ein
Klick um (öffnet nicht), `Umschalt`+Klick fügt den Bereich ab Anker hinzu; der Cursor (Akzentbalken) bleibt von der
Auswahl (Auswahlfarbe) getrennt. Kontextmenü auf einer ausgewählten Zeile wirkt auf die ganze Auswahl.

| Aktion im Auswahlmodus | Shortcut |
|---|---|
| Alle auswählen / keine (ganze Ansicht, lädt fehlende Seiten) | `Strg+A` (in Textfeldern nativ) |
| Cursor-Zeile aus-/abwählen | `X` bzw. `Leertaste` |
| Auswahl erweitern | `Umschalt+↑/↓` |
| Ausgewählte erledigen bzw. wieder öffnen | `E` |
| Ausgewählte löschen (Rückgängig) | `Entf` |
| Verschieben nach… | `M` |
| Fälligkeit (ein `set_dues` für alle) | `T` / `Umschalt+T` / `D` |
| Auswahlblock nach oben/unten (nur Eingang/Listen; verstreute Auswahl wird zusammengezogen) | `Alt+↑/↓` |
| Modus beenden | `Esc` |

**Listen & Board (P2).**

| Aktion | Shortcut |
|---|---|
| Darstellung Liste ↔ Kanban | `V` |
| Im Fenster „Neue Task": Fälligkeit / Priorität / Liste / Notiz | `Alt+D` / `Alt+P` / `Alt+L` / `Alt+N` |
| Listenaktionen der fokussierten Liste (Seitenleiste) | `Umschalt+F10` bzw. Menütaste |
| Fokussierte Liste in der Seitenleiste verschieben | `Alt+↑/↓` |
| Board: Karte wählen / Spalte wechseln | `↑/↓`, `J/K` / `←/→`, `H/L` (gleiche Höhe) |
| Board: Karte öffnen / abhaken / löschen | `Enter` / `Leertaste`, `E` / `Entf` |
| Board: Karte in die Nachbarspalte / in der Spalte verschieben | `Alt+←/→` / `Alt+↑/↓` |
| Board: fokussierte Spalte (Kopf) verschieben | `Alt+←/→` |

**Notes (N5-07).** Task-Kürzel gelten nur im Modus „Tasks“ (Registry-`scope`), die folgenden nur im Modus „Notes“.

| Aktion | Shortcut |
|---|---|
| Modus Tasks / Notes (überall, auch in Feldern) | `Strg+1` / `Strg+2` |
| Neue Notiz (im Ordner der offenen Notiz, Editor fokussiert; leer verlassen = verworfen) | `N` bzw. `Strg+N` |
| Baum: bewegen / aufklappen, hinein / zuklappen, Elternordner | `↑/↓`, `J/K` / `→` / `←` |
| Baum: Notiz öffnen und Editor fokussieren / Ordner auf- und zuklappen | `Enter` |
| Baum: umbenennen / in den Papierkorb (Rückgängig) / verschieben nach… | `F2` / `Entf` / `M` |
| Baum: unter Geschwistern nach oben/unten | `Alt+↑/↓` |
| Baum: Aktionen | `Umschalt+F10` bzw. Menütaste |
| Editor: Liste fortsetzen (`- `, `* `, `1.` → `2.`, `- [ ] `, `> `; leerer Punkt beendet) | `Enter` am Ende des Punkts |
| Editor: ein-/ausrücken (Auswahl zeilenweise; `.md` 2 Leerzeichen, `.txt` Tab) | `Tab` / `Umschalt+Tab` |
| Editor verlassen (statt Tab) | `Esc`, dann `Tab` |
| `.md`: Fett / Kursiv / Durchgestrichen / Link / Checkbox | `Strg+B` / `Strg+I` / `Strg+Umschalt+S` / `Strg+L` / `Strg+Umschalt+X` |
| `.md`: Überschrift 1–3 / Absatz (Ziffer über die Taste, auch mit AltGr) | `Strg+Alt+1…3` / `Strg+Alt+0` |
| `.md`: Adresse über markiertem Text einfügen → Link | `Strg+V` |
| Zen-Modus | `Strg+Umschalt+F` |

Bewusst kein „Keine auswählen" auf `Strg+Umschalt+A`. Seit P4-04 erkennt die Kürzel-Registry Shift bei **Buchstaben**
(`shift+t`), bei Satzzeichen weiterhin nicht (`?` ist je nach Layout schon Shift); Feststelltaste zählt nicht als Shift.

- Fokus immer sichtbar (`:focus-visible`), nie `outline: none` ohne Ersatz. Umrisse mit `--color-focus-outline` (deckend, ≥ 3:1 auf allen Flächen, in `tokens.test.ts` geprüft); `--color-focus-ring` ist halbtransparent und nur der zusätzliche Halo.
- Drag & Drop zum Umsortieren; immer auch per Tastatur (`Alt+↑/↓`). Ziehen einer ausgewählten Zeile zieht den Block
  (Zähler-Badge am Zeiger).
- `prefers-reduced-motion` respektieren: Animationen auf Opacity reduzieren oder abschalten.

## 6. Framework & Design-System-Empfehlung

- **Svelte 5** (Runes) – kleinste Runtime, feingranulare Updates, gute Tauri-Unterstützung (siehe ADR-003).
- **Eigenes Mini-Design-System** in `packages/ui`: Tokens + ~12 Basiskomponenten. Kein Material, kein Bootstrap,
  kein Tailwind. Aufwand überschaubar, volle Kontrolle über Dichte und Optik.
- **Headless-Primitives optional:** `bits-ui` (Svelte, auf Basis von Melt UI) für Menüs, Popover, Dialog, falls
  Barrierefreiheit (Focus-Trap, ARIA) selbst zu teuer wird. Einzeln importieren, Bundle-Größe prüfen.
- Styling: Komponenten-scoped CSS von Svelte + globale Tokens. Kein CSS-in-JS.

## 7. UI-Performance-Regeln

- Keine `backdrop-filter`, keine großflächigen Schatten, keine `filter`-Effekte in scrollenden Bereichen
  (WebKitGTK!).
- Animationen nur `transform`/`opacity`. Keine Dauer-Animationen im Leerlauf (Sync-Icon dreht nur während `Syncing`).
- Listen > 200 Zeilen virtualisieren; Zeilen sind leichtgewichtig (keine Komponenten-Bäume pro Zeile mit Menüs –
  Menüs erst bei Bedarf rendern).
- Schriften: System-Font-Stack als Standard; wenn eigene Schrift, dann **eine** variable WOFF2, `font-display: swap`,
  im Bundle (offline!).
- Bilder: keine. Icons inline als SVG.

## 8. Design-Review P7 (2026-10-18, Skills `frontend-design` + `ui-ux-pro-max`)

Geprüft gegen §1–§7 und die Checkliste des Skills; Sichtung per Screenshot (Demo `pnpm --filter @tasks/ui dev`, Parameter
`?theme=`, `?collapsed=`, `?open=1`, `?menu=1`, `?settings=1&login=1`) und in Tauri (Selbsttest-Overlays).

| Bereich | Befund | Status |
|---|---|---|
| Proton-Muster (§1, §6) | Sidebar mit großem Primärbutton, Listen/Detail-Split, Hover-Aktionen, Kontextmenü, Toasts unten links mit „Rückgängig“; Hauptbereich als abgerundete Fläche auf Sidebar-Ton (aktuelles Proton-Layout); eine Akzentfarbe | ✔ |
| Kontrast (P1) | Alle Token-Paare automatisch geprüft (`tokens.test.ts`, 46 Fälle, beide Themes); 6 Paare in P7-05 korrigiert, Danger-Füllung getrennt | ✔ |
| Fokus (P1) | Jedes `outline: none` mit Ersatz (Ring per box-shadow/Outline); Zeilen-Fokusring bei Tastaturbedienung unabhängig von `:focus-visible`; **Menüeinträge hatten nur Hover-Fläche → Ring ergänzt** | ✔ behoben |
| Namen (P1) | Icon-Buttons nur über `IconButton` mit Pflicht-`label` + Tooltip; Zeilen mit eigenem `aria-label`; Dialoge `aria-labelledby` | ✔ |
| Trefferflächen (P2) | Desktop dicht (36-px-Zeilen, §2); **Haken nur 20 px → 28 px Trefferfläche bei 20 px Optik** | ✔ behoben |
| Hover-only (P2) | Löschen an der Zeile nur bei Hover, aber auch per `Entf`, Kontextmenü und Detail-Panel | ✔ |
| Performance (P3) | Virtualisierung, `$state.raw`, Login-Dialog lazy; keine `backdrop-filter`/Blur; Schatten nur Popover | ✔ |
| Layout (P5) | 3 Spalten ≥ 1100 px, Overlay 700–1100, Schublade < 700; 360 px ohne horizontalen Scroll | ✔ |
| Typo/Farbe (P6) | System-Font-Stack, 13/14/20 px, nur semantische Tokens (keine Hex-Werte in Komponenten) | ✔ |
| Text (P6) | **„Nur auf diesem Gerät“ wurde neben „Anmelden“ abgeschnitten → sichtbar „Nur lokal“, lang in Tooltip/Label** | ✔ behoben |
| Animation (P7) | ≤ 160 ms, nur transform/opacity; `prefers-reduced-motion` setzt Dauern auf 0, Spinner nur während Sync und dann aus | ✔ |
| Formulare (P8) | Labels über den Feldern im Detail, Fehler direkt am Titel (`role=alert`), Speichern ohne Button | ✔ |
| Leer/Fehler (P8) | Leere Ansichten mit Icon + konkretem Hinweis; Fehler als Toast mit Klartext (Login: verständliche Meldungen) | ✔ |
| Navigation (P9) | Hash-Router (Zurück/Vor), `G I/T/U`, unbekannte Liste → Eingang | ✔ |

Abschlusskritik (`frontend-design`): Signatur (runder Haken) bleibt das einzige auffällige Element; Aktion und Toast
heißen gleich („Erledigt“ → „erledigt“, „Löschen“ → „gelöscht“). **Leere Ansichten boten keine Aktion → Button
„Neue Task“ (außer in „Erledigt“)** – behoben.

Offen (bewusst): Tags/Subtasks (nach dem Prototyp). Eigenes Datumsfeld seit P5-08 (`DuePicker`), Suche seit P4-05.

## 9. Design-Review P5 (2026-10-21)

Sichtung per Screenshot (Demo `app.html`, headless Chrome, 1440/1000/420 px, hell/dunkel; Parameter neu: `?layout=board`,
`?mode=notes`, `?due=1` mit `?open=1`) und Selbsttest in der WebView (`base`, `p3new`, `p4due`, `p4today`, `p4keys`,
`p708a/b`).

| Bereich | Befund | Status |
|---|---|---|
| Typografie | `system-ui` je nach Distribution anders, keine eigene Handschrift | ✔ Inter mitgeliefert (ADR-030), Titel 24 px |
| Lesbarkeit | Zeilen über die volle Breite (Titel links, Datum ~1 000 px weiter rechts), Trennlinien unter jeder Zeile | ✔ Lesespalte 820 px, Flächen statt Linien |
| Farbe/Bedeutung | Priorität doppelt (Ring + Fähnchen), Rot zugleich „hoch" und „überfällig" | ✔ Priorität nur im Ring, rote Schrift nur überfällig |
| Fokus | Cursor im Dunkelmodus als eckiger 2-px-Rahmen bis an den Rand | ✔ gerundet; Karte mit Fläche + Balken |
| Navigation | fünf gleichrangige Icons, Suche unsichtbar | ✔ Suchfeld in der Seitenleiste, Einstellungen in der Fußzeile (jetzt auch im Modus „Notes") |
| Zähler | „1 3" bei Heute mehrdeutig | ✔ eine Zahl, Pille bei Überfälligen |
| Detail | native Datumsfelder, doppelte Datumsanzeige, Notizfeld mit Griff, Fuß unten angeheftet | ✔ `DueField`/`DuePicker`, Notiz wächst mit, Fuß unter dem Inhalt |
| Touch/schmal | 36-px-Zeilen, „Neue Task" nur über die Schublade | ✔ 44 px, runder Button, Menü statt Icon-Reihe |

| Fenster „Neue Task" (P6) | mittig, sprang beim Aufklappen; Kopf und Syntax-Zeile wie ein Formular; erkannte Wörter nicht zu sehen; gesetzte und leere Chips gleich; Notiz als eigenes Kästchen; Prioritätsmenü hinter der modalen Ebene | ✔ oben verankert ohne Kopf, Markierung im Text, Chips mit ×, Datums-Popover, Notiz als zweite Zeile, Menüs als Popover |

Bewusst nicht in P5 (mit dem User geklärt): Eingabezeile in der Liste (P4-07 bleibt offen), Listenfarben,
Notiz-Vorschau in der Zeile (bräuchte DTO-Änderung und variable Zeilenhöhe).
