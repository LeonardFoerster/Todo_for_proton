# Android (später)

Android ist Priorität 3. Dieses Dokument hält fest, **was heute schon gilt**, damit Android später ohne Umbau
möglich ist, und welche Wege offenstehen.

## 1. Was bereits heute sichergestellt wird

| Vorkehrung | Wo |
|---|---|
| Business-Logik + Sync in Rust, ohne Linux-spezifische APIs | `todo-core`, `todo-sync` |
| Keine Runtime (tokio) im Core | ADR-002 |
| FFI-freundliche Fassade (owned DTOs, Enum-Fehler) | ADR-008, `todo_core::api` |
| Plattform-Dinge hinter Ports: `KeyStore`, `Clock`, `TaskRepository`, `RemoteStore` | `interfaces.md` |
| UI nur gegen `CoreClient` → neue Implementierung reicht | `packages/core-client` |
| Responsive Layout bis < 700 px, Touch-Zeilenhöhe 44 px | `ui-guidelines.md` |
| SQLite als Speicher (läuft nativ auf Android) | ADR-004 |
| Proton-Adapter in Rust → läuft auch auf `aarch64-linux-android` | `crates/proton-adapter` |

## 2. Option A – Tauri 2 Mobile (bevorzugt für den Start)

- Gleiche Svelte-UI, gleiche Rust-Composition-Root wie Desktop; Tauri 2 unterstützt Android offiziell.
- Neuer Ordner: Tauri generiert `apps/desktop/src-tauri/gen/android` (oder eigenes `apps/mobile`, falls die
  Konfiguration stark abweicht).
- WebView = Android System WebView (Chromium-basiert) → meist schneller als WebKitGTK.
- **Vorteile:** minimaler Mehraufwand, eine UI für alle Plattformen.
- **Nachteile:** Gefühl nicht 100 % nativ (Gesten, System-Back, Material-You-Farben brauchen Feinarbeit);
  Widgets/Quick-Settings-Tiles erfordern trotzdem Kotlin.

Nötige Arbeiten:
- `KeyStore`-Impl über Android Keystore (Tauri-Plugin oder eigenes Kotlin-Plugin).
- Background-Sync: WorkManager-Job über Tauri-Plugin, der die Rust-Sync-Engine aufruft.
- Benachrichtigungen für Fälligkeiten (`tauri-plugin-notification`).
- Touch-Anpassungen in `packages/ui` (Swipe zum Abhaken/Löschen, größere Hit-Targets, Bottom-Sheet statt Detail-Spalte).

## 3. Option B – Native Kotlin/Jetpack-Compose-UI + Rust-Core via UniFFI

- `crates/todo-ffi` (neu) mit UniFFI-Bindings auf `todo_core::api` → generierte Kotlin-API.
- UI komplett in Compose; Core, Sync, Storage, Proton-Adapter bleiben identisch.
- **Vorteile:** echtes natives Gefühl, beste Akku-/Speichereffizienz, Widgets einfach.
- **Nachteile:** zweite UI-Codebasis, die gepflegt werden muss.
- Wird gewählt, wenn Option A in der Praxis nicht gut genug ist – dank ADR-008 ohne Core-Umbau.

## 4. Entscheidungskriterien (wenn es so weit ist)

1. Wie gut fühlt sich die Tauri-Mobile-UI mit Touch-Anpassungen an? (Prototyp, 1 Woche)
2. Braucht es Widgets / tiefe Systemintegration im ersten Android-Release?
3. Kapazität für eine zweite UI-Codebasis?

## 5. Android-spezifische Fallstricke

- Cross-Compiling Rust: NDK-Version fixieren, `cargo-ndk`; OpenSSL vermeiden → `rustls` im HTTP-Client
  (gilt bereits jetzt für `proton-adapter`).
- `bundled` SQLite (rusqlite-Feature) statt System-SQLite.
- Doze/App-Standby: kein dauerhafter Hintergrund-Sync; WorkManager mit Netzwerk-Constraint.
- APK-Größe: Rust-Libs pro ABI (arm64-v8a reicht weitgehend), `strip`, LTO.
- Proton-Login auf Mobilgeräten: Human Verification/CAPTCHA im WebView lösen; FIDO2 ggf. eingeschränkt.
