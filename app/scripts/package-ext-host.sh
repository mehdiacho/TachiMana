#!/usr/bin/env bash
# Packs the extension helper for one platform (PLAN.md §P8.4): ext-host.jar, its lib/ folder and a
# jlink runtime holding only the modules the host uses, zipped with a .sha256 beside it. The app
# starts `<helper>/runtime/bin/java` when it is there (tm-exthost/src/host.rs).
#
#   app/scripts/package-ext-host.sh <platform> <out-dir>     e.g. linux-x64 dist
#
# Needs a JDK (jdeps, jlink) and `./gradlew :ext-host:fatJar` to have run.
set -euo pipefail
platform=${1:?platform, e.g. linux-x64 or windows-x64}
out=${2:?output folder}
root=$(cd "$(dirname "$0")/../.." && pwd)
host="$root/ext-host/build/ext-host"
test -f "$host/ext-host.jar" || { echo "build the host first: ./gradlew :ext-host:fatJar" >&2; exit 1; }

sep=":"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) sep=";" ;; esac
cp=$(cd "$host" && ls lib/*.jar | paste -sd "$sep" -)

# What the classes need, plus what is only reached by reflection or service loading: TLS
# (crypto.ec), extra charsets, locale data for date parsing in sources, and Unsafe for GraalJS.
found=$(cd "$host" && jdeps --ignore-missing-deps --multi-release base --print-module-deps \
  --class-path "$cp" ext-host.jar)
mods="$found,jdk.crypto.ec,jdk.charsets,jdk.localedata,jdk.unsupported,java.naming,jdk.zipfs"
echo "modules: $mods"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage="$work/ext-host"
mkdir -p "$stage"
cp -R "$host/." "$stage/"
jlink --add-modules "$mods" --strip-debug --no-header-files --no-man-pages \
  --compress zip-6 --output "$stage/runtime"

mkdir -p "$out"
out=$(cd "$out" && pwd)
name="tachimana-ext-host-$platform.zip"
rm -f "$out/$name"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) (cd "$work" && powershell -NoProfile -Command "Compress-Archive -Path ext-host/* -DestinationPath '$(cygpath -w "$out/$name")'") ;;
  *) (cd "$stage" && zip -qr -9 "$out/$name" .) ;;
esac
(cd "$out" && sha256sum "$name" > "$name.sha256")
ls -l "$out/$name"
cat "$out/$name.sha256"
