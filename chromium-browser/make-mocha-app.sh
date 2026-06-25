#!/usr/bin/env bash
#
# OPTIONAL: turn CEF's `cefsimple` sample into *our* browser, "Mocha Chromium".
#
# This copies the known-good cefsimple app (which already solves all the tricky
# per-OS CEF process/window boilerplate), renames the build target, and sets a
# default homepage. We deliberately reuse CEF's maintained boilerplate instead
# of re-authoring it, so the result actually compiles.
#
# Usage:   ./make-mocha-app.sh [homepage-url]
#          ./make-mocha-app.sh https://www.youtube.com
# Then re-run ./build.sh and launch tests/mocha_chromium/Release/mocha_chromium.
#
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
CEF="$DIR/cef"
SRC="$CEF/tests/cefsimple"
DST="$CEF/tests/mocha_chromium"
HOME_URL="${1:-https://www.google.com}"

[ -d "$SRC" ] || { echo "Need $SRC. Run ./setup.sh first." >&2; exit 1; }

echo "Creating $DST (homepage: $HOME_URL) ..."
rm -rf "$DST"
cp -r "$SRC" "$DST"

# Rename the CMake target/executable from cefsimple -> mocha_chromium.
sed -i.bak "s/cefsimple/mocha_chromium/g" "$DST/CMakeLists.txt"
rm -f "$DST/CMakeLists.txt.bak"

# Point the default homepage (cefsimple falls back to google when no --url given)
# at our choice. Handles both http and https spellings across CEF versions.
for f in "$DST"/*.cc; do
  sed -i.bak \
    -e "s#http://www\.google\.com#${HOME_URL}#g" \
    -e "s#https://www\.google\.com#${HOME_URL}#g" \
    "$f"
  rm -f "$f.bak"
done

# Register the new app with the distribution's top-level build (idempotent).
if ! grep -q "tests/mocha_chromium" "$CEF/CMakeLists.txt"; then
  printf '\nadd_subdirectory(tests/mocha_chromium)\n' >> "$CEF/CMakeLists.txt"
fi

echo "Done. Now run ./build.sh and launch:"
echo "  $CEF/build/tests/mocha_chromium/Release/mocha_chromium"
