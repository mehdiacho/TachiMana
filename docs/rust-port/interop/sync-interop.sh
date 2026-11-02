#!/usr/bin/env bash
# Kotlin <-> Rust LAN sync and backup interop check (P7 gate).
# Builds both halves, makes two libraries, and has each app pull from the other and
# restore the other's backup. Prints both libraries and compares the transferred files.
#
#   docs/rust-port/interop/sync-interop.sh [workdir]
#
# Needs: JDK 17+, cargo, python3. Run from the repo root.
set -euo pipefail
ROOT=$(pwd)
WORK=${1:-$(mktemp -d)}
rm -rf "$WORK/K" "$WORK/R"; mkdir -p "$WORK"
echo "workdir: $WORK"

sh ./gradlew -q ${GRADLE_ARGS:-} :desktop:syncInteropClasspath
(cd app && cargo build -q -p tm-sync --example sync_peer)
CP=$(cat desktop/build/interop-classpath.txt)
K() { java -cp "$CP" tachimana.sync.SyncInteropKt "$@" 2>/dev/null; }
R() { "$ROOT/app/target/debug/examples/sync_peer" "$@"; }

python3 - "$WORK" <<'PY'
import base64, os, sys, zipfile
png = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC")
def manga(root, name, chapters):
    d = os.path.join(root, "local", name); os.makedirs(d, exist_ok=True)
    open(os.path.join(d, "cover.png"), "wb").write(png)
    for c in chapters:
        with zipfile.ZipFile(os.path.join(d, c + ".cbz"), "w") as z:
            for p in ("001.png", "002.png", "003.png"): z.writestr(p, png + c.encode())
w = sys.argv[1]
manga(os.path.join(w, "K"), "Kotlin Manga", ["Chapter 1", "Chapter 2"])
manga(os.path.join(w, "R"), "Rust Manga", ["Chapter 1"])
PY

K seed "$WORK/K" >/dev/null
echo "== 1. Rust pulls from Kotlin"
coproc KS { java -cp "$CP" tachimana.sync.SyncInteropKt serve "$WORK/K" 111111 2>/dev/null; }
read -r line <&"${KS[0]}"; KPORT=${line#READY port=}
R pull "$WORK/R" 127.0.0.1 "$KPORT" 111111 | tee "$WORK/r-after-pull.txt"
exec {KS[1]}>&-; wait "$KS_PID" || true

echo "== 2. Kotlin pulls from Rust"
coproc RS { "$ROOT/app/target/debug/examples/sync_peer" serve "$WORK/R" 222222; }
read -r line <&"${RS[0]}"; RPORT=${line#READY port=}
K pull "$WORK/K" 127.0.0.1 "$RPORT" 222222 | tee "$WORK/k-after-pull.txt"
exec {RS[1]}>&-; wait "$RS_PID" || true

echo "== 3. Transferred files are byte-identical"
for f in "Kotlin Manga/Chapter 1.cbz" "Kotlin Manga/Chapter 2.cbz" "Kotlin Manga/cover.png" \
         "Rust Manga/Chapter 1.cbz" "Rust Manga/cover.png"; do
  cmp "$WORK/K/local/$f" "$WORK/R/local/$f" && echo "same: $f"
done

echo "== 4. Backups: Kotlin file into Rust, Rust file into Kotlin"
K export "$WORK/K" "$WORK/kotlin.tachibk" >/dev/null
R export "$WORK/R" "$WORK/rust.tachibk" >/dev/null
mkdir -p "$WORK/R2" "$WORK/K2"
R import "$WORK/R2" "$WORK/kotlin.tachibk" | tee "$WORK/r2.txt"
K import "$WORK/K2" "$WORK/rust.tachibk" | tee "$WORK/k2.txt"

echo "== 5. Both libraries agree"
K print "$WORK/K" > "$WORK/k-final.txt"
R print "$WORK/R" > "$WORK/r-final.txt"
# Extension stores are opt-in: Kotlin K was seeded with one, Rust R only lists it as pending.
diff <(grep -v '^REPOS' "$WORK/k-final.txt") <(grep -v '^REPOS' "$WORK/r-final.txt") \
  && echo "Kotlin and Rust libraries are identical (apart from opt-in extension stores)"
