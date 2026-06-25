#!/usr/bin/env bash
#
# Download the official CEF (Chromium Embedded Framework) binary distribution
# for this machine into ./cef, so the sample browsers (cefclient / cefsimple)
# and our "Mocha Chromium" rebrand can be built against it.
#
# This is the real, full Chromium engine — the same one Chrome ships. We are
# NOT building Chromium from source (that needs ~100 GB and hours); we download
# the prebuilt framework the CEF project publishes, which is how every CEF app
# is made.
#
# Usage:  ./setup.sh            # latest stable for your OS/arch
#         CEF_CHANNEL=beta ./setup.sh
#
set -euo pipefail

INDEX_URL="https://cef-builds.spotifycdn.com/index.json"
BASE_URL="https://cef-builds.spotifycdn.com"
CHANNEL="${CEF_CHANNEL:-stable}"
DEST="$(cd "$(dirname "$0")" && pwd)/cef"

# --- Map this machine to a CEF platform key ----------------------------------
uname_s="$(uname -s)"
uname_m="$(uname -m)"
case "$uname_s" in
  Linux)
    case "$uname_m" in
      x86_64)        PLATFORM="linux64" ;;
      aarch64|arm64) PLATFORM="linuxarm64" ;;
      *) echo "Unsupported Linux arch: $uname_m" >&2; exit 1 ;;
    esac ;;
  Darwin)
    case "$uname_m" in
      arm64)  PLATFORM="macosarm64" ;;
      x86_64) PLATFORM="macosx64" ;;
      *) echo "Unsupported macOS arch: $uname_m" >&2; exit 1 ;;
    esac ;;
  *) echo "Unsupported OS: $uname_s (use setup.ps1 on Windows)" >&2; exit 1 ;;
esac

command -v python3 >/dev/null || { echo "python3 is required to parse the CEF index" >&2; exit 1; }
command -v curl    >/dev/null || { echo "curl is required" >&2; exit 1; }

echo "Platform: $PLATFORM   channel: $CHANNEL"
echo "Fetching CEF build index ..."

# Pick the newest build on the requested channel and its "standard" archive
# (standard includes the cefclient/cefsimple SOURCE + CMake so we can build).
read -r FILE_NAME CEF_VERSION < <(curl -fsSL "$INDEX_URL" | python3 - "$PLATFORM" "$CHANNEL" <<'PY'
import json, sys
data = json.load(sys.stdin)
platform, channel = sys.argv[1], sys.argv[2]
versions = data.get(platform, {}).get("versions", [])
for v in versions:                      # newest first in the index
    if v.get("channel") != channel:
        continue
    for f in v.get("files", []):
        if f.get("type") == "standard":
            print(f["name"], v["cef_version"])
            sys.exit(0)
sys.exit("No 'standard' build found for %s/%s" % (platform, channel))
PY
)

echo "Selected: $FILE_NAME  (CEF $CEF_VERSION)"

# The '+' characters in the filename must be percent-encoded in the URL.
ENCODED_NAME="${FILE_NAME//+/%2B}"
ARCHIVE="/tmp/${FILE_NAME}"

if [ ! -f "$ARCHIVE" ]; then
  echo "Downloading (~1 GB, this takes a while) ..."
  curl -fL --progress-bar "$BASE_URL/$ENCODED_NAME" -o "$ARCHIVE"
fi

echo "Extracting into $DEST ..."
rm -rf "$DEST"
mkdir -p "$DEST"
# Strip the top-level cef_binary_* directory so $DEST holds CMakeLists.txt etc.
tar -xjf "$ARCHIVE" -C "$DEST" --strip-components=1

echo
echo "Done. CEF $CEF_VERSION is in $DEST"
echo "Next:  ./build.sh        (builds cefclient, cefsimple, and mocha_chromium)"
