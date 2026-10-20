# Technologie-Entscheidungen (ADR-Log)

Format pro Eintrag: Kontext → Entscheidung → Begründung → Konsequenzen. Status: `akzeptiert`,
`vorläufig` (bis Spike), `ersetzt durch ADR-x`. Neue Entscheidungen unten anhängen.

---

## ADR-001 · Desktop-Shell: Tauri 2 statt nativem GTK4 oder Electron
**Status:** akzeptiert · S-4 gemessen (2026-10-17), RAM-Budget angehoben (User-Entscheidung)

**Kontext.** Linux zuerst, Web als Nummer 2, Android später. Harte Anforderung: niedriger Idle-RAM,
schneller Start, kein Electron.

**Optionen**

| Kriterium | Electron | **Tauri 2 (WebKitGTK)** | GTK4 + Rust (gtk-rs/Relm4) |
|---|---|---|---|
| Idle-RAM (PSS, einfache App) | 150–300 MB | ~70–130 MB | ~35–70 MB |
| Kaltstart | 0,8–2 s | ~0,3–0,6 s | ~0,15–0,4 s |
| Binary/Paket | 80–150 MB | 3–10 MB | 2–8 MB |
| UI-Code wiederverwendbar für Web | ja | **ja** | nein |
| UI-Code wiederverwendbar für Android | eingeschränkt | **ja (Tauri Mobile)** | nein |
| Proton-ähnliche Optik umsetzbar | ja | **ja (CSS)** | mühsam (libadwaita-Look dominiert) |
| Rendering-Risiken | gering | WebKitGTK/GPU-Treiber | gering |

*(Größenordnungen aus Erfahrungswerten; verbindlich sind erst eigene Messungen in S-4.)*

**Entscheidung.** Tauri 2 mit Svelte-5-Frontend.

**Begründung.**
- Eine UI-Codebasis für Linux, Web und (später) Android – bei drei Zielplattformen der entscheidende Faktor.
  GTK4 hieße: UI zweimal (GTK + Web), mit Android dreimal.
- Erfüllt „kein Electron" klar: keine gebündelte Chromium-/Node-Runtime, System-WebView, Rust-Backend.
- Proton-Optik (eigene Dichte, Farben, Typografie) ist in CSS deutlich einfacher als gegen libadwaita.
- GTK4 wäre beim RAM ~2× besser. Dieser Abstand wird akzeptiert, solange die Budgets aus `AGENTS.md` halten.

**Konsequenzen.**
- WebKitGTK ist die Referenz-Engine: CSS/JS-Features nur, wenn sie in WebKitGTK 2.4x performant laufen.
- **Ausstiegsoption:** Da der Core UI-unabhängig ist, kann ein GTK4-Frontend (`apps/desktop-gtk`) später
  ergänzt werden, ohne Core/Sync/Adapter zu ändern. Auslöser: Budgets in S-4 dauerhaft verfehlt.

**Messung S-4 (P7-02, 2026-10-17).** Tauri 2.12.4, WebKitGTK 2.54.1, Mesa 26.2 (radeonsi), KDE Plasma, AMD Ryzen 9 5900X +
Radeon RX 7700/7800 XT, Release-Build (4,4 MB). Temporäre Seite: 1 000 Zeilen à 36 px, einfache virtualisierte Liste
(Svelte 5, `contain: strict`, `translateY`). Messung mit `tools/measure-desktop.sh` (PSS aus `/proc/*/smaps_rollup` über
den ganzen Prozessbaum; `smem`/`pidstat` waren nicht installiert). NVIDIA/Intel nicht verfügbar → offen für P8.

| Metrik | Wayland (Standard) | X11 (`GDK_BACKEND=x11`) | Budget alt (Ziel/hart) |
|---|---|---|---|
| Start bis erster Paint (Median, warm) | 349–507 ms | 368 ms | < 400 / < 800 ms ✔ |
| PSS nach 60 s Leerlauf | **288–299 MB** (App 111–119, WebProcess 146–151, Network 37) | **266 MB** (80 / 156 / 37) | < 120 / < 180 MB ✗ |
| Leerlauf-CPU (30 s) | 0,00–0,03 % | 0,03 % | ~0 % ✔ |
| Auto-Scroll 1 000 Zeilen (rAF-Intervalle) | p50 17 ms, p95 18–19 ms, ≤ 4 Drops | p50 17 ms, p95 19–21 ms, ≤ 1 Drop | 60 fps ✔ |

Aufschlüsselung (smaps): Großteil ist gemappter Bibliothekscode, der ohne andere WebKitGTK-Programme allein der App
angerechnet wird – `libwebkit2gtk` ~52 MB, `libjavascriptcoregtk` ~21 MB, `libLLVM`+`libgallium` (Mesa-GL) ~38 MB über
UI- und WebProcess; dazu Heap des UI-Prozesses 30–44 MB (unter Wayland mehr, EGL), `WKFastMalloc` ~40 MB. Ausprobiert ohne
verlässlichen Effekt (Streuung ±40 MB je Lauf): Cache-Modell `DocumentViewer`, `HardwareAccelerationPolicy::Never`
(spart LLVM im WebProcess, sonst 216–274 MB), `WEBKIT_DISABLE_DMABUF_RENDERER`, `WEBKIT_DISABLE_COMPOSITING_MODE`,
`JSC_useJIT=0`, `MALLOC_ARENA_MAX`. Messfalle: Läuft eine zweite Instanz (auch nur deren WebKit-Kindprozesse), halbiert sich
der Bibliotheksanteil scheinbar (gemessen: 104 MB) – das Skript wartet deshalb auf den ganzen Prozessbaum und warnt bei
fremden WebKit-Prozessen.

**Entscheidung nach S-4 (User, 2026-10-17):** Tauri bleibt (eine UI für Linux/Web/Android wiegt schwerer). RAM-Budget
angehoben auf **Ziel < 250 MB, hart < 320 MB** PSS inkl. WebKit (`AGENTS.md` §4). Start-, CPU- und Interaktions-Budgets
unverändert. In P8-03 mit echter App erneut messen (inkl. `HardwareAccelerationPolicy::Never` als Option, NVIDIA falls
verfügbar). Die GTK4-Ausstiegsoption bleibt bestehen, falls auch das neue Budget dauerhaft verfehlt wird.
**Nachtrag P7 (2026-10-18):** Fertige App (angemeldet, Sync aktiv) 326–330 MB PSS – knapp über dem neuen harten Budget
→ FIX-05 / P8-03.
**Nachtrag P8-03 (2026-10-18):** Echte App, Release (20 MB), Testdatenbank mit 1 000 Tasks, nicht angemeldet, Wayland.
Vorher 315–336 MB. Je Variante 3 Läufe (PSS nach 45 s): `MALLOC_ARENA_MAX=2` 305–313; **`WEBKIT_DISABLE_COMPOSITING_MODE=1`
303/303/303 MB** (WebProcess −20 MB, Streuung weg); `GDK_BACKEND=x11` 275–277 (App-Prozess −37 MB); X11 + ohne Compositing
264–285. Ohne Compositing rendert die App unter Wayland und X11 korrekt (Bildschirmfotos), Scrollen 1 000 Zeilen p95 18 ms
und Abhaken (nächster Frame 15–16 ms) unverändert – anders als `HardwareAccelerationPolicy::Never` in S-4 (schwarzes
X11-Fenster). Die frühere Aussage „ohne verlässlichen Effekt" galt für den Spike mit ±40 MB Streuung.
**Entscheidung:** Die App setzt `WEBKIT_DISABLE_COMPOSITING_MODE=1` beim Start, sofern nicht gesetzt (Opt-out
`TODO_WEBKIT_COMPOSITING=1`). Ergebnis: **306 MB** (App 120, WebProcess 159, Network 35) – unter hart 320, Ziel 250 verfehlt.
X11 wird **nicht** erzwungen (Wayland-Skalierung/Schärfe); als dokumentierter Schalter für knappe Systeme. Weitere
Hebel → FIX-07 (Heap des App-Prozesses 44 MB, `malloc_trim`, Proton-Kontext später laden).
**User-Entscheidung (2026-10-18):** Der RAM-Verbrauch ist so in Ordnung; FIX-07/N0-02 zurückgestellt (nur bei Bedarf).

---

## ADR-002 · Shared Logic in Rust (nativ + WASM + später UniFFI)
**Status:** akzeptiert

**Entscheidung.** Domain, Use-Cases und Sync-Engine in Rust (`todo-core`, `todo-sync`).
Nativ in Tauri, als WASM im Web, auf Android via Tauri Mobile oder UniFFI.

**Begründung.** Eine Implementierung der kritischen Logik (Merge, HLC, Validierung) statt Parallel-Implementierungen
in TS/Kotlin, die auseinanderlaufen. Rust kompiliert sauber auf alle drei Ziele.

**Konsequenzen.**
- Async-Traits müssen auf `wasm32` ohne `Send` funktionieren → Ports mit `#[async_trait(?Send)]` oder
  `cfg`-abhängigen Bounds (siehe `interfaces.md`).
- Kein `tokio` im Core. Runtime nur in Composition Roots (Tauri: tokio; Web: `wasm-bindgen-futures` – noch nicht umgesetzt).
- Öffentliche API FFI-freundlich halten (ADR-008).

---

## ADR-003 · UI-Framework: Svelte 5 + eigenes Mini-Design-System
**Status:** akzeptiert

**Optionen.** Svelte 5, SolidJS, Preact, Lit, React.

**Entscheidung.** Svelte 5 (Runes), ohne Komponentenbibliothek; Headless-Primitives nur punktuell
(z. B. `bits-ui` für Menüs/Dialoge, falls Barrierefreiheit sonst zu teuer wird).

**Begründung.**
- Compiler-basiert, sehr kleine Runtime (~wenige KB), feingranulare Reaktivität via Runes.
- Solid ist ähnlich schnell; Svelte gewinnt bei Lesbarkeit, Ökosystem und Tauri-Vorlagen.
- React/Preact: VDOM-Overhead, größeres Bundle, unnötig für dieses Profil.
- Kein Tailwind: Design-Tokens als CSS Custom Properties sind leichter, Theme-Wechsel ohne Rebuild,
  kein Utility-Klassen-Wildwuchs. (Tailwind wäre performance-neutral, aber unnötig.)
- **Kein SvelteKit** in v1: eine SPA mit eigenem Mini-Router reicht und vermeidet SSR-Komplexität in Tauri.

---

## ADR-004 · Lokaler Speicher: SQLite (nativ), SQLite-WASM/OPFS (Web, vorläufig)
**Status:** nativ akzeptiert · Web vorläufig bis Spike S-3

**Begründung.** SQLite: bewährt, schnell, transaktional, Volltextsuche (FTS5) möglich, gleiches Schema überall.
Web: SQLite-WASM mit OPFS im Worker liefert Schema-Parität. Alternative IndexedDB ist kleiner, aber
ohne SQL/FTS und mit eigener Query-Logik.

**Konsequenzen.** Storage-Port ist async. Web-Implementierung kann in TS liegen und per wasm-bindgen
injiziert werden, falls `rusqlite` auf wasm32 nicht praktikabel ist (S-3 klärt das).

---

## ADR-005 · Sync über Proton Drive mit Op-Log pro Gerät
**Status:** akzeptiert · Details: `sync-strategy.md`

**Entscheidung.** Kein „eine Datenbank-Datei auf Drive". Stattdessen: unveränderliche, gebündelte
Op-Segmente pro Gerät + periodische Snapshots; Merge feldweise per Last-Writer-Wins mit HLC.

**Begründung.** Drive ist ein Dateispeicher ohne Transaktionen/Locks. Wenn jedes Gerät nur eigene Dateien
schreibt, gibt es keine Schreibkonflikte auf Drive-Ebene; Konflikte werden deterministisch im Core gelöst.

---

## ADR-006 · Verschlüsselung
**Status:** akzeptiert

- **Transport/Remote:** Proton Drive verschlüsselt Dateien E2E (OpenPGP). Diese Krypto lebt komplett
  im `proton-adapter`.
- **Zusätzlich** (über `Cipher`-Port): Payload der Op-Segmente wird vor Upload mit einem App-eigenen
  Schlüssel (XChaCha20-Poly1305) verschlüsselt. Begründung: Defense-in-Depth und Backend-Austauschbarkeit
  (gleiches Format auf einem anderen `RemoteStore`). Kosten gering.
- **Lokal at-rest:** v1 verlässt sich auf Festplattenverschlüsselung des OS; SQLCipher als optionales
  Feature evaluieren (kostet Startzeit und Binary-Größe).
- Schlüssel liegen im `KeyStore`-Port (Secret Service über `oo7`, ohne libsecret / später WebCrypto non-extractable /
  Android Keystore).
- Keine eigene Krypto-Primitive implementieren; nur `RustCrypto`-Crates bzw. Proton-eigene Bibliotheken im Adapter.

---

## ADR-007 · Monorepo-Werkzeuge
**Status:** akzeptiert

Cargo-Workspace (`crates/*`, `apps/desktop/src-tauri`) + pnpm-Workspace (`apps/*`, `packages/*`).
Kein Nx/Turborepo – unnötig bei dieser Größe. Aufgaben über `justfile`, sobald Code existiert.

---

## ADR-008 · FFI-freundliche Core-API
**Status:** akzeptiert

Öffentliche Fassade (`todo_core::api` bzw. Composition-Root-Commands) nimmt/liefert nur owned DTOs
(`String`, `Vec`, einfache Enums, `Option`), Fehler als Enum. Damit sind Tauri-IPC, wasm-bindgen und UniFFI
ohne Umbau bedienbar.

---

## ADR-009 · Release-Build-Profil
**Status:** akzeptiert

```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
opt-level = 3        # Desktop: Geschwindigkeit
[profile.release-wasm]
inherits = "release"
opt-level = "z"      # Web: Größe
```

---

## ADR-010 · Datums-Crate im Core: chrono ohne Default-Features
**Status:** akzeptiert (P1-05, 2026-10-17)

**Kontext:** `Due::Date` braucht ein Kalenderdatum; Kandidaten `time` und `chrono`.
**Entscheidung:** `chrono = { default-features = false, features = ["serde"] }`; Zeitpunkte bleiben
`Timestamp(i64)` ohne Kalender-Crate.
**Begründung:** Gleiche Probe-Funktion (parse/vergleich/format) als Release-wasm32 (`opt-level = "z"`):
`time` 17,7 KB gzip, `chrono` 18,4 KB gzip – vernachlässigbar. `chrono` hat 1 statt 4 transitive Laufzeit-Deps,
kleineren Quelltext und den bereits dokumentierten Typ `NaiveDate`.
**Konsequenzen:** Default-Features müssen aus bleiben (sonst `clock`/`wasmbind` → js-sys im Core).
Keine Zeitzonen-Rechnung im Core; lokale Daten/Tagesgrenzen liefert der Aufrufer.

---

## ADR-011 · Sortierschlüssel: eigenes Fractional Indexing nach Greenspan
**Status:** akzeptiert (P1-06, 2026-10-17)

**Kontext:** Umsortieren soll nur eine Task ändern (konfliktarmer Sync).
**Entscheidung:** Eigene Implementierung (~200 Zeilen, `domain/sort_key.rs`) des Verfahrens aus
„Implementing Fractional Indexing" (D. Greenspan): Base62, Ganzzahlteil variabler Länge + Bruchteil.
**Begründung:** Kein zusätzliches Crate; einfaches Bruchteil-Verfahren ließe Keys beim wiederholten Einfügen am
Anfang/Ende linear wachsen (~170 Zeichen nach 1 000), so bleiben es 3 Zeichen. Property-Test sichert Ordnung.
**Konsequenzen:** Sortierung per Byte-Vergleich (SQLite: `BINARY`-Collation). Gleiche Keys sind nach
parallelem Einfügen auf zwei Geräten möglich → Tiebreak (z. B. nach ID) in P5 festlegen.
**Nachtrag (FIX-02):** Anzeige nach `sort_key`, dann `TaskId`; `MoveTask` verteilt einen Lauf gleicher Keys neu.

---

## ADR-012 · ID-Erzeugung im Core ohne Port
**Status:** akzeptiert (P1-08, 2026-10-17)

**Kontext:** `TaskId::new()` usw. nutzen `Uuid::now_v7()` – also Systemzeit und Zufall direkt, nicht über `Clock`.
**Entscheidung:** Bewusst akzeptiert; kein `IdGenerator`-Port.
**Begründung:** IDs müssen nur eindeutig und grob zeitlich sortierbar sein, nicht deterministisch; Tests
vergleichen IDs nie mit festen Werten. Ein Port würde jede Entity-Erzeugung verkomplizieren.
**Konsequenzen:** Auf `wasm32-unknown-unknown` muss die Composition Root (`todo-wasm`) die Zufallsquelle
liefern (`uuid`-Feature `js`). `just wasm` prüft das bei jedem `just check`.

---

## ADR-013 · Proton-Anbindung: Rust-Eigenbau mit Browser-Session-Fork
**Status:** akzeptiert (P2-06, 2026-10-17 – vom User freigegeben; Fork-Login live bestätigt in P2-02)

**Kontext:** Spike S-1/S-2, Recherche `development/spikes/proton-research.md`.
**Entscheidung:** `proton-adapter` als Rust-Eigenbau:
- Login per **Browser-Session-Fork** (Client-ID `external-drive`), kein Passwort/2FA/CAPTCHA in der App.
- Krypto ausschließlich über `proton-crypto` + `proton-crypto-account` (MIT, Feature `rustpgp`), keine Eigenbau-Krypto.
- HTTP über `reqwest` (rustls); `x-pm-appversion = external-drive-<produkt>@<semver>-<channel>`.
- Drive-Umfang minimal: ein App-Ordner, kleine unveränderliche Dateien, Änderungen über Volume-Events.
- Neue Inhalte im „pre-crypto-refresh"-Format (v4/SEIPD v1), bis die offiziellen Apps das neue Format lesen.
**Begründung:** Lizenz (MIT), ein Prozess (RAM-Budget), Web-Option (reines Rust), Isolation hinter den Ports.
**Risiken:** Krypto-Migration Ende 2026/Anfang 2027 (Nachziehen nötig, Hoffnung: über Proton-Crates); undokumentierte
API; Fork-Login im SDK als „Übergangslösung" markiert; Proton-Crates „not vetted outside Proton".
**Lizenzfolgen:** nur MIT/Apache-Abhängigkeiten; kein Code aus GPL/AGPL-Projekten; SDK-Code nur als Referenz.
**Alternativen:** C – offizielles Drive-CLI als Sidecar (Notnagel); B – C#-SDK per FFI (zu schwer).
**Nachtrag P2-05 (live):** Uploads verlangen inzwischen **Block-Verifikation** (`…/revisions/{r}/verification` +
`Verifier.Token` je Block); ohne sie 400/2000 „outdated version". Referenz für den Adapter ist daher das **aktuelle
offizielle SDK** (MIT, nur lesen), nicht rclone/go-proton-api (Stand 2026-08 ohne Verifikation). Bestätigt das Risiko
„API ändert sich unangekündigt" → Contract-/Live-Smoke-Test in P6-05 einplanen.
**Nachtrag P6-04 (2026-10-17):** Krypto-Migration erneut geprüft – laut SDK-README weiterhin „end of 2026/early 2027,
may shift", kein Migrationsleitfaden für Drittanbieter; Weg A bleibt. Adapter umgesetzt mit v1-Share-Endpunkten (live erprobt)
plus v2 für Verifikation/Commit/endgültiges Löschen; Referenz für Request-Formate: OpenAPI-Typen des MIT-SDK (nur gelesen).
Kosten: `proton-crypto`/rpgp bringt den Abhängigkeitsbaum des Adapters auf 255 Crates (RSA/DSA/ECC) – Binärgröße in P8 messen.
Bekannte Lücke: keine Signaturprüfung beim Lesen (siehe `interfaces.md` §4) – geschlossen mit ADR-025.
**Web/CORS:** vertagt – Web ist nicht im Prototyp; Preflight-Test steht aus. Falls Proton fremde Origins ablehnt:
Same-Origin-Proxy oder Web ohne Sync (eigener ADR vor dem Web-Meilenstein).
**Bestätigt in den Live-Proben P2-03…P2-05** (Details `interfaces.md` §3–4): Refresh-Rotation, Fehlercodes,
Duplikat-Code 2500, Entwurfs-Code 2501, Events, Latenzen, Lesbarkeit in der Proton-Weboberfläche. Weiter zu beobachten:
Stand der Krypto-Migration (Schreiben im Altformat muss weiter funktionieren).

---

## ADR-014 · SQLite-Zugriff über einen dedizierten Datenbank-Thread
**Status:** akzeptiert (P4-01, 2026-10-17)

**Kontext:** `rusqlite` ist synchron, der `TaskRepository`-Port async. Core und Adapter sollen runtime-frei bleiben.
**Entscheidung:** `todo-storage-sqlite` startet pro Datenbank einen Thread, der die einzige `Connection` besitzt.
Repository-Methoden schicken eine Closure per `std::sync::mpsc` und warten auf einen `futures::channel::oneshot`.
**Begründung:** Kein Executor-Thread blockiert auf SQLite; keine Bindung an tokio (`spawn_blocking`) und damit
gleich nutzbar in Tauri, Dev-CLI (`block_on`) und Tests; alle Zugriffe ohne Lock serialisiert.
**Alternativen:** `Mutex<Connection>` + `spawn_blocking` in jeder Shell (Runtime-Bindung, Lock über `.await`);
Connection-Pool mit parallelen Lesern (unnötig für eine Einzelnutzer-App, mehr RAM).
**Konsequenzen:** Alle Zugriffe laufen seriell – ein langer Lesezugriff verzögert Schreibzugriffe. Bei Bedarf später
eine zweite, rein lesende Verbindung (WAL erlaubt parallele Leser). Drop wartet auf das Thread-Ende.

---

## ADR-015 · Sync-Datenmodell im Core, Sync-Buchhaltung im Repository-Commit
**Status:** akzeptiert (P5-01, 2026-10-17)

**Kontext:** P5 verlangt, dass `commit` Zustand + Ops + Field-Clocks atomar schreibt und `apply_remote` gegen die
Field-Clocks mergt. Adapter (`todo-storage-sqlite`, Web-Speicher) dürfen nur von `todo-core` abhängen.
Würde die Engine Field-Clocks lesen, außerhalb mergen und das Ergebnis später schreiben, könnte ein lokaler Commit
dazwischen liegen (verlorene oder falsch gewonnene Änderung → Divergenz). Gleiches gilt für Ops, die aus einem
veralteten „vorher"-Zustand berechnet werden.
**Entscheidung:**
1. HLC, Op-Typen, `FieldName`/`FieldValue`, Diff (`ops_for`) und Merge (LWW + Tombstones) liegen als reine Logik
   in `todo_core::sync`. Abweichung vom Plan: Merge sollte in `todo-sync` liegen.
2. Die Repository-Implementierung ruft diese Funktionen **innerhalb ihrer Transaktion** auf: Diff gegen den
   gespeicherten Register-Zustand, HLC-Tick, Outbox, Register; bzw. beim Pull Merge + Materialisierung + Cursor.
   Die Orchestrierung ist einmal im Core implementiert (generisch über einen kleinen synchronen Transaktions-Trait),
   Adapter liefern nur Lesen/Schreiben.
3. Pro Entity und Feld wird ein **LWW-Register (Wert + HLC)** gespeichert (`field_clocks`), Löschungen als Tombstone.
   Die Tabellen `tasks`/`task_lists` sind die daraus materialisierte Sicht. Dadurch können auch Ops für (noch)
   unbekannte Entities gespeichert werden, und Konvergenz ist eine Eigenschaft der Register.
4. HLC-Zustand, Geräte-ID und Op-Sequenz liegen in `sync_meta` und werden im selben Commit fortgeschrieben.
   Die Uhrzeit für den HLC-Tick kommt vom Aufrufer (`ChangeSet::at`, gesetzt vom `CoreApi`-Decorator).
5. `todo-sync` enthält Segment-Format, Engine, Scheduler und Snapshots. Adapter mit IO bzw. Krypto bekommen eigene
   kleine Crates (`todo-remote-fs`, `todo-crypto`), damit `todo-sync` frei von Dateisystem und Krypto-Implementierung bleibt.
**Begründung:** Keine Races zwischen lokalem Schreiben und Merge, ohne Locks über `.await`; HLC/Seq nach Absturz nie
doppelt; Merge-Logik einmal implementiert und für alle Speicher identisch (Konvergenz-Property-Test im Core).
**Konsequenzen:** `todo-core` bekommt `postcard` als Abhängigkeit (kanonisches Op-Format, auch für die Outbox).
`updated_at` ist gerätelokale Metadaten (kein Sync-Feld): lokal vom Use-Case, bei Remote-Änderungen = größte
Wall-Zeit der Register. Die Inbox bekommt eine feste ID, damit die Inboxen aller Geräte zu einer zusammenlaufen.

---

## ADR-016 · Op-Serialisierung: postcard
**Status:** akzeptiert (P5-02, 2026-10-17)

**Kontext:** Ops werden in der Outbox gespeichert und (komprimiert, verschlüsselt) als Segmente/Snapshots zwischen
Geräten ausgetauscht. JSON ist zu groß und zu langsam. Kandidaten: `postcard`, CBOR (`ciborium`).
**Entscheidung:** `postcard` (`default-features = false`, `alloc`) als kanonisches Format, Abhängigkeit von `todo-core`.
**Begründung:** Kleinste Ausgabe (Varints, keine Feldnamen), sehr schnell, `no_std`-fähig, winzig im WASM-Bundle;
`ciborium` ist größer und langsamer und bringt Selbstbeschreibung, die wir wegen Versionierung im Segment-Header
nicht brauchen.
**Regeln (Formatstabilität):** postcard ist nicht selbstbeschreibend → Struct-Felder und Enum-Varianten aller
Op-Typen (inkl. `EntityRef`) nie umsortieren/entfernen, nur anhängen; `FieldName` als fester `u8`-Code,
`Priority` als fester Code, Datum als Tageszahl; IDs (16 Bytes) und HLC (28 Bytes) im Binärformat roh, in JSON als
String. Golden File `crates/todo-core/tests/golden/ops_v1.bin` sichert das ab. Inkompatible Änderungen nur mit neuer
`format_version` im Segment-Header.
**Konsequenzen:** Unbekannte Feld-Codes (neuere App) machen eine Op unlesbar → Segment wird übersprungen, Hinweis
„App aktualisieren" (`sync-strategy.md` §5). postcard verwirft eigene serde-Fehlertexte (nur generischer Fehler).

---

## ADR-017 · Segment-Format, Cipher-API und Kompression
**Status:** akzeptiert (P5-07, 2026-10-17)

**Entscheidung:**
- Segment = 40-Byte-Klartext-Header (`TSEG`, `format_version` 1, Codec 1 = zstd, Gerät, `seq_from`, `seq_to`) +
  `Cipher::seal(zstd(postcard(Vec<Op>)), aad = Header)`. Dateiname `ops/<device>/<seq_from>-<seq_to>.seg`, ≤ 500 Ops.
  Lesefehler sind `NewerFormat` (überspringen, „App aktualisieren") oder `Corrupt` (überspringen).
- `Cipher` bekommt Schlüsselmaterial (`SecretBytes`) statt `KeyId` und erzeugt Schlüssel selbst (`generate_key`).
  Implementierung `XChaCha20Poly1305Cipher` im eigenen Crate `todo-crypto` (zufällige 192-Bit-Nonce pro Nachricht).
- Kompression per Feature: `zstd-native` (C, Standard nativ) oder `pure-rust-zstd` (`ruzstd`, Web). Gleiches
  Datenformat (Standard-zstd-Frames); entpackt wird höchstens 64 MiB.
- Zufall: `getrandom` 0.4 (dieselbe Version wie `uuid`); im Browser aktiviert `todo-wasm` `getrandom/wasm_js`.
**Begründung:** Header als AAD bindet Inhalt an Gerät/Bereich ohne zusätzliche Felder; zustandslose Cipher ist
einfacher zu testen und FFI-freundlich; eigenes Krypto-Crate hält `todo-sync` frei von Krypto-Implementierungen.
**Konsequenzen:** `just test` testet den `ruzstd`-Pfad zusätzlich (`--all-features` aktiviert sonst beide).
Golden File für den Header: `crates/todo-sync/tests/golden/segment_header_v1.bin`.

---

## ADR-018 · Proton-HTTP-Schicht: reqwest + rustls/ring, Logging nur mit Routen-Vorlagen
**Status:** akzeptiert (P6-01, 2026-10-17)

**Kontext:** ADR-013 legt `reqwest` mit rustls fest. `reqwest` 0.13 bringt mit `rustls` standardmäßig `aws-lc-rs`
(C/CMake-Build, für Android-Cross-Compile lästig). Logs dürfen weder Tokens noch Dateinamen enthalten; der Fork-Selector
steht im URL-Pfad und ist selbst ein Geheimnis (wer ihn kennt, kann die Session abholen).
**Entscheidung:**
1. `reqwest` ohne Default-Features (`rustls-no-provider`, `json`, `multipart`; kein HTTP/2, kein System-Proxy) +
   `rustls` mit `ring`; der Adapter installiert `ring` als Prozess-Provider, falls die Shell keinen gesetzt hat.
   Zertifikate über `rustls-platform-verifier` (Systemspeicher).
2. Der Adapter braucht einen **tokio-Kontext** (reqwest/hyper). Tauri hat ihn; die Dev-CLI startet für Proton-Befehle
   eine Runtime. Core, Sync-Engine und die übrigen Adapter bleiben runtime-frei.
3. Kein automatisches Wiederholen im HTTP-Client: 429/503+`Retry-After` → `CoreError::RateLimited`, Netz/5xx/408 →
   `Network`; Backoff macht der Scheduler (P5-10). Ausnahme kommt mit P6-03: einmaliger Refresh bei 401.
4. Logging über `tracing` (Target `proton_adapter::http`): Methode, **Routen-Vorlage** (`/auth/v4/sessions/forks/{selector}`),
   Status, Proton-Code, Dauer. Nie konkrete Pfade/URLs (auch nicht in Transportfehlern: `without_url`), Header, Bodies oder
   Server-Fehlertexte (lokalisiert, können Namen enthalten). Tokens als `Secret` mit redigiertem `Debug`.
**Begründung:** `ring` baut ohne C-Toolchain-Extras für alle Ziele; 102 Crates im normalen Abhängigkeitsbaum, davon fast alles
hyper/rustls/tokio, das Tauri ohnehin mitbringt. Routen-Vorlagen machen Redaction strukturell statt per Filter.
**Alternativen:** `aws-lc-rs` (Default, schneller, aber Build-Aufwand); `ureq` auf eigenem Thread (runtime-frei, kleiner, aber
kein späterer wasm-Pfad und Abweichung von ADR-013); Log-Filter mit Regex-Redaction (fehleranfällig).
**Offen für Android:** `rustls-platform-verifier` braucht dort eine JNI-Initialisierung (oder `webpki-roots`).

---

## ADR-019 · Desktop-Shell: Typgenerierung, Fehlerform, Sync-Treiber
**Status:** akzeptiert (P7-03, 2026-10-18)

**Entscheidung:**
1. **TS-Typen mit `ts-rs` als dev-dependency** (`#[cfg_attr(test, derive(ts_rs::TS), ts(export))]` an den DTOs in
   `todo_core::api::dto` und `apps/desktop/src-tauri/src/dto.rs`). Export beim `cargo test` (`just types`, auch in
   `just test`) nach `packages/core-client/src/generated/`, Pfad über `.cargo/config.toml` (`TS_RS_EXPORT_DIR`);
   `packages/core-client/src/types.ts` re-exportiert. Drift sieht man im Git-Diff nach `just check`.
2. **Fehler an der IPC-Grenze** als `ErrorDto { kind, message }` (`kind` = camelCase-Variante von `CoreError` +
   `unavailable`); die UI verzweigt nur über `kind`.
3. **Sync-Treiber** (`src-tauri/src/sync.rs`): ein tokio-Task um den reinen `Scheduler` (ein Kanal, ein Timer,
   `select!`); Läufe in eigenen Tasks. `LocalChange` kommt nur aus schreibenden Commands (nicht aus `core://changed`,
   sonst löste jeder Pull mit Änderungen einen weiteren Lauf aus). Ohne Sitzung kein Runner → `Disabled`.
4. **Konto im Hintergrund:** Schlüsselbund + Proton-Auth werden nach dem Start geladen (D-Bus blockiert den ersten
   Paint nicht); Engine + Remote-Store leben so lange wie die Anmeldung (Schlüssel/Ordner-Cache einmal pro Prozess).
5. **ACL:** App-Manifest in `build.rs` listet alle Commands; die Capability erlaubt genau diese + `event:listen/unlisten`.

**Alternativen:** `specta`/`tauri-specta` (nur RC-Versionen, Laufzeit-Abhängigkeit im Core) – verworfen;
handgeschriebene TS-Typen – driften.
**Konsequenzen:** `todo-core` bekommt `ts-rs` nur als dev-dependency (kein Einfluss auf Binary/WASM). Neues Command ⇒
Eintrag in `build.rs` (sonst „not allowed by ACL"), `generate_handler!` und `CoreClient`.

---

## ADR-020 · Erster Login: Bestände vereinen, Inbox per fester ID, Listen nie nach Namen zusammenführen

**Status:** akzeptiert (2026-10-18, P8-01)
**Kontext:** Vor der ersten Anmeldung angelegte Daten liegen nur in der Outbox. Zwei Geräte mit eigenen Beständen
dürfen nach dem Login weder Daten verlieren noch die Inbox doppelt zeigen. Im P7-11-Live-Test gab es „Arbeit" zweimal
(zwei CLI-Datenbanken, gleiche Namen, verschiedene IDs).
**Entscheidung:**
1. Lokale Daten werden beim ersten Sync als normale Ops gepusht; kein Sonderpfad für „erster Login".
2. Die Inbox hat auf allen Geräten dieselbe ID (`TaskListId::INBOX`, seit P1-07); ihre beiden `Create`-Ops verschmelzen
   per LWW zu einer Liste. Gleiche Sort-Keys aus beiden Inboxen ordnet der `TaskId`-Tiebreak (FIX-02).
3. **Gleichnamige Listen bleiben getrennte Listen** (eigene IDs). Kein automatisches Zusammenführen: Zwei Geräte, die
   gleichzeitig „B in A" bzw. „A in B" zusammenführen, löschen per Delete-wins beide Listen und schieben alle Tasks in
   die Inbox; ein deterministischer Gewinner (z. B. kleinere ID) wäre möglich, verschiebt aber Tasks ohne Zutun des
   Nutzers. Zusammenführen kommt später als explizite Aktion („Liste zusammenführen").
4. Desktop-Profile mit eigenem Datenverzeichnis (`TODO_DATA_DIR`) haben eigene Schlüsselbund-Einträge
   (`dev.tasks.app@<pfad>`): eigene Sitzung, eigener Sync-Schlüssel-Cache – zwei Profile mit einer Sitzung würden sich
   beim Token-Refresh (rotiert das Token-Paar) gegenseitig abmelden.

**Konsequenzen:** Nachweis in `crates/todo-sync/tests/first_login.rs` (SQLite + Verzeichnis-Backend, auch Einstieg per
Snapshot). Doppelte Listennamen sind nach dem ersten Login möglich und für den Nutzer sichtbar.

---

## ADR-021 · Sammel-Operationen in der Core-API

**Status:** akzeptiert (2026-10-18, P0-01)
**Kontext:** Mehrfachauswahl (P0) braucht Erledigen, Löschen, Verschieben und Rückgängig für bis zu tausende Tasks.
Eine Schleife über Einzel-Commands in der UI hieße n IPC-Aufrufe, n Commits, n `Changed`-Events und halbe Zustände bei
Abbruch; das bisherige Rückgängig nach Löschen brauchte 3–4 Commits je Task und verlor Felder (FIX-09).
**Entscheidung:**
1. Eigene Use-Cases `CompleteTasks`, `DeleteTasks`, `MoveTasks`, `RestoreTasks` und `CoreApi::complete_tasks`,
   `delete_tasks`, `move_tasks`, `restore_tasks`: **ein** Aufruf mit allen IDs → **ein** Commit (eine Transaktion,
   Ops im Outbox) → **ein** `Changed`-Event. Obergrenze `BULK_LIMIT_MAX = 2 000` je Aufruf (`Validation`), doppelte
   IDs werden entfernt, leere Eingabe = kein Commit.
2. **Unbekannte IDs werden übersprungen**, nicht als Fehler gemeldet (z. B. per Sync gelöscht, während die Auswahl
   stand); das Ergebnis enthält nur die tatsächlich geänderten Tasks. Tasks im Zielzustand erzeugen keinen Op. Jeder
   andere Fehler lässt den ganzen Aufruf scheitern, nichts ist geschrieben.
3. `DeleteTasks` liefert die Tasks **vor** dem Löschen; `RestoreTasks` legt daraus Kopien mit **neuen IDs** an (eine
   gelöschte ID ist per Tombstone endgültig, Delete-wins), mit allen Feldern inkl. `created_at`, Status/`completed_at`
   und **altem Sort-Key** – die Position stimmt ohne Nachbar-Suche, Gleichstand löst der `TaskId`-Tiebreak (FIX-02).
   Fehlt die Liste inzwischen → Inbox. Die neuen IDs werden in alter Anzeigereihenfolge (Sort-Key, alte ID) vergeben
   (UUIDv7 ist im Prozess monoton), damit auch Tasks mit gleichem Sort-Key ihre Reihenfolge behalten.
4. `MoveTasks` verschiebt einen Block in bisheriger Anzeigereihenfolge (`sort_key`, `TaskId`), auch aus gemischten
   Listen; ohne `list_id` nur, wenn alle Tasks in einer Liste liegen. Neue Keys per `SortKey::n_between` (Halbierung
   zwischen den Nachbarn, Länge wächst logarithmisch; nacheinander erzeugte Keys wüchsen linear – ~170 Zeichen bei
   1 000). Nachbar-Logik inkl. FIX-02-Sonderfall teilt es mit `MoveTask`. Zielliste wird einmal geladen.
5. Port-Ergänzungen: `TaskRepository::get_tasks(ids)` (Standard-Implementierung = Schleife, SQLite in einem
   DB-Aufruf); `ReplicaStore::set_entity_state` bekommt den vorherigen Zustand, damit SQLite nur geänderte Register
   schreibt. SQLite-Verbindungen laufen mit `temp_store = MEMORY` (Statement-Journale des Fremdschlüssels, Messung in
   `architecture.md` §6).

**Alternativen:** Schleifen in der UI (siehe Kontext); Sammel-Varianten nur in der Shell (Commit pro Task, kein
atomares Rückgängig); Wiederbeleben gelöschter IDs (widerspricht Delete-wins im Sync-Modell).
**Konsequenzen:** Rückgängig nach Löschen ändert die IDs der Tasks – die UI muss Auswahl/Cursor auf die neuen IDs
umstellen. Tags/Unteraufgaben kennt `TaskDto` noch nicht; `restore_tasks` setzt sie leer (heute ohne Wirkung).

---

## ADR-022 · Upload-Debounce mit Höchstwartezeit, definierte Auslöser

**Status:** akzeptiert (2026-10-18, P1-02)
**Kontext:** Der Scheduler lud 3 s nach der *letzten* lokalen Änderung hoch (reiner Trailing-Debounce). Wer zügig
mehrere Dinge ändert (abhaken, umbenennen, anlegen), verschob den Upload dadurch beliebig weit; im Test der installierten
App kam das Hochladen „zu spät". Gleichzeitig soll nicht jede einzelne Änderung einen eigenen Lauf auslösen (Anfragen an
Proton, Rate-Limits, Akku).
**Entscheidung:**
1. Debounce 1,5 s nach der letzten Änderung, **höchstens 5 s** nach der ersten noch nicht hochgeladenen Änderung einer
   Serie (`SchedulerConfig::max_wait_ms`, Feld `burst_since`; zurückgesetzt bei Laufstart, Sync aus und Netz weg).
2. Änderungen während eines Laufs → genau ein weiterer Lauf direkt danach (wie bisher, `rerun`).
3. Fehler, Offline und `Retry-After` behalten Vorrang: kein schnellerer Upload im Backoff.
4. Auslöser sind genau die schreibenden Commands, je Aufruf **eine** Meldung (Sammelaktionen eingeschlossen); lesende
   melden nichts. Festgehalten in `commands.rs`, nachgewiesen per IPC-Test mit Zähler in `SyncHandle`.
**Alternativen:** Upload sofort je Änderung (viele Läufe, Rate-Limits); nur kürzerer Debounce ohne Höchstwert (Serien
verhungern weiterhin); Leading-Edge-Upload (zweiter Lauf für dieselbe Serie fast sicher).
**Konsequenzen:** Eine Änderung ist nach ~1,5 s plus Laufzeit oben, eine Serie nach ≤ 5 s; das andere Gerät sieht sie
beim nächsten Pull (Fokus bzw. periodisch) – dessen Takt bleibt unverändert.

---

## ADR-023 · Listen-Reihenfolge: Inbox fest zuerst, Benutzerlisten per Fractional Index

**Status:** akzeptiert (2026-10-18, P2-01)
**Kontext:** Listen lassen sich ab P2 umsortieren (Seitenleiste, Board-Spalten). Die Inbox hat auf allen Geräten dieselbe
ID (ADR-020), ihr Sort-Key kann aber von irgendeinem Gerät stammen und größer sein als der einer Benutzerliste.
**Entscheidung:**
1. Die Inbox steht **immer zuerst** und ist nicht verschiebbar (`TaskList::can_move`, `MoveTaskList` lehnt sie als Liste
   und als Nachbarn ab). `CoreApi::list_task_lists` sortiert sie vor alle anderen; das Repository bleibt bei
   `sort_key`, dann `id`.
2. Benutzerlisten werden wie Tasks per Fractional Index umsortiert (`MoveTaskList`, Nachbarn `after`/`before`, ohne
   Nachbarn ans Ende); nur der `sort_key` der verschobenen Liste ändert sich (ein `Set`-Op) – Ausnahme gleiche
   Nachbar-Keys nach parallelem Umsortieren (Lauf neu verteilen, wie FIX-02).
3. Kein neues Sync-Feld: `FieldName::SortKey` für Listen existiert seit P5; Konvergenz per LWW, Tiebreak `TaskListId`.
**Konsequenzen:** Umsortieren auf zwei Geräten gleichzeitig konvergiert (Test `concurrent_list_reorders_converge`);
die Inbox kann sich nie zwischen Benutzerlisten schieben.

## ADR-024 · Reihenfolge-Sperre in den Use-Cases, `last_sort_key` im Repository

**Status:** akzeptiert (2026-10-18, FIX-10/FIX-03)
**Kontext:** Use-Cases, die neue Sort-Keys vergeben, lesen erst die Reihenfolge und committen danach getrennt. Laufen
mehrere gleichzeitig (UI feuert parallel, Selbsttest `Promise.all`), lesen alle denselben Stand: 50 parallele
`create_task` bekamen **einen** Key (FIX-10). Außerdem lud `create_task` dafür die ganze Liste (O(n), FIX-03).
**Entscheidung:**
1. `Deps` hält eine gemeinsame async-Sperre (`futures::lock::Mutex<()>`, `todo-core` hängt dafür fest von `futures`
   ab – ohne Default-Features, nur `executor`/`std`; läuft auf wasm32). `CreateTask`, `CreateTaskList`, `MoveTask`,
   `MoveTasks`, `MoveTaskList` und `DeleteTaskList` nehmen sie für Lesen **und** Commit. Untergeordnete Use-Cases
   (`EnsureInbox`) nehmen sie nicht (nicht reentrant). Schreibende Use-Cases ohne neue Keys bleiben parallel.
2. Kein Key-Vergeben „in der Transaktion" im Repository: Das würde Fractional-Index-Logik in jeden Speicher ziehen;
   die Sperre im Core gilt für alle Speicher (SQLite, später Web) gleich.
3. Neuer Port `TaskRepository::last_sort_key(list_id)` mit Default über `list_tasks`; SQLite beantwortet ihn über den
   Index `tasks_by_list`. Anhängen ist damit O(1)-nah; Einfügen zwischen Nachbarn lädt die Liste weiterhin (FIX-11).
**Konsequenzen:** Parallele Aufrufe derselben `CoreApi` vergeben verschiedene Keys (Test
`parallel_creates_get_distinct_sort_keys`, ohne Sperre rot). Gleiche Keys über **Geräte** hinweg bleiben möglich und
werden wie bisher per `TaskId` aufgelöst (FIX-02). Sync (`apply_remote`) nimmt die Sperre nicht – ein Remote-Key, der
zwischen Lesen und Commit eintrifft, kann zu gleichen Keys führen; das ist derselbe Fall wie paralleles Bearbeiten
auf zwei Geräten.

---

## ADR-025 · Sicherheits-Audit run-1: Signaturprüfung beim Lesen, Integrität gegenüber dem Backend

**Status:** akzeptiert (2026-10-19, Audit `~/security-audit-skill/proton_todo/run-1`)
**Kontext:** Das Audit hat bestätigt, dass ein feindlicher oder kompromittierter Proton-Server den App-Ordner mit einem
eigenen Schlüssel ausstatten konnte (Lesepfad ohne Signaturprüfung, ADR-013 „bekannte Lücke"): Ein neues Gerät oder
ein Gerät nach Ab- und Wiederanmeldung erzeugte dann einen neuen Sync-Schlüssel, den der Server lesen konnte. Dazu
kamen kleinere Befunde (fremdes Segment unter `ops/<anderes Gerät>/`, Symlinks im Dev-Verzeichnis-Store,
Escape-Sequenzen in der Dev-CLI) und Leads (Login-URL in der Kommandozeile, unbegrenzte Downloads).
**Entscheidung:**
1. **Drive-Lesepfad signaturgeprüft, fail closed.** Share-`PassphraseSignature`, `NodePassphraseSignature`, Namen
   (eingebettete Signatur) und das Block-Manifest (`ManifestSignature` über die Block-Hashes in Index-Reihenfolge)
   müssen von einem nicht kompromittierten Adress-Key der Share-Adresse stammen; `NodeHashKey` vom Node- oder
   Adress-Key, `ContentKeyPacketSignature` vom Node-Key. Blöcke: lückenlos 1..n, SHA-256 des Chiffrats = gelisteter
   Hash. Nicht prüfbare Kinder werden übersprungen, nicht prüfbare Ordner auf dem Pfad sind ein Fehler.
   Signiert wird nur mit nicht kompromittierten, nicht veralteten Adress-Keys; der Share muss zu Volume und Wurzel passen.
2. **Sync-Schlüssel:** Ein lokaler Schlüssel muss zur Schlüsseldatei passen (sonst `Validation`, „ab- und anmelden");
   fehlt die Datei in einem Ordner mit Daten, wird kein neuer erzeugt; mit lokalem Schlüssel wird sie neu angelegt.
   Schlüssel müssen zur Cipher passen (Länge). Server-seitig widerrufene Sitzung → Desktop löscht den Sync-Schlüssel.
3. **Engine gegenüber unauthentisierten Dateinamen:** Segment-Header-Gerät = Ordner-Gerät (`fetch`), Snapshot-Header =
   Dateiname, Snapshot-Cursor höchstens bis zum höchsten vorhandenen Segment, eigene Snapshots werden nur über die lokal
   gemerkte Liste gelöscht, Gerätedateien nur mit passender ID; ein neueres Segment-Format pausiert nicht mehr den
   ganzen Sync (nur noch das Manifest). Bleibende Lücken erscheinen als Warnung.
4. **HLC:** feste Obergrenze `MAX_WALL_MS` (2100-01-01) in `Op::validate` und `apply_remote` – für alle Geräte gleich
   (Konvergenz), statt „jetzt + Drift".
5. **HTTP:** keine Weiterleitungen, Bodies begrenzt (API 16 MiB, Block 5 MiB), Storage-Token nur per HTTPS an
   `*.proton.me`; Dateien ≤ 128 MiB, Seiten-, Tiefen- und Eintragsgrenzen beim Listen; Namen mit `/`, `\`, `.`, `..`
   oder Steuerzeichen werden übersprungen; Decode-Fehler zitieren keine Server-Werte.
6. **Login:** Die Login-URL (Fork-Schlüssel im Fragment) geht als private HTML-Datei (`$XDG_RUNTIME_DIR`, 0600) an
   `xdg-open`, nicht als Argument. Selector/User-Code werden geprüft und im Pfad prozentkodiert; eine ersetzte Sitzung
   wird serverseitig beendet; Abmelden löscht Sitzung und Schlüssel unabhängig voneinander.
7. **Desktop:** Selbsttest-Commands nur im Debug-Build registriert, `freezePrototype`, kein natives Drag-and-drop,
   Datenverzeichnis 0700, Runner an ein Konto gebunden. **Dev-Adapter/CLI:** `FsRemoteStore` lehnt Symlinks unterhalb
   der Wurzel ab und legt Temp-Dateien exklusiv an; die CLI gibt Sync-Texte terminalsicher aus.
**Bewusst nicht geändert (Restrisiken):**
- `style-src 'unsafe-inline'` bleibt: Die UI nutzt `style=`-Attribute; ohne HTML-Injektionsstelle (keine gefunden) ohne Wirkung.
- Lokale Daten sind nicht an ein Konto gebunden: Nach Kontowechsel landen lokale Tasks im neuen Konto (gewollt, Abnahme
  Schritt 4); der Schlüssel des alten Kontos wird beim Abmelden/Widerruf gelöscht bzw. beim Vergleich abgelehnt.
- Abmelden offline beendet die Server-Sitzung nicht (Refresh-Token ist lokal gelöscht; sie verfällt serverseitig).
- `devices/*.json` und `manifest.json` bleiben ohne App-AEAD (jetzt aber Drive-signaturgeprüft); Formatänderung wäre ein
  neues Layout.
- `FsRemoteStore`: Wettlauf zwischen Symlink-Prüfung und Zugriff bleibt (nur Entwicklung/CLI).
- Debug-Build lädt die UI von `localhost:1420` (nur Entwicklung).
- Abhängigkeiten (OSV 2026-10-19, `pnpm audit --prod` ohne Befund): `rsa 0.9.10` (Marvin, Timing bei RSA-Entschlüsselung;
  lokal im Client ohne Orakel nicht ausnutzbar, kein Fix), `ml-dsa 0.0.4` (nur PQC-v6-Keys, wir nutzen v4; Fix erst mit
  neuerer `pgp`/`proton-crypto`), `glib 0.18.5` (`VariantStrIter`, nicht genutzt; Tauri pinnt gtk-rs 0.18),
  `proc-macro-error`, `zvariant`, `zvariant_derive`, `zbus_names` (unmaintained). Keine semver-kompatiblen Updates;
  bei Updates von Tauri/`proton-crypto`/`oo7` erneut prüfen; vom Nutzer am 2026-10-19 als Restrisiko akzeptiert.
**Konsequenzen:** Bestandsdaten dieser App sind kompatibel (sie hat schon immer signiert). Live-Probe am 2026-10-19 bestanden (alle 8 Contract-Fälle gegen das echte Drive, vom Nutzer ausgeführt) für: Prüfung der
von Proton angelegten Wurzel (Share- und Root-Passphrase-Signatur, `NodeHashKey`) und der `GET …/revisions/{id}`-Felder
`ManifestSignature`/`Blocks[].Hash` gegen das echte Backend – Tests laufen gegen den Fake-Server. Regressionstests:
`todo-sync/tests/integrity.rs`, `drive::tests::rekeyed_app_folder_is_rejected`, `tampered_blocks_are_rejected`,
`crypto::tests::unsigned_or_foreign_signed_material_is_rejected`, `api_client::tests::*`, `todo-remote-fs`
Symlink-Tests, Contract-Fall `apply_remote_ignores_implausible_future_ops`.

---

## ADR-026 · „Heute"/„Geplant" nach Fälligkeit sortiert; `set_dues` als Sammelaktion

**Status:** akzeptiert (2026-10-19, P4-02)
**Kontext:** P4 teilt „Heute" in Überfällig/Heute und „Geplant" nach Tagen. Bisher sortierte der Core alle Ansichten
außer „Erledigt" nach `sort_key` – bei seitenweisem Laden stünden Abschnitte dann quer durcheinander. Der Core rechnet
aber keine Zeitzonen (`Due::DateTime` speichert `tz` nur; Tagesgrenzen liefert der Aufrufer als `LocalDay`). „Alle auf
heute" muss außerdem viele Tasks in einem Commit verschieben und dabei je Task die Uhrzeit behalten.
**Entscheidung:**
1. `SmartView::Today`/`Upcoming` sortieren nach `Due::sort_instant(&day)`, dann `sort_key`, dann `id`. Zeitpunkt =
   `at` bzw. für ganztägige Fälligkeiten **ungefährer** lokaler Tagesbeginn `day.start + (datum − day.date) · 24 h`
   (`LocalDay::approx_start_of`). Ganztägiges steht damit vor Uhrzeiten desselben Tages; überfällig = älteste zuerst.
   Neue Cursor-Art `d:<ms>:<sort_key>:<id>` (In-Memory + SQLite, `COALESCE(due_at, (due_date − ?) · 86 400 000 + ?)`),
   Contract-Fall `due_views_sorted_by_due` (über Seitengrenzen).
2. Die Annäherung weicht an Tagen mit anderer UTC-Abweichung (Zeitumstellung zwischen heute und dem Datum) um deren
   Differenz (1 h) ab. Die UI sortiert geladene Seiten dieser beiden Ansichten deshalb exakt nach lokaler Zeit nach
   (`compareInView`/`dueInstant`); die Gruppierung (`lib/groups.ts`) baut darauf auf. Kein Zonen-Port nur dafür –
   der kommt ggf. mit N4 (Uhrzeit-Serien).
3. Neuer Use-Case `SetDues` / `CoreApi::set_dues(Vec<TaskDueDto>)`: je Task ein eigener Wert (`null` = entfernen),
   ein Commit, ein Event; unbekannte IDs und unveränderte Werte übersprungen; eine ungültige Fälligkeit schreibt
   nichts; Obergrenze `BULK_LIMIT_MAX`. Rückgängig = `set_dues` mit den alten Werten. Wird in P4-04 für „Fälligkeit…"
   an der Auswahl wiederverwendet.
**Konsequenzen:** Die Reihenfolge in „Heute"/„Geplant" ist nicht mehr von Hand änderbar (war sie schon vorher nicht:
Ziehen nur in Eingang/Listen). Ein Cursor alter Art (`s:`) für diese Ansichten wird als ungültig abgelehnt – Cursor sind
flüchtig (nur innerhalb einer Sitzung), daher kein Migrationsbedarf. Tauri: neues Command `set_dues` (Capability,
löst wie alle schreibenden Commands einen Sync aus).

---

## ADR-027 · Suche: Port `search_tasks`, SQLite mit FTS5-Trigram

**Status:** akzeptiert (2026-10-19, P4-05)
**Kontext:** Ab einigen hundert Tasks findet man eine Task nur durch Scrollen. Gesucht wird beim Tippen in der
Befehlspalette – keine O(n)-Arbeit je Tastendruck (§4), Teilwörter („richt" in „Bericht"), ohne Groß/Klein, auch bei
Umlauten; Titel und Notizen.
**Entscheidung:**
1. Neue Port-Methode `TaskRepository::search_tasks(&SearchQuery { text, limit })` mit **Standard-Implementierung**
   über `list_tasks` + `get_tasks` (O(n), für In-Memory/Tests und einen späteren einfachen Web-Speicher). Semantik in
   `SearchQuery::rank` und den Contract-Fällen `search_*`: jedes Wort als Teilwort in Titel **oder** Notiz;
   Rang offen vor erledigt, Titeltreffer vor reinen Notiztreffern, dann `sort_key`, `id`; `limit` 1..=500; Text
   höchstens 200 Zeichen; leerer Text = keine Treffer; gelöschte Tasks nie.
2. **SQLite überschreibt** das mit FTS5 (`tasks_fts`, Migration 0003): `tokenize = 'trigram case_sensitive 0'`,
   externer Inhalt (`content = 'tasks'`), Trigger für Einfügen/Löschen und für Updates **nur bei geändertem Titel
   oder Notiz** (`WHEN old.title IS NOT new.title …` – der UPSERT setzt alle Spalten; ohne `WHEN` kostete jedes
   Abhaken/Verschieben einen Index-Schreibvorgang, `move_tasks` 1 000 IDs 60 → 34 ms). Bestehende Tasks per
   `'rebuild'` in der Migration. FTS5 ist im gebündelten SQLite von `rusqlite` enthalten – keine neue Abhängigkeit.
3. Wörter unter 3 Zeichen kann ein Trigram-Index nicht finden; sie gehen über `LIKE … ESCAPE '\'` (Groß/Klein dann
   nur bei ASCII egal – bekannte Grenze). Suchwörter werden als FTS5-Zeichenketten gequotet, Operatoren wie `OR`,
   `*`, Klammern sind normaler Text.
4. Kein Snippet aus der Notiz: `TaskSummary` bleibt ohne Notiztext; die Palette zeigt bei reinen Notiztreffern
   „in Notiz".
**Konsequenzen:** Messung (`bench_storage`, Release, 10 000 Tasks): ein Wort in allen Tasks p95 ≈ 6–7 ms, seltenes
Teilwort ≈ 0,1–0,2 ms, kurzes Wort (`LIKE`) ≈ 1–1,5 ms – Budget 16 ms. Die Datenbank wächst um den Trigram-Index
(grob 3× Textmenge von Titel + Notiz). Neues lesendes Command `search_tasks` (kein Sync). Die Web-App (N1) braucht
eine eigene Antwort (Standard-Implementierung oder SQLite-WASM mit FTS5).

---

## ADR-029 · Notes: Notizen im Op-Log, Inhalt als Revisions-DAG mit deterministischem 3-Wege-Merge

**Status:** akzeptiert (2026-10-20, User-Freigabe zur Umsetzung; Domain mit N5-01, Merge mit N5-02). Details:
`notes.md`.
**Kontext:** Die App bekommt einen zweiten Modus „Notes" mit `.txt`- und `.md`-Notizen in verschachtelten Ordnern,
offline-first und über Proton Drive synchronisiert. Feld-LWW für den Inhalt (wie `Task.notes`) verliert bei
Offline-Bearbeitung auf zwei Geräten einen Stand. Ein Text-CRDT (`yrs`) kostet ~200–400 KB WASM, RAM und eine
wachsende Historie, und für einen einzelnen Nutzer ist echtes Co-Editing kein Ziel. Lesbare Dateien in Drive würden
einen zweiten, veränderlichen Sync-Pfad brauchen und brächen „jedes Gerät schreibt nur eigene Dateien".
**Entscheidung:**
1. Notizen und Ordner sind Entitäten im **bestehenden Op-Log** (`EntityRef::Note`/`NoteFolder` angehängt). Name,
   Ordner, Typ, Reihenfolge und Papierkorb sind LWW-Felder. `RemoteStore` bleibt unverändert.
2. Der **Inhalt** ist eine wachsende Menge unveränderlicher Revisionen (`OpKind::Revise { parents, text }`, ID =
   `OpId`). Eine lokale Bearbeitung verweist auf die Revision, die der Editor geladen hatte.
3. Mehrere Köpfe werden mit `resolve_body` aufgelöst: nach HLC sortiert gefaltet, Basis = jüngster gemeinsamer
   Vorfahr, zeilenbasierter **diff3**. Das ist eine reine Funktion, also kommt jedes Gerät mit derselben Menge zum
   selben Text (Konvergenz).
4. Überlappende Hunks oder eine fehlende Basis: Der Kopf mit der höheren HLC gewinnt, der andere wird
   **Konfliktkopie** mit deterministischer ID (UUIDv5 aus Notiz-ID + Revisions-ID, wie ADR-028).
5. Nach jedem Pull legt ein Folge-Commit (`ResolveNoteHeads`) die Köpfe in einer Merge-Revision zusammen.
6. Papierkorb = LWW-Feld `Trashed` statt Tombstone. Endgültiges Löschen = `Delete` (Delete gewinnt). Waisen eines
   gelöschten Ordners landen in der Wurzel, Ordner-Zyklen werden beim Materialisieren deterministisch gebrochen.
7. In der Outbox ersetzt eine neue ungepushte Revision derselben Notiz die vorige. Für Inhalte gilt eine
   Upload-Höchstwartezeit von 30 s (Nachtrag zu ADR-022), sofort bei Notiz-/Moduswechsel, Fokusverlust und Schließen.
   Segmente werden auf 2 MiB Klartext begrenzt.
8. Editor = natives `textarea` + eigener Markdown-Renderer (kein CodeMirror), im Lazy-Chunk.
**Alternativen:** Feld-LWW (Datenverlust), immer Konfliktkopie (Kopien auch bei harmlosen Änderungen), `yrs`
(Budgets), echte Dateien in Drive (zweiter Sync-Pfad, Konflikte auf Drive-Ebene), CodeMirror 6 (~150 KB gzip).
**Konsequenzen:** Neue Formatteile (alle angehängt), ältere Apps überspringen Notiz-Segmente mit „App aktualisieren".
Alle Geräte müssen gleichzeitig aktualisieren. diff3-Crate (Kandidat `diffy`) wird in N5-02 nach Größe und wasm32
geprüft. Revisionen werden lokal aufgeräumt, sobald alle aktiven Geräte einen Nachfahren gesehen haben. Fehlt eine
Basis trotzdem, entsteht eine Konfliktkopie, kein Verlust. Gleichzeitiges Bearbeiten und endgültiges Löschen verliert
die Bearbeitung (bewusst, User-Entscheidung 2026-10-20).
**Nachtrag N5-02 (2026-10-20):** Kein `diffy`, eigener Diff in `sync/text_merge.rs` (~250 Zeilen): Myers mit
Präfix-/Suffix-Abgleich, ab 1 000 Bearbeitungsschritten ein einziger Hunk (begrenzt Laufzeit und Speicher bei völlig
neuen Texten; dann eben Konfliktkopie). Grund: Die Regel „benachbarte Zeilen sind kein Konflikt, dieselbe Zeile
schon" (klassisches diff3/Git wertet Nachbarzeilen als Konflikt) lässt sich so direkt umsetzen, und es kommt keine
Abhängigkeit dazu. Konflikt = zwei Hunks betreffen dieselbe Basiszeile, zwei verschiedene Einfügungen an derselben
Stelle oder eine Einfügung mitten in einem ersetzten Bereich; gleiche Änderungen beider Seiten zählen einmal.
Merge-Basis: gemeinsamer Vorfahr; gibt es mehrere gleichrangige (Kreuz-Merges), wird ihre eigene Auflösung die Basis
(wie Git „recursive"). Zwei unabhängige erste Revisionen setzen auf dem leeren Text auf. Köpfe mit gleichem Text
brauchen keine Merge-Revision; die nächste Bearbeitung nennt sie alle als Eltern (`parents_for_edit`).
**Nachtrag N5-03 (2026-10-20):**
- **Format:** `EntityRef::Note`/`NoteFolder`, Felder `ParentFolder` (13), `NoteKind` (14), `AutoName` (15), `Trashed`
  (16), Werte `Folder`, `NoteKind`, `Trashed`, `OpKind::Revise` – alles angehängt; Golden File `ops_v1_notes.bin`,
  `ops_v1.bin` unverändert. Snapshot-Format **Version 2** hängt die gespeicherten Revisionen als `Revise`-Ops an
  (`SnapshotBody.revisions`); Version 1 bleibt lesbar, ältere Apps überspringen Version 2 („neueres Format").
- **Auto-Name ist kein Sync-Feld:** Mit `auto_name` leitet jedes Gerät den Namen aus dem *aufgelösten* Inhalt ab
  (`Name` wird erst nach dem ersten Umbenennen synchronisiert). Sonst könnte nach einem Merge der LWW-Name zum
  unterlegenen Text passen, und jeder Tastendruck in der ersten Zeile erzeugte ein `Set`.
- **`ResolveNoteHeads` läuft im selben Commit** (User-Entscheidung 2026-10-20): `record_local` und `apply_remote`
  legen nach jeder Änderung Köpfe mit verschiedenem Text zu einer Merge-Revision zusammen (lokale Op in der Outbox)
  und legen Konfliktkopien an; die UI sieht nie einen ungemergten Stand. Kein eigener Engine-Schritt.
- **Konfliktkopie** heißt „<Name> (Konflikt TT.MM.JJJJ)" (Datum der unterlegenen Revision, UTC) ohne Gerätenamen
  (User-Entscheidung 2026-10-20): Der Core kennt die Namen anderer Geräte nicht, und das Ergebnis ist überall gleich.
  Existiert die Kopie schon (anderes Gerät war schneller) oder ist sie gelöscht, entsteht keine zweite.
- **Zusammenfassen in der Outbox:** Ersetzt werden die neueste ungepushte `Revise`-Op (wenn sie Kopf und Basis der
  Bearbeitung ist) und ungepushte `Set`s derselben Notiz und desselben Felds – mit **gleicher Nummer** und neuer HLC
  (Segmente verlangen lückenlose Nummern). **Rennen mit dem Upload:** Die Engine ruft vor jedem Upload
  `freeze_outbox(seq_to)` (persistent); ab dann sind diese Ops unveränderlich, auch nach Abbruch oder Absturz. Sonst
  könnte eine Bearbeitung zwischen Lesen und `mark_ops_pushed` verloren gehen oder dieselbe Op-ID mit zwei Inhalten
  im Backend landen. SQLite findet die Kandidaten über `outbox_ops.entity_id/slot` (Migration 0004).
- **Segmente** sind zusätzlich auf 2 MiB Klartext begrenzt (`MAX_SEGMENT_PLAIN_BYTES`).
- **Aufräumen** (§4.5) läuft mit der Kompaktierung nach einem eigenen Snapshot: je Ursprungsgerät die kleinste
  gesehene Op-Nummer über alle aktiven Geräte (`devices/*.json`, 90 Tage); eine unlesbare Gerätedatei verhindert es.
- **Scheduler:** `SchedulerEvent::ContentChange` (Debounce 3 s, höchstens 30 s, solange die Serie nur Inhalte
  enthält) und `Flush` (sofort, wenn etwas wartet – Notiz-/Moduswechsel). Nachtrag zu ADR-022.
- **Ports:** `NoteRepository` (Lesen) ist Supertrait von `TaskRepository`; so bleibt es bei einem Repository für
  `CoreApi`, Engine und Shells. Neue Sync-Methoden `freeze_outbox`, `revision_ops`, `prune_revisions`.

---

## Spikes

| ID | Frage | Ergebnis entscheidet | Stand |
|---|---|---|---|
| **S-1** | Proton-Login aus Third-Party-Client: SRP, 2FA (TOTP/FIDO2), Two-Password-Mode, Human Verification, Token-Refresh. ToS/Zulässigkeit. CORS für Web. | Machbarkeit Sync überhaupt; Web-Sync-Weg (R-1, R-3) | erledigt (P2) → ADR-013; CORS offen bis zur Web-Etappe |
| **S-2** | Proton Drive aus Rust: offizielles SDK (Sprachen, Lizenz, Stabilität) vs. Community-Crates vs. Eigenbau gegen die API | Implementierungsweg `proton-adapter` | erledigt (P2) → ADR-013 (Eigenbau) |
| **S-3** | `rusqlite` auf wasm32 vs. sqlite-wasm (JS) mit OPFS vs. IndexedDB: Größe, Startzeit, Komplexität | ADR-004 Web-Teil | **offen** (`roadmap-next.md` N1-01) |
| **S-4** | Tauri-2-Hello-World mit Svelte-Liste (1 000 Tasks) auf Arch: PSS, Kaltstart, Scroll-FPS, NVIDIA/Intel/AMD | Bestätigung ADR-001 bzw. GTK4-Ausstieg | erledigt (P7-02, Nachmessung P8-03) → ADR-001; NVIDIA/Intel offen |

Bis S-1/S-2 abgeschlossen waren, wurde gegen `InMemoryRemoteStore`/`FsRemoteStore` entwickelt – die Sync-Engine war
dadurch vollständig testbar, bevor Proton angebunden war.
