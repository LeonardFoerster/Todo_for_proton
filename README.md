# Tasks

Eine schnelle, ressourcenschonende Task-App, die deine Aufgaben **offline-first** auf dem Gerät hält und sie
**Ende-zu-Ende-verschlüsselt über Proton Drive** zwischen deinen Geräten synchronisiert. Die Oberfläche ist an
Proton Mail/Drive angelehnt (ruhig, dicht, gute Hell-/Dunkel-Darstellung), aber eine eigenständige Umsetzung.

> **Hinweis:** Tasks ist ein unabhängiges Drittanbieter-Projekt und wird von Proton AG weder entwickelt noch
> unterstützt. „Proton" und „Proton Drive" sind Marken der Proton AG. Die App zeigt beim Anmelden einen entsprechenden
> Hinweis.

**Status:** Prototyp (v0.1.0). Die Desktop-App für Linux ist voll benutzbar und abgenommen; Web, Windows, Android und
iOS folgen (siehe [Plattformen](#plattformen) und [Roadmap](#roadmap)).

---

## Inhalt

- [Funktionen](#funktionen)
- [Plattformen](#plattformen)
- [Sync und Sicherheit](#sync-und-sicherheit)
- [Architektur](#architektur)
- [Aus dem Quellcode bauen (Arch Linux)](#aus-dem-quellcode-bauen-arch-linux)
- [Entwicklung](#entwicklung)
- [Tastaturkürzel](#tastaturkürzel)
- [Performance](#performance)
- [Roadmap](#roadmap)
- [Mitmachen](#mitmachen)
- [Lizenz](#lizenz)

---

## Funktionen

**Aufgaben**
- Tasks mit Titel, Notizen, Priorität und Fälligkeit (Datum oder Datum + Uhrzeit mit Zeitzone).
- Fenster „Neue Task" (`N`) mit natürlicher Eingabe: „Übungsblatt 3 **bis Fr 14:00 !1 @uni**" erkennt Tag
  (`morgen`, `Fr`, `24.10.`), Uhrzeit, Priorität (`!1`–`!3`) und Liste; dazu Chips zum Nachjustieren, optionale Notiz
  und „Weitere anlegen" für mehrere Tasks am Stück.
- Ansichten: **Eingang, Heute, Geplant, Alle Tasks, Erledigt** – plus eigene Listen. Abgehakte Tasks verlassen die
  offenen Ansichten und stehen unter „Erledigt"; jede Aktion lässt sich im Hinweis unten links **rückgängig** machen.
- Manuelle Reihenfolge per Drag & Drop oder `Alt+↑/↓`.

**Listen**
- Eigene Listen (z. B. „Einkaufen", „Arbeit") anlegen, umbenennen, löschen (ihre Tasks wandern in den Eingang) und
  umsortieren. Die Liste lässt sich schon beim Anlegen einer Task wählen.

**Kanban-Board**
- Umschalter **Liste | Kanban** (`V`): alle Listen als Spalten nebeneinander, Tasks als Karten.
- Karten per Drag & Drop (oder `Alt+←/→`) in eine andere Liste ziehen – das ändert die tatsächliche Zuordnung und
  wird synchronisiert. Spalten lassen sich am Kopf umsortieren; `Umschalt` + Mausrad scrollt waagerecht.

**Mehrfachauswahl**
- Auswahlmodus mit `Strg`/`Umschalt`+Klick, `Umschalt+↑/↓` oder `Strg+A` (ganze Ansicht).
- Sammelaktionen für die Auswahl: erledigen, verschieben, löschen, als Block umsortieren – jeweils als **ein**
  Vorgang mit Rückgängig.

**Notes**
- Zweiter Modus neben den Tasks (`Strg+1` / `Strg+2` oder der Umschalter oben in der Seitenleiste): Notizen als
  `.md` oder `.txt` in einem Ordnerbaum, beliebig tief, mit Drag & Drop, Papierkorb (30 Tage) und Rückgängig.
- Editor mit Autosave, Markdown-Hilfen (Listen fortsetzen, Einrücken, `Strg+B/I/L`, Überschriften, Checkboxen),
  Vorschau (`Strg+E`) mit anklickbaren Checklisten, Zen-Modus und Wort-/Zeichenzählung.
- Die Befehlspalette (`Strg+K`) durchsucht Tasks und Notizen (Namen und Inhalt) gemeinsam.
- Gleichzeitige Bearbeitung auf mehreren Geräten wird zeilenweise zusammengeführt; ändern zwei Geräte dieselbe
  Stelle, bleibt die andere Fassung als Kopie „… (Konflikt TT.MM.JJJJ)" erhalten – nichts geht verloren.
- Desktop: „Ordner importieren…" übernimmt einen vorhandenen Notizordner (z. B. ein Obsidian-Vault) als Kopie.

**Allgemein**
- Komplett per Tastatur bedienbar (Übersicht mit `?`), Fokus immer sichtbar, Screenreader-Beschriftungen.
- Funktioniert ohne Netz und ohne Konto; der Sync ist optional.
- Hell/Dunkel folgt dem System, einklappbare Seitenleiste, responsives Layout bis Handybreite.

## Plattformen

| Plattform | Stand | Technik |
|---|---|---|
| **Linux** (Arch zuerst) | ✅ verfügbar | Tauri 2 (WebKitGTK), Rust-Core nativ, SQLite, Schlüsselbund über Secret Service |
| **Web** | 🔜 nächste Etappe | Svelte-UI im Browser, Rust-Core als **WebAssembly im Web Worker**, Speicher im Browser (OPFS/IndexedDB – wird per Spike entschieden) |
| **Windows** | 📋 geplant | Tauri 2 (WebView2), gleiche UI und gleicher Core; Schlüssel im Windows-Anmeldeinformationsspeicher |
| **Android** | 📋 geplant | Tauri 2 Mobile (System-WebView), gleiche UI; Schlüssel im Android Keystore |
| **iOS** | 📋 geplant | Tauri 2 Mobile (WKWebView), gleiche UI; Schlüssel im iOS-Schlüsselbund |

Alle Plattformen teilen sich **eine** Svelte-Oberfläche (`packages/ui`) und **einen** Rust-Kern (`crates/todo-core`,
`crates/todo-sync`). Plattformspezifisch sind nur dünne Hüllen: Speicher, Schlüsselablage und die Anbindung an die UI.
Der Kern ist dafür von Anfang an ohne Linux-spezifische APIs, ohne eigene Laufzeit und mit FFI-freundlicher Schnittstelle
gebaut.

## Sync und Sicherheit

- **Offline-first:** Jede Änderung wird sofort lokal in einer Transaktion gespeichert (Zustand + Änderungsprotokoll)
  und ist sofort sichtbar. Netzwerk passiert ausschließlich im Hintergrund.
- **Synchronisation über Proton Drive:** Jedes Gerät schreibt sein eigenes Änderungsprotokoll (Ops) in einen Ordner im
  Drive; die Geräte führen die Änderungen feldweise zusammen (Hybrid Logical Clocks, „letzte Änderung je Feld gewinnt",
  Löschen gewinnt gegen Bearbeiten, Reihenfolge per Fractional Index). Zwei Geräte, die offline unterschiedliche
  Felder derselben Task ändern, behalten beide Änderungen. Snapshots halten das Protokoll klein.
- **Notizen** synchronisieren als Folge unveränderlicher Revisionen; parallele Änderungen werden zeilenweise
  zusammengeführt (diff3), Überschneidungen landen in einer Konfliktkopie.
- **Alle Geräte gleichzeitig aktualisieren:** Neue Versionen können neue Arten von Änderungen schreiben (z. B. die
  Notizen ab N5), die ältere Versionen nicht lesen können. Ein Gerät mit alter Version stockt dann beim Sync, bis
  es aktualisiert ist – lokale Daten bleiben erhalten.
- **Zeitnah:** Nach einer Änderung wird nach ~1,5 s hochgeladen, bei vielen Änderungen hintereinander spätestens nach
  5 s; andere Geräte holen beim Fokuswechsel und periodisch ab. Keine Timer im Leerlauf, Backoff bei Fehlern.
- **Ende-zu-Ende-verschlüsselt:** Zusätzlich zu Protons eigener Verschlüsselung sind alle Sync-Daten mit
  **XChaCha20-Poly1305** und einem App-eigenen Schlüssel verschlüsselt; der Schlüssel liegt im System-Schlüsselbund.
- **Anmeldung im Browser:** Passwort, Zwei-Faktor und Prüfungen laufen auf der Anmeldeseite von Proton im Browser –
  die App sieht dein Passwort nie. Die Sitzung liegt im System-Schlüsselbund.
- **Isoliert:** Proton-spezifischer Code steckt ausschließlich in `crates/proton-adapter`; der Rest der App kennt nur
  allgemeine Schnittstellen (`AuthProvider`, `RemoteStore`). Ein anderes Sync-Ziel ließe sich als weiterer Adapter
  anschließen.

## Architektur

Hexagonal (Ports & Adapters): Der Kern enthält Domain-Modell, Anwendungsfälle und Schnittstellen und hat **keine**
Abhängigkeit auf IO, Netzwerk, UI oder Proton. Abhängigkeiten zeigen nach innen; verdrahtet wird nur in den App-Hüllen.

```
apps/desktop (Tauri) ─┐                     ┌─ todo-storage-sqlite (Speicher)
apps/web (später)  ───┼─► packages/ui ─► packages/core-client ─► todo-core ◄─┼─ todo-sync (Sync-Engine)
                      │                     (IPC | WASM)                 └─ proton-adapter (Login, Drive)
```

| Pfad | Inhalt |
|---|---|
| `crates/todo-core` | Domain (`Task`, `TaskList`, Sort-Keys, Fälligkeit), Anwendungsfälle, Port-Traits, Sync-Modell (HLC, Ops, Merge), FFI-freundliche Fassade `CoreApi` |
| `crates/todo-sync` | Sync-Engine: Segmente, Push/Pull, Snapshots, Scheduler (Debounce, Backoff, ereignisgetrieben) |
| `crates/todo-storage-sqlite` | Speicher auf SQLite (WAL, eigener DB-Thread) |
| `crates/todo-crypto` | Verschlüsselung der Sync-Daten (XChaCha20-Poly1305) |
| `crates/todo-keystore-secret-service` | Schlüsselablage im Linux-Schlüsselbund (Secret Service) |
| `crates/proton-adapter` | Anmeldung (Browser-Session) und Proton-Drive-Zugriff |
| `crates/todo-remote-fs` | Sync gegen ein lokales Verzeichnis (Entwicklung, Tests) |
| `crates/todo-wasm` | WebAssembly-Bindings für die Web-App |
| `crates/todo-cli` | Entwickler-CLI (Core + SQLite ohne UI) |
| `apps/desktop` | Tauri-2-Hülle für Linux (Commands → Core, Sync-Treiber) |
| `apps/web` | Einstieg der Web-App (Vite + Svelte) |
| `packages/ui` | Gemeinsame Svelte-5-Oberfläche und Design-Tokens |
| `packages/core-client` | TypeScript-Schnittstelle `CoreClient` mit Tauri- und Test-Implementierung; DTO-Typen aus Rust generiert |
| `tools` | Mess- und Abnahmeskripte |

Technik: **Rust** (Kern, Sync, Speicher, Krypto), **Tauri 2**, **Svelte 5** mit eigenem kleinen Design-System (keine
UI-Kits), **SQLite**, **Vite**, **pnpm**, **just**.

## Aus dem Quellcode bauen (Arch Linux)

### Schnellinstallation

```sh
git clone git@github.com:LeonardFoerster/Todo_for_proton.git
cd Todo_for_proton
./install.sh
```

`install.sh` installiert fehlende Systempakete per `pacman` (fragt nach dem sudo-Passwort), die Rust-Toolchain aus
`rust-toolchain.toml` und die JavaScript-Abhängigkeiten, baut die App und installiert sie wie `just desktop-install`.
Erneut ausführen ist unschädlich und aktualisiert die App; `--noconfirm` reicht an `pacman` durch.

### Voraussetzungen

```sh
sudo pacman -S --needed base-devel git rustup nodejs pnpm just webkit2gtk-4.1 openssl librsvg desktop-file-utils
```

- Die Rust-Version ist über `rust-toolchain.toml` festgelegt; `rustup` lädt sie beim ersten Build automatisch.
- Für Anmeldung und Sync braucht es einen laufenden **Secret Service** (GNOME Keyring oder KWallet). Ohne
  Schlüsselbund funktioniert die App lokal, nur der Sync bleibt aus.

### Bauen und starten

```sh
git clone git@github.com:LeonardFoerster/Todo_for_proton.git
cd Todo_for_proton
pnpm install

just desktop-dev       # Entwicklungsmodus mit Hot-Reload
just desktop-build     # Release-Binary: target/release/todo-desktop
just desktop-install   # für den aktuellen Benutzer installieren/aktualisieren
```

`just desktop-install` legt die App ohne `sudo` unter `~/.local` ab (`~/.local/bin/tasks-app`, Starter „Tasks" im
App-Menü, Icons). Daten, Schlüsselbund und Sitzung bleiben bei einer Aktualisierung erhalten. Entfernen:
`tools/install-desktop.sh --uninstall`.

### Umgebungsvariablen

| Variable | Wirkung |
|---|---|
| `TODO_DATA_DIR` | Eigenes Datenverzeichnis (zweites Profil, inkl. eigenem Schlüsselbund-Eintrag) |
| `TODO_WEBKIT_COMPOSITING=1` | WebKit-Compositing wieder einschalten (standardmäßig aus – spart Speicher) |
| `TODO_LOG` | Log-Filter für stderr, z. B. `debug` oder `info,proton_adapter::http=debug` (Standard `info`) |
| `TODO_REMOTE_DIR` | *nur Debug-Builds:* Sync gegen ein lokales Verzeichnis statt Proton Drive |

## Entwicklung

```sh
just --list    # alle Rezepte
just check     # Standard-Prüfung: Formatierung, Clippy, Rust-Tests, Doku, WASM, ESLint, Typecheck, Vitest
just test      # Rust-Tests (inkl. Doctests, Contract- und Mehrgeräte-Tests)
just ts        # TypeScript/Svelte: ESLint, Typecheck, Vitest
just types     # TS-Typen der DTOs aus Rust neu erzeugen
```

Pre-commit-Hook einmal pro Klon aktivieren: `git config core.hooksPath .githooks`.

**Tests**
- Rust: Unit-Tests je Anwendungsfall, Property-Tests für Sortierung und Merge (Konvergenz), Contract-Tests für jeden
  Speicher und jedes Sync-Ziel, Mehrgeräte-Szenarien mit echten SQLite-Dateien.
- UI: Vitest mit einem In-Memory-`CoreClient`, der die Semantik des Kerns nachbildet.
- **Selbsttest in der echten App:** Debug-Builds führen auf Wunsch Testsuiten direkt in der WebView aus
  (`apps/desktop/src/selftest.ts`, Start über `TASKS_SELFTEST=<suite>`) – ohne Eingaben am Desktop.
- **Zwei-Geräte-Abnahme:** `tools/acceptance-two-devices.sh` startet zwei Profile, die über ein gemeinsames Verzeichnis
  synchronisieren, inklusive Offline-Konflikten, hartem Abbruch während des Syncs, abgelaufener Sitzung, Sammelaktionen
  und Board-Verschiebungen.
- **Messung:** `tools/measure-desktop.sh` (Startzeit, Speicher als PSS über alle Prozesse, Leerlauf-CPU).

**Projektregeln** (Architektur, Benennung, Performance-Budgets, Arbeitsweise) stehen in [`AGENTS.md`](AGENTS.md).
Planungs- und Entscheidungsdokumente (ADRs, Roadmap) stehen in `development/`.

## Tastaturkürzel

| Aktion | Kürzel |
|---|---|
| Neue Task | `N`, `Strg+N` |
| Im Fenster „Neue Task": Fälligkeit / Priorität / Liste / Notiz | `Alt+D` / `Alt+P` / `Alt+L` / `Alt+N` |
| Navigieren / Öffnen | `↑/↓` bzw. `J/K` / `Enter` |
| Erledigt umschalten / Löschen | `Leertaste` bzw. `E` / `Entf` |
| Umsortieren | `Alt+↑/↓` |
| Liste ↔ Kanban | `V` |
| Board: Spalte wechseln / Karte in Nachbarspalte | `←/→` bzw. `H/L` / `Alt+←/→` |
| Auswahl: alle / erweitern / umschalten | `Strg+A` / `Umschalt+↑/↓` / `X` |
| Ansichten | `G` dann `I` (Eingang), `T` (Heute), `U` (Geplant) |
| Modus Tasks / Notes | `Strg+1` / `Strg+2` |
| Befehlspalette und Suche (Tasks + Notizen) | `Strg+K` |
| Notes: neue Notiz / Umbenennen / Verschieben | `N` bzw. `Strg+N` / `F2` / `M` |
| Notes: Bearbeiten ↔ Vorschau / Zen | `Strg+E` / `Strg+Umschalt+F` |
| Alle Kürzel | `?` |

## Performance

Budgets (Release-Build, Mittelklasse-Laptop) und gemessene Werte:

| Metrik | Ziel | Gemessen |
|---|---|---|
| Kaltstart bis zur ersten Liste | < 400 ms | 382 ms |
| Speicher gesamt im Leerlauf (PSS inkl. WebKit) | < 320 MB | 306 MB |
| Leerlauf-CPU | ~0 % | 0,02 % |
| Abhaken bis Neuzeichnen | < 16 ms | nächster Frame |
| Kanban mit 10 Listen à 200 Tasks zeichnen | – | ~75 ms (Debug-Build) |
| 1 000 Tasks löschen bzw. verschieben (eine Aktion) | < 50 ms | 17–28 ms |
| Start im Modus Notes / Mehrspeicher | < 400 ms | 355 ms / +2 MB |
| Notes: Tippen bis Neuzeichnen | < 16 ms | nächster Frame (1 ms Arbeit) |

Listen sind virtualisiert, der Austausch zwischen Oberfläche und Kern ist grob-granular (Seiten statt einzelner Tasks,
eine Sammelaktion = ein Aufruf), und im Leerlauf läuft nichts.

## Roadmap

1. **Linux-Desktop** – ✅ Prototyp fertig (lokal, Sync, Listen, Kanban, Mehrfachauswahl); ✅ Modus Notes.
2. **Abschluss Prototyp** – Live-Abnahme über Proton Drive, kleinere Nachträge.
3. **Web-App** – Kern als WebAssembly im Web Worker, Speicher im Browser, Sync aus dem Browser.
4. **Windows** – Tauri 2 mit WebView2, Installer, Schlüsselablage im Windows-Anmeldeinformationsspeicher.
5. **Android** – Tauri 2 Mobile, Android Keystore, Touch-Bedienung (Zeilenhöhe 44 px, Gesten).
6. **iOS** – Tauri 2 Mobile, iOS-Schlüsselbund, Verteilung über TestFlight/App Store.
7. **Weitere Funktionen** (Kandidaten) – Tags, Unteraufgaben, wiederkehrende Tasks, Erinnerungen, Volltextsuche,
   Befehlspalette, Pakete für Linux-Distributionen (AUR, Flatpak).

## Mitmachen

Issues und Vorschläge sind willkommen. Vor Änderungen bitte [`AGENTS.md`](AGENTS.md) lesen – dort stehen die
verbindlichen Architektur- und Performance-Regeln. Jede Änderung muss `just check` bestehen; Commits folgen
[Conventional Commits](https://www.conventionalcommits.org/) (`feat(core): …`, `fix(ui): …`).

Bitte keine Zugangsdaten, Tokens oder Testkonten in Code, Tests oder Issues.

## Lizenz

Noch nicht festgelegt. Bis eine Lizenz hinzugefügt ist, gelten die gesetzlichen Urheberrechte des Autors.
