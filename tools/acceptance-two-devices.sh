#!/usr/bin/env bash
# Zwei-Geräte-Abnahme (P8-02) mit der echten Desktop-App (Debug-Build, WebKitGTK, Tauri-IPC, SQLite,
# Sync-Engine): zwei Profile A und B (`TODO_DATA_DIR`) synchronisieren über ein gemeinsames Verzeichnis
# (`TODO_REMOTE_DIR`, nur Debug-Builds – Proton-Lauf siehe development/prototype-acceptance.md).
# Jeder App-Lauf führt eine Selbsttest-Suite aus (`apps/desktop/src/selftest.ts`, `p802*`), meldet
# PASS/FAIL auf stdout und beendet sich; das Skript startet die Läufe, beendet einen hart (`kill -9`)
# und vergleicht die Ergebnisse beider Geräte.
#
# Aufruf:  tools/acceptance-two-devices.sh [AUSGABE-ORDNER]
# Fenster öffnen sich sichtbar (keine Eingaben am Desktop nötig). Dauer: ~3–5 min.
# Abschnitt 5 (P0-07): Sammelaktionen der Mehrfachauswahl über beide Profile.
# Abschnitt 6 (P1-03): Abhaken verlässt den Eingang, Upload ohne manuellen Sync.
# Abschnitt 7 (P2-09): Board-Ziehen, Listen umbenennen/umsortieren über beide Profile.
set -uo pipefail
cd "$(dirname "$0")/.."

OUT=${1:-$(mktemp -d -t acceptance-XXXX)}
mkdir -p "$OUT"
OUT=$(realpath "$OUT")
REMOTE=$OUT/remote
LOG=$OUT/protocol.txt
APP=target/debug/todo-desktop
CLI=target/debug/todo-cli
: >"$LOG"
failures=0

say() { echo "$*" | tee -a "$LOG"; }
check() { # check NAME BEDINGUNG-ALS-EXITCODE
    if [ "$2" -eq 0 ]; then say "  ✔ $1"; else say "  ✘ $1"; failures=$((failures + 1)); fi
}

say "== Build (Debug) =="
cargo build -q -p todo-desktop -p todo-cli || exit 1

# Ein Vite-Server für alle Läufe; die Suite kommt zur Laufzeit aus TASKS_SELFTEST.
(cd apps/desktop && VITE_TASKS_SELFTEST=runtime exec pnpm exec vite) >"$OUT/vite.log" 2>&1 &
VITE=$!
trap 'kill $VITE 2>/dev/null; wait $VITE 2>/dev/null' EXIT
for _ in $(seq 100); do curl -sf http://localhost:1420 >/dev/null && break; sleep 0.2; done
curl -sf http://localhost:1420 >/dev/null || { echo "Vite startet nicht (Port 1420 belegt?)"; exit 1; }

# start PROFIL SUITE [VERZÖGERUNG_MS] → PID (eigene Prozessgruppe), Ausgabe in $OUT/PROFIL-SUITE.log
start() {
    local log="$OUT/$1-$2.log"
    : >"$log"
    TODO_DATA_DIR=$OUT/$1 TODO_REMOTE_DIR=$REMOTE TASKS_SELFTEST=$2 TODO_REMOTE_DELAY_MS=${3:-0} \
        setsid "$APP" >"$log" 2>&1 &
    echo $!
}

# finish PROFIL SUITE PID: wartet aufs Ende (max. 5 min), übernimmt die Berichtszeilen ins Protokoll.
finish() {
    local log="$OUT/$1-$2.log" i
    for i in $(seq 3000); do kill -0 "$3" 2>/dev/null || break; sleep 0.1; done
    if kill -0 "$3" 2>/dev/null; then kill -9 -- -"$3"; say "  [$1] Zeitüberschreitung"; fi
    wait "$3" 2>/dev/null
    grep '^SELFTEST' "$log" | grep -v -E 'SELFTEST (SYNC_STARTED|BULK_CREATED|WAITING)$' |
        sed "s/^SELFTEST /  [$1] /" | tee -a "$LOG"
}

run() { local pid; pid=$(start "$@"); finish "$1" "$2" "$pid"; }

passed() { grep -q '^SELFTEST ALLE BESTANDEN' "$OUT/$1-$2.log"; }
state() { grep '^SELFTEST PASS Zustand' "$OUT/$1-$2.log" | tail -1 | sed 's/^SELFTEST PASS Zustand[^–]*– //'; }

wait_for() { # wait_for DATEI MUSTER SEKUNDEN
    local i
    for i in $(seq $(($3 * 10))); do grep -q "$2" "$1" && return 0; sleep 0.1; done
    return 1
}

say ""
say "== 1. Anlegen auf A → erscheint auf B =="
pid_b=$(start B p802wait)
wait_for "$OUT/B-p802wait.log" '^SELFTEST WAITING' 60 || say "  B wurde nicht bereit"
run A p802create
finish B p802wait "$pid_b"
passed A p802create; check "A legt an und lädt hoch" $?
passed B p802wait; check "B sieht „Von A“ und die Liste ohne Neustart" $?

say ""
say "== 2. Beide offline: Titel/Fälligkeit, Löschen/Bearbeiten, Umsortieren → wieder online =="
run A p802offA
run B p802offB
passed A p802offA && passed B p802offB; check "Offline-Änderungen bleiben lokal (nichts hochgeladen)" $?
run A p802sync
cp "$OUT/A-p802sync.log" "$OUT/A-p802sync-1.log"
run B p802sync
run A p802sync
a=$(state A p802sync)
b=$(state B p802sync)
say "  A: $a"
say "  B: $b"
[ -n "$a" ] && [ "$a" = "$b" ]; check "beide Geräte identisch" $?
[[ $a == *"T=T neu von A fällig 2026-10-25"* ]]; check "Titel von A und Fälligkeit von B bleiben beide (feldweises LWW)" $?
[[ $a != *"X "* ]]; check "X gelöscht – Löschen gewinnt gegen Bearbeiten" $?
[[ $a == "Reihenfolge S4 | T neu von A | S2 | S3 | S1 "* ]]; check "Umsortierungen beider Geräte (S4 nach oben, S1 ans Ende)" $?

say ""
say "== 3. App während des Syncs hart beenden (kill -9) → Neustart =="
# Beleg, dass wirklich mitten im Sync abgebrochen wurde: Outbox nicht leer, aber schon Segmente oben.
outbox() { sqlite3 "$OUT/$1/tasks.db" 'SELECT count(*) FROM outbox_ops'; }
segments() { find "$REMOTE/Tasks/ops" -name '*.seg' | wc -l; }
killed() { # killed PID TEXT
    kill -9 -- -"$1" 2>/dev/null && wait "$1" 2>/dev/null
    local pending; pending=$(outbox A)
    say "  [A] kill -9 $2 → Outbox $pending ausstehend, $(segments) Segmente im Sync-Ordner"
    [ "$pending" -gt 0 ]; check "Abbruch traf einen laufenden Sync" $?
}
pid=$(start A p802bulk 400)
if wait_for "$OUT/A-p802bulk.log" '^SELFTEST SYNC_STARTED' 120; then
    sleep 1.5
    grep -h '^SELFTEST PASS' "$OUT/A-p802bulk.log" | sed 's/^SELFTEST /  [A] /' | tee -a "$LOG"
    killed "$pid" "1,5 s nach Sync-Start (Backend 400 ms je Aufruf)"
else
    say "  [A] Sync startete nicht"; failures=$((failures + 1))
fi
# Neustart: der Start-Sync holt nach – auch ihn abschießen (fester Zeitpunkt nach dem Start).
# Langsameres Backend (1 s je Aufruf): Auf einer schnellen Maschine ist beim ersten Abbruch oft nur
# noch ein Segment offen, das bei 400 ms schon vor 2,5 s oben wäre.
pid=$(start A p802sync 1000)
sleep 2.5
killed "$pid" "2,5 s nach Neustart (Start-Sync, Backend 1 s je Aufruf)"
run A p802sync
run B p802sync
count() { "$CLI" --db "$OUT/$1/tasks.db" ls Masse | awk '{print $NF}' | sort | uniq | wc -l; }
total() { "$CLI" --db "$OUT/$1/tasks.db" ls Masse | wc -l; }
ca=$(count A); ta=$(total A); cb=$(count B); tb=$(total B)
say "  Masse: A $ta Tasks ($ca verschiedene Titel), B $tb Tasks ($cb verschiedene Titel)"
[ "$ta" = 2500 ] && [ "$ca" = 2500 ]; check "A: alle 2 500 Tasks, keine verloren" $?
[ "$tb" = 2500 ] && [ "$cb" = 2500 ]; check "B: alle 2 500 Tasks, keine Duplikate" $?
[ "$(state A p802sync)" = "$(state B p802sync)" ]; check "beide Geräte identisch" $?
la=$("$CLI" --db "$OUT/A/tasks.db" lists | grep -c Masse); lb=$("$CLI" --db "$OUT/B/tasks.db" lists | grep -c Masse)
[ "$la" = 1 ] && [ "$lb" = 1 ]; check "Liste „Masse“ genau einmal" $?

say ""
say "== 4. Sitzung abgelaufen → lokal nutzbar, Status „Anmeldung erforderlich“ =="
touch "$REMOTE/.auth-revoked"
run A p802auth
passed A p802auth; check "A zeigt „Anmeldung erforderlich“ und arbeitet lokal weiter" $?
rm "$REMOTE/.auth-revoked"
run A p802sync
run B p802sync
"$CLI" --db "$OUT/B/tasks.db" ls | grep -q "Ohne Sitzung"; check "nach erneuter Anmeldung: B sieht die lokal angelegte Task" $?

say ""
say "== 5. Mehrfachauswahl: A löscht und verschiebt je 50, sortiert einen Block um → B gleich, Neustart A gleich =="
run A p0bulkA
passed A p0bulkA; check "A: Sammel-Löschen, -Verschieben und Block-Umsortieren über die UI" $?
run B p0bulkB
run A p0bulkLocal
a=$(state A p0bulkA)
b=$(state B p0bulkB)
r=$(state A p0bulkLocal)
say "  A: ${a:0:120}…"
say "  B: ${b:0:120}…"
[ -n "$a" ] && [ "$a" = "$b" ]; check "B sieht dasselbe wie A (Reihenfolge, Listen)" $?
[ "$a" = "$r" ]; check "nach Neustart von A unverändert" $?
[[ $a == "Sammel 100: S102 S104 S101 S103 S105 "* ]]; check "50 gelöscht, Block S101+S103 hinter S104" $?
[[ $a == *"· Ziel 50: S051 S052 "* ]]; check "50 in „Ziel“, Reihenfolge erhalten" $?

say ""
say "== 6. Abhaken im Eingang → verschwindet, automatisch hochgeladen → B sieht sie unter „Erledigt“ =="
run A p1A
passed A p1A; check "A: Zeile verlässt den Eingang, Upload ohne manuellen Sync in ≤ 6 s" $?
run B p1B
a=$(state A p1A)
b=$(state B p1B)
say "  A: $a"
say "  B: $b"
[ "$a" = "Abhaken-Test erledigt=true im Eingang=false" ] && [ "$a" = "$b" ]; check "B: erledigt, nicht mehr im Eingang" $?

say ""
say "== 7. Listen & Board: A zieht eine Karte in eine andere Liste, benennt um, sortiert um → B gleich, Neustart A gleich =="
run A p2syncA
passed A p2syncA; check "A: Board-Ziehen, Umbenennen und Umsortieren über die UI" $?
run B p2syncB
run A p2syncLocal
a=$(state A p2syncA)
b=$(state B p2syncB)
r=$(state A p2syncLocal)
say "  A: $a"
say "  B: $b"
[ "$a" = "Listen Haushalt | Obst · Äpfel in Obst" ] && [ "$a" = "$b" ]; check "B: dieselbe Zuordnung, Namen und Reihenfolge" $?
[ "$a" = "$r" ]; check "nach Neustart von A unverändert" $?

say ""
if [ $failures -eq 0 ]; then say "ERGEBNIS: alle Prüfungen bestanden"; else say "ERGEBNIS: $failures Prüfung(en) fehlgeschlagen"; fi
say "Protokoll: $LOG"
exit $((failures > 0))
