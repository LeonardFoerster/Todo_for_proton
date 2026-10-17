# Entwickler-Kommandos. Übersicht: `just --list`

# Standard-Prüfung vor jedem Commit: Format, Lints, Tests
check: fmt-check clippy test doc wasm ts

# Alle Rust-Tests im Workspace (`--all-targets` enthält keine Doctests, daher zweiter Lauf).
# `--all-features`, damit Tests mit `required-features = ["test-support"]` nicht still entfallen.
test:
    cargo test --workspace --all-targets --all-features
    cargo test --workspace --doc --all-features
    # Kompression ohne C-zstd (Pfad der Web-App) separat, da --all-features beide aktiviert.
    cargo test -p todo-sync --lib --no-default-features --features pure-rust-zstd

# Code formatieren
fmt:
    cargo fmt --all

# Formatierung nur prüfen (ändert nichts)
fmt-check:
    cargo fmt --all --check

# Clippy mit Warnungen als Fehler
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# API-Doku bauen, Warnungen (z. B. kaputte Links) = Fehler
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Kompiliert die Web-Composition-Root und todo-core inkl. Tests (?Send-Ports) für wasm32.
# todo-core allein braucht dort `uuid/js` als Zufallsquelle (liefert sonst todo-wasm).
wasm:
    cargo check -p todo-wasm --target wasm32-unknown-unknown
    cargo check -p todo-core --target wasm32-unknown-unknown --all-targets --features uuid/js,test-support

# Desktop-App im Dev-Modus (Vite-Dev-Server + Tauri-Fenster mit Hot-Reload)
desktop-dev:
    cd apps/desktop && pnpm install --frozen-lockfile && pnpm tauri dev

# Desktop-App als Release-Binary bauen (ohne Pakete; Packaging kommt nach dem Prototyp)
desktop-build:
    cd apps/desktop && pnpm install --frozen-lockfile && pnpm tauri build --no-bundle

# TS-Typen der DTOs neu erzeugen (packages/core-client/src/generated; läuft auch mit `just test`)
types:
    cargo test -p todo-core -p todo-desktop --lib export_bindings

# TS/Svelte: ESLint, Typecheck (tsc/svelte-check) und Vitest über alle pnpm-Pakete
ts:
    pnpm install --frozen-lockfile --silent
    pnpm lint
    pnpm check
    pnpm test
