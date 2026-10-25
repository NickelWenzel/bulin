#!/bin/sh
# Fetches the pinned wgsl-analyzer-web build (the wgsl-analyzer language server compiled to
# emscripten; MIT OR Apache-2.0) into web/vendor/wgsl-analyzer, which index.html copies into the
# trunk output as `wgsl-analyzer/`.
#
# Trunk.toml runs this as a pre_build hook. It does nothing once the pinned version is in place,
# and checks the npm integrity hash before unpacking. BULIN_NO_ANALYZER=1 builds offline without
# the analyzer; the app then reports it as unavailable.
set -eu

VERSION=0.12.0-next.527
INTEGRITY=sha512-Qy333feFfRIgm94YBpLBXjVyosomeG/McZpGc3zHD9EoO4GZ3NCclZjoGVDK2sqWlgdImcgySLmEARp9eJEVqw==
URL="https://registry.npmjs.org/wgsl-analyzer-web/-/wgsl-analyzer-web-$VERSION.tgz"

cd "$(dirname "$0")"
DEST=vendor/wgsl-analyzer

if [ "${BULIN_NO_ANALYZER:-}" = 1 ]; then
    # trunk's copy-dir needs the directory to exist.
    mkdir -p "$DEST"
    exit 0
fi
if [ "$(cat "$DEST/VERSION" 2>/dev/null || true)" = "$VERSION" ]; then
    exit 0
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "fetching wgsl-analyzer-web $VERSION" >&2
curl -fsSL "$URL" -o "$TMP/package.tgz"
GOT="sha512-$(openssl dgst -sha512 -binary "$TMP/package.tgz" | openssl base64 -A)"
if [ "$GOT" != "$INTEGRITY" ]; then
    echo "wgsl-analyzer-web $VERSION: integrity mismatch (got $GOT)" >&2
    exit 1
fi
tar -xzf "$TMP/package.tgz" -C "$TMP"

# index.js starts worker.js, which loads the emscripten glue and wasm next to itself. Source maps
# and type declarations aren't needed at runtime.
rm -rf "$DEST"
mkdir -p "$DEST"
cp "$TMP"/package/dist/*.js "$TMP"/package/dist/*.wasm \
    "$TMP/package/LICENSE-MIT" "$TMP/package/LICENSE-APACHE" "$DEST/"
echo "$VERSION" >"$DEST/VERSION"
