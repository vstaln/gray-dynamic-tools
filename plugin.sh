#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/gray-dynamic-tools.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/gray-dynamic-tools" ;;
    *) rel="x86_64-apple-darwin/gray-dynamic-tools" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/gray-dynamic-tools" ;;
    *) rel="x86_64-unknown-linux-musl/gray-dynamic-tools" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/gray-dynamic-tools.exe" "$here/target/release/gray-dynamic-tools" "$here/target/debug/gray-dynamic-tools.exe" "$here/target/debug/gray-dynamic-tools"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "gray-dynamic-tools binary not found in $here" >&2
exit 1
