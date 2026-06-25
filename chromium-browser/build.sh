#!/usr/bin/env bash
#
# Build the CEF sample browsers (cefclient, cefsimple) and, if you generated it,
# our rebranded "mocha_chromium", using the CMake that ships in the CEF
# distribution downloaded by ./setup.sh.
#
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
CEF="$DIR/cef"
BUILD="$CEF/build"

[ -f "$CEF/CMakeLists.txt" ] || { echo "CEF not found. Run ./setup.sh first." >&2; exit 1; }
command -v cmake >/dev/null || { echo "cmake is required (install it first)." >&2; exit 1; }

JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"
echo "Configuring (Release) ..."
cmake -S "$CEF" -B "$BUILD" -DCMAKE_BUILD_TYPE=Release
echo "Building with $JOBS jobs ..."
cmake --build "$BUILD" --config Release -j"$JOBS"

echo
echo "Built. Browser binaries:"
find "$BUILD/tests" -maxdepth 3 \( -name cefclient -o -name cefsimple -o -name mocha_chromium -o -name '*.app' \) 2>/dev/null || true
echo
echo "Run a full browser (URL bar + tabs):"
echo "  Linux:  $BUILD/tests/cefclient/Release/cefclient"
echo "  macOS:  open $BUILD/tests/cefclient/Release/cefclient.app"
