#!/bin/bash
# Cold start to a painted Library window, Linux/X11 (used under Xvfb for the P0 baseline).
# Usage: BIN=/opt/tachimana/bin/TachiMana measure-start.sh <data-dir> <label> [out-dir]
# Needs: xdotool, ImageMagick `import`. Prints window_ms (window mapped; xdotool polls every
# 0.5 s, so ±0.5 s) and settled_ms (window contents unchanged for 5 consecutive 50 ms frames).
BIN=${BIN:?set BIN to the app launcher}
DATA=$1; LABEL=$2; OUT=${3:-./measure}; mkdir -p "$OUT"
t0=$(date +%s%N)
SKIKO_RENDER_API=${SKIKO_RENDER_API:-SOFTWARE} TACHIMANA_DATA_DIR=$DATA "$BIN" > "$OUT/$LABEL.app.log" 2>&1 &
PID=$!
WID=$(xdotool search --sync --onlyvisible --name '^TachiMana$' | head -1)
t1=$(date +%s%N)
prev=""; stable=0; t2=""
for i in $(seq 1 300); do
  h=$(import -window "$WID" rgb:- 2>/dev/null | md5sum | cut -c1-32)
  if [ "$h" = "$prev" ]; then stable=$((stable+1)); else stable=0; fi
  prev=$h
  if [ $stable -ge 5 ]; then t2=$(date +%s%N); break; fi
  sleep 0.05
done
echo "$LABEL pid=$PID window_ms=$(( (t1-t0)/1000000 )) settled_ms=$(( (t2-t0)/1000000 - 250 ))"
