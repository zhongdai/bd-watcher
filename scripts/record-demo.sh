#!/usr/bin/env bash
# Regenerate assets/demo.cast by recording bd-watcher against a fake `bd`
# shim with a scripted sequence of status transitions. No real project
# data is involved.
#
# Requirements: asciinema 3.x, python3, cargo (release binary is built
# into ./target/release/bd-watcher).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/assets/demo.cast"
COLS=140
ROWS=36
FRAMES=10          # number of state frames in the shim
INTERVAL=3         # bd-watcher poll interval (seconds)
DURATION=$((FRAMES * INTERVAL))   # total recording window

command -v asciinema >/dev/null || { echo "asciinema not found on PATH"; exit 1; }
command -v python3   >/dev/null || { echo "python3 not found on PATH"; exit 1; }
command -v tmux      >/dev/null || { echo "tmux not found on PATH (needed to give asciinema a sized pty in non-interactive runs)"; exit 1; }

# Use /tmp (not TMPDIR) so the cast header shows a short, uninformative
# path like /tmp/bdw-demo-XXXX/... rather than the full macOS private
# /var/folders/... path with a user-specific UUID.
TMPDIR="$(mktemp -d /tmp/bdw-demo-XXXX)"
trap 'rm -rf "$TMPDIR"' EXIT

# ---- Fake `bd` shim ------------------------------------------------------
mkdir -p "$TMPDIR/bin"
cat > "$TMPDIR/bin/bd" <<'SHIM'
#!/usr/bin/env bash
exec python3 "$DEMO_BD_SHIM" "$@"
SHIM
chmod +x "$TMPDIR/bin/bd"

# ---- bd-watcher binary ---------------------------------------------------
if [ ! -x "$ROOT/target/release/bd-watcher" ]; then
    echo "Building release binary..."
    (cd "$ROOT" && cargo build --release --quiet)
fi

# ---- Record --------------------------------------------------------------
echo "Recording ${DURATION}s into $OUT ..."
export PATH="$TMPDIR/bin:$PATH"
export DEMO_BD_SHIM="$ROOT/scripts/demo_bd_shim.py"
export DEMO_BD_STATE_FILE="$TMPDIR/state"
# Fake repo dir — just needs to exist; the shim ignores it.
mkdir -p "$TMPDIR/repo"

cat > "$TMPDIR/run_watcher.sh" <<RUN
#!/usr/bin/env bash
"$ROOT/target/release/bd-watcher" demo-s04 --repo "$TMPDIR/repo" --interval $INTERVAL &
PID=\$!
sleep $DURATION
kill -TERM \$PID 2>/dev/null || true
wait \$PID 2>/dev/null || true
RUN
chmod +x "$TMPDIR/run_watcher.sh"

# Run asciinema inside a detached tmux session so it gets a proper pty at
# the requested size even when this script runs headless (no controlling
# TTY). tmux writes a marker file when the record finishes; we poll for it
# and then tear down the session.
DONE_MARKER="$TMPDIR/.recording-done"
cat > "$TMPDIR/record_inside_tmux.sh" <<TMUXCMD
#!/usr/bin/env bash
asciinema rec --overwrite --title "bd-watcher — single-epic DAG demo" --command "$TMPDIR/run_watcher.sh" "$OUT"
touch "$DONE_MARKER"
TMUXCMD
chmod +x "$TMPDIR/record_inside_tmux.sh"

SESSION="bdw-demo-$$"
tmux new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" \
    "PATH='$TMPDIR/bin:$PATH' DEMO_BD_SHIM='$DEMO_BD_SHIM' DEMO_BD_STATE_FILE='$DEMO_BD_STATE_FILE' '$TMPDIR/record_inside_tmux.sh'"

# Poll until the inner script marks itself done, then clean up.
MAX_WAIT=$((DURATION + 30))
waited=0
while [ ! -f "$DONE_MARKER" ] && [ $waited -lt $MAX_WAIT ]; do
    sleep 1
    waited=$((waited + 1))
done
tmux kill-session -t "$SESSION" 2>/dev/null || true

[ -f "$DONE_MARKER" ] || { echo "Recording timed out (no marker after ${MAX_WAIT}s)"; exit 1; }

echo "Done. Wrote $OUT"
