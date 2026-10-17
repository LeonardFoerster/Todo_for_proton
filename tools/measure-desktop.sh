#!/usr/bin/env bash
# Misst die Desktop-App (Release-Build) gegen die Budgets aus AGENTS.md §4:
#   1. Start bis erster Paint (Median über RUNS Starts; die UI meldet den ersten Frame per `app_ready`)
#   2. PSS über den ganzen Prozessbaum (App + WebKit-Prozesse) nach IDLE_S Sekunden Leerlauf
#   3. Leerlauf-CPU über CPU_S Sekunden (Summe aller Prozesse, in % eines Kerns)
#   4. Binärgröße
#
# Aufruf:  tools/measure-desktop.sh [BINARY]
# Umgebung: RUNS=5 IDLE_S=60 CPU_S=30; alles Weitere (z. B. XDG_DATA_HOME für eine
#           Testdatenbank, GDK_BACKEND) wird an die App durchgereicht.
# Fenster öffnen sich sichtbar – der Compositor drosselt verdeckte/versteckte Fenster.
set -euo pipefail

BIN=${1:-target/release/todo-desktop}
RUNS=${RUNS:-5}
IDLE_S=${IDLE_S:-60}
CPU_S=${CPU_S:-30}
TIMEOUT_S=30
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

[[ -x $BIN ]] || { echo "Binary fehlt: $BIN (zuerst: just desktop-build)" >&2; exit 1; }

now_ms() { date +%s%3N; }

# Alle Nachfahren von $1 (inkl. $1), z. B. WebKitWebProcess/NetworkProcess, ggf. über bwrap.
tree() {
    local pid=$1 child
    echo "$pid"
    for child in $(cat /proc/"$pid"/task/*/children 2>/dev/null); do
        tree "$child"
    done
}

# Startet die App; setzt APP_PID und APP_LOG.
launch() {
    APP_LOG="$TMP/app-$(now_ms).log"
    TODO_MEASURE_STARTUP=1 "$@" "$BIN" >"$APP_LOG" 2>&1 &
    APP_PID=$!
}

# Wartet auf eine Zeile mit Muster $1 im Log; gibt sie aus.
await_line() {
    local pattern=$1 deadline=$(( $(now_ms) + TIMEOUT_S * 1000 ))
    while (( $(now_ms) < deadline )); do
        if grep -m1 "$pattern" "$APP_LOG" 2>/dev/null; then return 0; fi
        kill -0 "$APP_PID" 2>/dev/null || { echo "App beendet:" >&2; cat "$APP_LOG" >&2; return 1; }
        sleep 0.02
    done
    echo "Zeitüberschreitung beim Warten auf '$pattern'" >&2
    cat "$APP_LOG" >&2
    return 1
}

# Beendet die App samt WebKit-Prozessen und wartet, bis alle weg sind – sonst teilen sich zwei
# Instanzen die Bibliotheken und der PSS-Wert der nächsten Messung halbiert sich scheinbar.
stop() {
    local pids p i
    pids=$(tree "$APP_PID")
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
    for i in $(seq 1 100); do
        for p in $pids; do kill -0 "$p" 2>/dev/null && { sleep 0.05; continue 2; }; done
        return 0
    done
    kill -9 $pids 2>/dev/null || true
}

# Andere WebKitGTK-Prozesse teilen sich Bibliotheken mit der App und verfälschen PSS.
others=$(pgrep -f 'WebKit(Web|Network)Process' || true)
if [[ -n $others ]]; then
    echo "Warnung: andere WebKit-Prozesse laufen ($(echo $others | wc -w)) – PSS fällt zu niedrig aus." >&2
fi

field() { sed -n "s/.*$1=\([0-9.]*\).*/\1/p"; }

median() { sort -n | awk '{a[NR]=$1} END {print (NR%2 ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2)}'; }

echo "== Binary: $BIN ($(du -h "$BIN" | cut -f1), $(stat -c %s "$BIN") Bytes)"

echo "== Start bis erster Paint ($RUNS Läufe)"
walls=() mains=()
for i in $(seq 1 "$RUNS"); do
    start=$(now_ms)
    launch env
    line=$(await_line "TODO_MEASURE first_paint")
    wall=$(( $(echo "$line" | field unix_ms) - start ))
    main=$(echo "$line" | field since_main_ms)
    walls+=("$wall") mains+=("$main")
    printf '   Lauf %d: %4d ms (ab main: %d ms)\n' "$i" "$wall" "$main"
    stop
    sleep 1
done
echo "   Median: $(printf '%s\n' "${walls[@]}" | median) ms (ab main: $(printf '%s\n' "${mains[@]}" | median) ms)" \
     "· Ziel < 400 ms, hart < 800 ms"

echo "== Leerlauf: PSS nach ${IDLE_S}s, CPU über ${CPU_S}s"
launch env
await_line "TODO_MEASURE first_paint" >/dev/null
sleep "$IDLE_S"
pids=$(tree "$APP_PID")
total=0
for p in $pids; do
    pss=$(awk '/^Pss:/ {print $2}' /proc/"$p"/smaps_rollup 2>/dev/null || echo 0)
    total=$(( total + ${pss:-0} ))
    printf '   %-22s %7d kB\n' "$(cat /proc/"$p"/comm 2>/dev/null || echo '?')" "${pss:-0}"
done
echo "   PSS gesamt: $(( total / 1024 )) MB · Ziel < 250 MB, hart < 320 MB"
if command -v smem >/dev/null; then
    smem -t -P "$(basename "$BIN")|WebKit" | tail -n 1 | sed 's/^/   smem: /'
fi

ticks() {
    local sum=0 p t
    for p in $(tree "$APP_PID"); do
        # Felder 14/15 (utime/stime) – nach dem „)“ des Prozessnamens zählen.
        t=$(sed 's/.*) //' /proc/"$p"/stat 2>/dev/null | awk '{print $12 + $13}')
        sum=$(( sum + ${t:-0} ))
    done
    echo "$sum"
}
hz=$(getconf CLK_TCK)
t0=$(ticks)
sleep "$CPU_S"
t1=$(ticks)
awk -v d=$(( t1 - t0 )) -v hz="$hz" -v s="$CPU_S" \
    'BEGIN { printf "   CPU im Leerlauf: %.2f %% eines Kerns (%d Ticks in %d s) · Ziel ~0 %%\n", d / hz / s * 100, d, s }'
stop

