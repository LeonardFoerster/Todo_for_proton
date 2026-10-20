# Prototyp-Abnahme: zwei Geräte (P8-02)

Stand: 2026-10-18 · Prototyp-Definition: `archive/roadmap-prototype.md` „Ziel: Was ‚Prototyp fertig' bedeutet".

Zwei Teile:

- **A · Automatisiert** mit der echten Desktop-App (Debug-Build: WebKitGTK, Tauri-IPC, SQLite, Sync-Engine,
  Scheduler, UI). Zwei Profile (`TODO_DATA_DIR`) synchronisieren über ein gemeinsames Verzeichnis
  (`TODO_REMOTE_DIR`, nur Debug-Builds, `apps/desktop/src-tauri/src/dev.rs`). Jeder App-Lauf führt eine
  Selbsttest-Suite aus (`apps/desktop/src/selftest.ts`, `p802*`) und beendet sich; das Skript startet und
  beendet die Läufe (auch per `kill -9`) und vergleicht beide Geräte.
  Wiederholbar: `tools/acceptance-two-devices.sh [ORDNER]`.
- **B · Live über Proton Drive** von Hand (Mensch am Rechner, zwei Profile, eigenes Testkonto).
  Unterschiede zu A: echtes Backend (Latenz, Änderungs-Feed, Proton-E2E), echte Anmeldung/Abmeldung.

## A · Automatisiert – Ergebnis 2026-10-18: **alle Punkte bestanden**

Rechner: Arch/CachyOS, KDE (Wayland), AMD; Debug-Build `todo-desktop` auf Stand nach P8-01. Dauer ~2 min.

| # | Punkt (Roadmap) | Umsetzung | Ergebnis |
|---|---|---|---|
| 1 | Anlegen auf A → erscheint auf B | B läuft und wartet; A legt per Quick-Add an + Liste „Abnahme" | ✔ B zeigt beides ohne Neustart – im protokollierten Lauf per periodischem Pull nach 120 s (nach zwei Läufen beim Start, davon einer ohne Änderungen → Poll-Abstand verdoppelt, wie vorgesehen); in einem früheren Lauf per Fokuswechsel beim Schließen von A nach ~0,1 s |
| 2 | Beide offline: Titel (A) / Fälligkeit (B), Löschen (A) / Bearbeiten (B), beide sortieren | WebView-`offline`; je 3 Ops bleiben lokal; dann A, B, A online | ✔ identisch; T = Titel von A + Fälligkeit von B; X gelöscht (Delete gewinnt); S4 oben (A), S1 unten (B) |
| 3 | Hart beenden während des Syncs | 2 500 Tasks offline, online, `kill -9` nach 1,5 s (Outbox 1 001 ausstehend, Teil-Upload) und erneut im Start-Sync nach Neustart (1 ausstehend) | ✔ beide Geräte genau 2 500 Tasks, keine Duplikate, Liste einmal |
| 4 | Sitzung abgelaufen | `.auth-revoked` im Sync-Ordner → Runner meldet `AuthRequired` | ✔ „Anmeldung erforderlich", Quick-Add funktioniert, Status bleibt; nach „erneuter Anmeldung" kommt die Task auf B an |
| 6 | Abhaken + automatischer Upload (P1-03, 2026-10-18) | A: „Abhaken-Test" per Quick-Add, im Eingang am runden Kästchen abhaken, **ohne** manuellen Sync warten; B synchronisiert | ✔ Zeile verlässt den Eingang, oben nach 1 520 ms; B: erledigt, nicht im Eingang. Abschnitt 3 dabei robuster: Neustart-Kill mit 1 s Backend-Verzögerung (bei 400 ms war auf schneller Maschine der Start-Sync vor dem Kill fertig) |
| 7 | Listen & Board (P2-09, 2026-10-18, alle 7 Abschnitte grün) | A: Listen „Obst"/„Zweite", Task „Äpfel" im Eingang; im Board „Äpfel" in die Spalte „Obst" gezogen, „Zweite" per Kontextmenü in „Haushalt" umbenannt und per `Alt+↑` vor „Obst" sortiert; Sync; B synchronisiert; A neu gestartet | ✔ A, B und Neustart: „Haushalt \| Obst · Äpfel in Obst" |
| 5 | Mehrfachauswahl (P0-07, Lauf 2026-10-18, alle 5 Abschnitte erneut grün) | A: Liste „Sammel" mit 200 Tasks; über die UI 50 per `Umschalt+↓` wählen und `Entf`, 50 weitere per `M` nach „Ziel", Block S101+S103 per `Alt+↓`; Sync; B synchronisiert; A neu gestartet (ohne Sync) | ✔ B identisch (Reihenfolge, beide Listen); nach Neustart unverändert; „Sammel" 100 (S102 S104 S101 S103 S105 …), „Ziel" 50 in Originalreihenfolge |

Keine Bugs gefunden. Grenzen von Teil A: Backend ist ein lokales Verzeichnis (atomar, schnell, kein
Änderungs-Feed), „Netz weg" ist das WebView-Ereignis, „Sitzung abgelaufen" simuliert – daher Teil B.

<details><summary>Protokoll des Laufs</summary>

```
== Build (Debug) ==

== 1. Anlegen auf A → erscheint auf B ==
  [A] PASS Quick-Add „Von A“ im Eingang
  [A] PASS Liste „Abnahme“ mit T, X, S1–S4
  [A] PASS Hochgeladen – 27 ms, vorher 8 ausstehend
  [A] PASS Zustand – Reihenfolge T Konflikt | X Löschen | S1 | S2 | S3 | S4 · T=T Konflikt fällig -
  [A] ALLE BESTANDEN
  [B] PASS B bereit: synchronisiert – 27 ms, vorher 0 ausstehend
  [B] PASS „Von A“ erscheint im Eingang (ohne Zutun, bis 150 s) – nach 120 s
  [B] PASS Abnahme-Liste ebenfalls da – Reihenfolge T Konflikt | X Löschen | S1 | S2 | S3 | S4 · T=T Konflikt fällig -
  [B] ALLE BESTANDEN
  ✔ A legt an und lädt hoch
  ✔ B sieht „Von A“ und die Liste ohne Neustart

== 2. Beide offline: Titel/Fälligkeit, Löschen/Bearbeiten, Umsortieren → wieder online ==
  [A] PASS Netz getrennt (WebView-Ereignis) → Status Offline
  [A] PASS A: T umbenennen, X löschen, S4 nach oben – 3 ausstehend
  [A] PASS Zustand A (offline) – Reihenfolge S4 | T neu von A | S1 | S2 | S3 · T=T neu von A fällig -
  [A] ALLE BESTANDEN
  [B] PASS Netz getrennt (WebView-Ereignis) → Status Offline
  [B] PASS B: T fällig setzen, X bearbeiten, S1 ans Ende – 3 ausstehend
  [B] PASS Zustand B (offline) – Reihenfolge T Konflikt | X bearbeitet von B | S2 | S3 | S4 | S1 · T=T Konflikt fällig 2026-10-25
  [B] ALLE BESTANDEN
  ✔ Offline-Änderungen bleiben lokal (nichts hochgeladen)
  [A] PASS Online, synchronisieren – 28 ms, vorher 0 ausstehend
  [A] PASS Zustand – Reihenfolge S4 | T neu von A | S1 | S2 | S3 · T=T neu von A fällig -
  [A] ALLE BESTANDEN
  [B] PASS Online, synchronisieren – 28 ms, vorher 0 ausstehend
  [B] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [B] ALLE BESTANDEN
  [A] PASS Online, synchronisieren – 27 ms, vorher 0 ausstehend
  [A] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [A] ALLE BESTANDEN
  A: Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  B: Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  ✔ beide Geräte identisch
  ✔ Titel von A und Fälligkeit von B bleiben beide (feldweises LWW)
  ✔ X gelöscht – Löschen gewinnt gegen Bearbeiten
  ✔ Umsortierungen beider Geräte (S4 nach oben, S1 ans Ende)

== 3. App während des Syncs hart beenden (kill -9) → Neustart ==
  [A] PASS Netz getrennt (WebView-Ereignis) → Status Offline
  [A] PASS 2500 Tasks in Liste „Masse“ (offline)
  [A] kill -9 1,5 s nach Sync-Start (Backend 400 ms je Aufruf) → Outbox 1001 ausstehend, 7 Segmente im Sync-Ordner
  ✔ Abbruch traf einen laufenden Sync
  [A] kill -9 2,5 s nach Neustart (Start-Sync) → Outbox 1 ausstehend, 9 Segmente im Sync-Ordner
  ✔ Abbruch traf einen laufenden Sync
  [A] PASS Online, synchronisieren – 28 ms, vorher 0 ausstehend
  [A] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [A] ALLE BESTANDEN
  [B] PASS Online, synchronisieren – 56 ms, vorher 0 ausstehend
  [B] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [B] ALLE BESTANDEN
  Masse: A 2500 Tasks (2500 verschiedene Titel), B 2500 Tasks (2500 verschiedene Titel)
  ✔ A: alle 2 500 Tasks, keine verloren
  ✔ B: alle 2 500 Tasks, keine Duplikate
  ✔ beide Geräte identisch
  ✔ Liste „Masse“ genau einmal

== 4. Sitzung abgelaufen → lokal nutzbar, Status „Anmeldung erforderlich“ ==
  [A] PASS Status „Anmeldung erforderlich“ – Sync-Status: Anmeldung erforderlich
  [A] PASS Lokal weiterarbeiten: Quick-Add „Ohne Sitzung“ – 1 ausstehend, Status bleibt authRequired
  [A] ALLE BESTANDEN
  ✔ A zeigt „Anmeldung erforderlich“ und arbeitet lokal weiter
  [A] PASS Online, synchronisieren – 30 ms, vorher 0 ausstehend
  [A] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [A] ALLE BESTANDEN
  [B] PASS Online, synchronisieren – 27 ms, vorher 0 ausstehend
  [B] PASS Zustand – Reihenfolge S4 | T neu von A | S2 | S3 | S1 · T=T neu von A fällig 2026-10-25
  [B] ALLE BESTANDEN
  ✔ nach erneuter Anmeldung: B sieht die lokal angelegte Task

ERGEBNIS: alle Prüfungen bestanden
Protokoll: <ordner>/protocol.txt
```
</details>


## B · Live über Proton Drive (von Hand)

Vorbereitung (einmal):

```sh
just desktop-build                      # Release-Binary target/release/todo-desktop
mkdir -p ~/tasks-A ~/tasks-B
# Zwei Terminals; jedes Profil hat eigene DB und eigene Sitzung im Schlüsselbund (ADR-020).
# Logs (seit 95ff2bf über `tracing`, ohne Inhalte/Tokens) mitschneiden – bei Funden die Logs mitschicken:
TODO_LOG=info,proton_adapter=debug TODO_DATA_DIR=~/tasks-A target/release/todo-desktop 2>&1 | tee ~/tasks-A/live.log
TODO_LOG=info,proton_adapter=debug TODO_DATA_DIR=~/tasks-B target/release/todo-desktop 2>&1 | tee ~/tasks-B/live.log
```

`proton_adapter::http` loggt auf `debug` je Anfrage Methode, **Routen-Vorlage** (z. B. `/drive/shares/{id}/…`), Status
und Dauer – keine IDs, Tokens oder Inhalte (ADR-018). Vor dem Weitergeben trotzdem kurz durchsehen.

| # | Schritt | Erwartung | Ergebnis |
|---|---|---|---|
| 0a | A: ohne Login 2 Tasks anlegen, dann Einstellungen → „Mit Proton anmelden" | Browser-Login; danach „Synchronisiert", beide Tasks bleiben | offen |
| 0b | B: ohne Login 1 Task anlegen, dann anmelden | B zeigt eigene + A's Tasks, **eine** Inbox (P8-01) | offen |
| 1 | A: Task „Live 1" anlegen; zu B wechseln (Fokus) | erscheint auf B nach wenigen Sekunden | ✔ User 2026-10-18: „Die Synchronisation zwischen zwei Geräten funktioniert" (Details/übrige Zeilen noch offen) |
| 2 | Netz trennen (z. B. `nmcli networking off`); A: Titel von T ändern, X löschen, S4 nach oben; B: Fälligkeit von T, X bearbeiten, S1 ans Ende; Netz an | Status „Offline · n ausstehend" → nach Netz an: beide gleich; T mit Titel (A) **und** Fälligkeit (B); X gelöscht; S4 oben, S1 unten | offen |
| 3 | A: 200 Tasks anlegen (z. B. per Quick-Add-Schleife im Dev-Build oder `todo-cli` auf `~/tasks-A/tasks.db` bei geschlossener App), App öffnen, während „Synchronisiere …" `kill -9 $(pgrep -f 'todo-desktop$' \| head -1)`; neu starten | keine Verluste, keine Duplikate; B zeigt alle | offen |
| 4 | B: Einstellungen → Abmelden; dann Task anlegen | „Nur auf diesem Gerät", App bleibt nutzbar; nach erneutem Login wird die Task hochgeladen | offen |
| 4b | Sitzung serverseitig beenden (Proton-Web → Sicherheit → Sitzungen → B abmelden), B fokussieren | Status „Anmeldung erforderlich", lokal weiter nutzbar | offen |

**Messung angemeldet** (N0-01, nach Schritt 1, App A geschlossen; Ergebnis → `architecture.md` §6):

```sh
TODO_DATA_DIR=~/tasks-A RUNS=1 IDLE_S=60 CPU_S=300 tools/measure-desktop.sh
```

Danach Sitzungen der Testprofile im Proton-Konto beenden und die Schlüsselbund-Einträge
`dev.tasks.app@…/tasks-A|B` entfernen (Abmelden in der App erledigt beides).

## C · Mehrfachauswahl & Sammelaktionen (P0-07) – Abnahme durch den User am Desktop: **bestanden** (2026-10-18)

Release-Build (`just desktop-build`), eigenes Profil mit Testdaten, z. B. `TODO_DATA_DIR=~/tasks-P0
target/release/todo-desktop`; > 200 Tasks in einer Liste (z. B. per `todo-cli --db ~/tasks-P0/tasks.db add …` bei
geschlossener App). Automatisch geprüft ist das schon (Selbsttest `p0sel`, `p0perf`, Abnahmeskript Abschnitt 5) – hier
geht es um Gefühl, Optik und Bedienbarkeit. Funde bitte als `FIX-nn` in `roadmap.md` (oder einfach melden).

| # | Schritt | Erwartung | Ergebnis |
|---|---|---|---|
| 1 | Kopfzeile: „Auswählen" (Icon mit Häkchenliste) klicken | Aktionsleiste ersetzt die Kopfzeilen-Aktionen ohne Höhensprung; eckige Kästchen links in jeder Zeile; „0 ausgewählt" | ✔ |
| 2 | Zeilen anklicken | schaltet Auswahl um (Auswahlfarbe), öffnet **kein** Detail; rundes Erledigt-Kästchen hakt weiter nur diese eine Task ab | ✔ |
| 3 | „Fertig", dann erneut per `Esc` | Modus endet, Auswahl weg | ✔ |
| 4 | Ohne Modus: Zeile anklicken (Detail öffnet), dann `Strg`+Klick auf eine andere | Modus startet mit beiden Zeilen, Detail schließt | ✔ |
| 5 | `Umschalt`+Klick weiter unten | Bereich vom Anker bis zur Zeile kommt hinzu, kein Text wird markiert | ✔ |
| 6 | `Esc`; Zeile fokussieren, `Umschalt+↓` mehrfach, dann `X`/`Leertaste` | Auswahl wächst mit dem Cursor; `X`/`Leertaste` schalten die Cursor-Zeile um | ✔ |
| 7 | `Strg+A` in der Liste (> 200 Tasks), nochmal `Strg+A`; `Strg+A` im Quick-Add-Feld | alle (Zähler = ganze Liste), dann keine; im Textfeld markiert es nur den Text | ✔ |
| 8 | Einige wählen → „Erledigt" (bzw. `E`) → „Rückgängig" im Toast | „n Tasks erledigt"; Rückgängig öffnet genau diese wieder | ✔ |
| 9 | Einige (auch verstreut) → „Verschieben nach…" (bzw. `M`) → andere Liste → „Rückgängig" | Menü: Eingang zuerst, aktuelle Liste ausgegraut; Tasks verschwinden; Rückgängig bringt jede an ihre alte Stelle | ✔ |
| 10 | Alle wählen → `Entf` → „Rückgängig" | Liste sofort leer; Rückgängig bringt alle in alter Reihenfolge zurück | ✔ |
| 11 | Zwei verstreute wählen → `Alt+↓`/`Alt+↑`; dann eine ausgewählte am Griff ziehen | Block wird zusammengezogen und wandert um eine Position; Ziehen zeigt Badge „2" und Einfügelinie | ✔ |
| 12 | Rechtsklick auf eine ausgewählte bzw. eine nicht ausgewählte Zeile | „n Tasks …"-Einträge bzw. die bisherigen Einzelaktionen | ✔ |
| 13 | In „Alle Tasks"/„Heute": auswählen, Rechtsklick → „Nach oben" | deaktiviert mit Tooltip „Nur im Eingang und in Listen sortierbar"; übrige Aktionen gehen | ✔ |
| 14 | Ansicht wechseln während Auswahl aktiv | Modus endet | ✔ |
| 15 | Hell/Dunkel umschalten im Modus | Kästchen, Auswahlfarbe und Cursor-Balken gut unterscheidbar | ✔ |

## D · Listen & Kanban-Board (P2-09) – Abnahme durch den User am Desktop: **bestanden** (2026-10-18)

Installierte App (`tasks-app` bzw. Starter „Tasks", Stand nach P2). Automatisch geprüft ist das schon (Selbsttests `p2board`,
`p2layout`, `p2perf`, Abnahmeskript Abschnitt 7) – hier geht es um Gefühl, Optik und Bedienbarkeit.

| # | Schritt | Erwartung | Ergebnis |
|---|---|---|---|
| 1 | Seitenleiste: „+" bei Listen, „Einkaufen" anlegen; mit `Esc` eine zweite abbrechen | Liste erscheint und ist geöffnet; abgebrochene wird nicht angelegt | ✔ |
| 2 | Rechtsklick auf „Einkaufen" → Umbenennen | Feld inline in der Seitenleiste, `Enter` speichert, `Esc` bricht ab | ✔ |
| 3 | In einer Liste: „⋯" in der Kopfzeile → Löschen… | Dialog nennt offene/erledigte Tasks, die in den Eingang wandern; Abbrechen lässt alles stehen | ✔ |
| 4 | Liste am Griff ziehen bzw. fokussieren und `Alt+↑/↓` | Reihenfolge ändert sich, Eingang bleibt oben | ✔ |
| 5 | Quick-Add: Button „Eingang ▾" bzw. `Alt+L` → „Einkaufen", Task anlegen | Task landet in „Einkaufen", Toast „… in „Einkaufen" angelegt · Öffnen" | ✔ |
| 6 | Detail-Panel einer Task: Liste über den Picker ändern | Task wandert in die gewählte Liste | ✔ |
| 7 | Kopfzeile: Umschalter „Liste | Kanban" (bzw. `V`) | Board mit allen Listen nebeneinander, gewählte Liste hervorgehoben; nach Neustart weiter Kanban | ✔ |
| 8 | Karte in eine andere Spalte ziehen, dann „Rückgängig" | Karte wechselt die Liste (auch in der Listenansicht sichtbar), Rückgängig bringt sie an den alten Platz | ✔ |
| 9 | Karte innerhalb der Spalte ziehen; `Esc` während des Ziehens | Reihenfolge ändert sich; `Esc` bricht ab | ✔ |
| 10 | Tastatur im Board: Pfeile/HJKL, `Enter`, `Leertaste`, `Alt+←/→` | Cursor wandert, Detail öffnet, abgehakte Karte verschwindet, Karte wechselt die Spalte | ✔ |
| 11 | Spaltenkopf ziehen bzw. fokussieren und `Alt+←/→` | Spaltenreihenfolge ändert sich – auch in der Seitenleiste | ✔ |
| 12 | Hell/Dunkel und schmales Fenster im Board | Spalten, Karten, Hervorhebung gut lesbar; Board scrollt horizontal | ✔ |

