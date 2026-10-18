#!/usr/bin/env bash
# Einmal-Installation auf Arch Linux: Systempakete, Rust-Toolchain, JS-Abhängigkeiten, dann die Desktop-App
# für den aktuellen Benutzer (~/.local/bin/tasks-app, Starter „Tasks“ im App-Menü).
#
# Aufruf (im geklonten Repo, als normaler Benutzer – sudo wird nur für pacman verwendet):
#   ./install.sh               fehlende Pakete installieren (mit Rückfrage von pacman), App bauen und installieren
#   ./install.sh --noconfirm   pacman ohne Rückfragen
#
# Erneut ausführen ist unschädlich: vorhandene Pakete werden übersprungen, die App wird aktualisiert.
# Später reicht zum Aktualisieren `just desktop-install` bzw. `tools/install-desktop.sh`.
set -euo pipefail
cd "$(dirname "$0")"

PACKAGES=(base-devel git rustup nodejs pnpm just webkit2gtk-4.1 openssl librsvg desktop-file-utils)
PACMAN_FLAGS=(--needed)
[[ ${1:-} == --noconfirm ]] && PACMAN_FLAGS+=(--noconfirm)

say() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
die() { printf '\033[31mFehler:\033[0m %s\n' "$*" >&2; exit 1; }

[[ $EUID -ne 0 ]] || die "bitte als normaler Benutzer ausführen (sudo wird nur für pacman genutzt)"
command -v pacman >/dev/null || die "pacman nicht gefunden – dieses Skript ist für Arch Linux"

say "Systempakete"
mapfile -t missing < <(pacman -T "${PACKAGES[@]}" || true)
# Werkzeuge, die schon anders installiert sind (z. B. rustup/just über ~/.cargo/bin), nicht doppelt holen.
for tool in rustup just pnpm git; do
    if command -v "$tool" >/dev/null; then
        for i in "${!missing[@]}"; do
            if [[ ${missing[i]} == "$tool" ]]; then unset 'missing[i]'; fi
        done
    fi
done
missing=("${missing[@]}")
if ((${#missing[@]})); then
    echo "Fehlt: ${missing[*]}"
    # `rust` aus den Paketquellen kollidiert mit `rustup` – pacman fragt dann, ob es ersetzt werden soll.
    if pacman -Qq rust &>/dev/null && [[ " ${missing[*]} " == *" rustup "* ]]; then
        echo "Hinweis: Das Paket „rust“ wird durch „rustup“ ersetzt (die Version legt rust-toolchain.toml fest)."
    fi
    sudo pacman -S "${PACMAN_FLAGS[@]}" "${missing[@]}"
else
    echo "alle vorhanden"
fi

say "Rust-Toolchain (aus rust-toolchain.toml)"
# Ohne Standard-Toolchain verweigern manche rustup-Versionen jeden Aufruf; die fixierte Version gilt im Repo trotzdem.
rustup default &>/dev/null || rustup default stable
rustup toolchain install
rustc --version

say "JavaScript-Abhängigkeiten"
pnpm install --frozen-lockfile

say "App bauen und installieren (Release-Build, dauert beim ersten Mal einige Minuten)"
tools/install-desktop.sh

say "Prüfungen"
case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "Hinweis: ~/.local/bin ist nicht im PATH – die App startet trotzdem über das App-Menü („Tasks“)." ;;
esac
if busctl --user status org.freedesktop.secrets &>/dev/null ||
    busctl --user list 2>/dev/null | grep -q '^org\.freedesktop\.secrets'; then
    echo "Schlüsselbund (Secret Service) gefunden – Anmeldung und Sync möglich."
else
    echo "Hinweis: Kein Secret Service gefunden (z. B. gnome-keyring oder kwallet). Die App läuft lokal, aber"
    echo "         Anmeldung und Sync brauchen einen Schlüsselbund: sudo pacman -S gnome-keyring"
fi

say "Fertig – starten mit „Tasks“ im App-Menü oder: tasks-app"
