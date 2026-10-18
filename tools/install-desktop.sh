#!/usr/bin/env bash
# Installiert bzw. aktualisiert die Desktop-App für den aktuellen Benutzer (ohne sudo, ohne Paket):
#   ~/.local/bin/tasks-app                                   Release-Binary
#   ~/.local/share/applications/dev.tasks.app.desktop        Starter im App-Menü
#   ~/.local/share/icons/hicolor/<größe>/apps/dev.tasks.app.png
# Daten, Schlüsselbund und Sitzung bleiben bei einer Aktualisierung erhalten (liegen nicht hier).
#
# Aufruf:  tools/install-desktop.sh [--uninstall]   (bzw. `just desktop-install`)
# Variable: PREFIX (Standard ~/.local)
set -euo pipefail
cd "$(dirname "$0")/.."

PREFIX=${PREFIX:-$HOME/.local}
BIN=$PREFIX/bin/tasks-app
DESKTOP=$PREFIX/share/applications/dev.tasks.app.desktop
ICONS=$PREFIX/share/icons/hicolor
SIZES=(32x32 128x128 256x256 512x512)

if [[ ${1:-} == --uninstall ]]; then
    rm -f "$BIN" "$DESKTOP"
    for s in "${SIZES[@]}"; do rm -f "$ICONS/$s/apps/dev.tasks.app.png"; done
    echo "Deinstalliert (Daten unter ~/.local/share/dev.tasks.app bleiben erhalten)."
    exit 0
fi

(cd apps/desktop && pnpm install --frozen-lockfile --silent && pnpm tauri build --no-bundle)

# Laufende installierte Instanz beenden, sonst ist das Binary „busy“.
pkill -x tasks-app 2>/dev/null && sleep 0.5 || true

install -Dm755 target/release/todo-desktop "$BIN"
src=apps/desktop/src-tauri/icons
install -Dm644 "$src/32x32.png" "$ICONS/32x32/apps/dev.tasks.app.png"
install -Dm644 "$src/128x128.png" "$ICONS/128x128/apps/dev.tasks.app.png"
install -Dm644 "$src/128x128@2x.png" "$ICONS/256x256/apps/dev.tasks.app.png"
install -Dm644 "$src/icon.png" "$ICONS/512x512/apps/dev.tasks.app.png"

mkdir -p "$(dirname "$DESKTOP")"
cat >"$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=Tasks
GenericName=Aufgaben
Comment=Aufgaben offline-first, verschlüsselt synchronisiert
Exec=$BIN
Icon=dev.tasks.app
Terminal=false
Categories=Office;ProjectManagement;
StartupWMClass=dev.tasks.app
EOF

update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true

echo "Installiert: $BIN ($(git rev-parse --short HEAD)$(git diff --quiet HEAD -- . ':!development' || echo ', mit lokalen Änderungen'), $(date '+%F %T'))"
