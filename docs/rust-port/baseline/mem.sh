#!/bin/bash
# Total PSS and RSS in MB of a process and its child processes (threads are not double-counted).
# Usage: mem.sh <pid>
all() { echo "$1"; for c in $(pgrep -P "$1"); do all "$c"; done; }
pss=0; rss=0
for p in $(all "$1"); do
  [ -r "/proc/$p/smaps_rollup" ] || continue
  pss=$((pss + $(awk '/^Pss:/{print $2}' "/proc/$p/smaps_rollup")))
  rss=$((rss + $(awk '/^Rss:/{print $2}' "/proc/$p/smaps_rollup")))
done
echo "pss_mb=$((pss/1024)) rss_mb=$((rss/1024))"
