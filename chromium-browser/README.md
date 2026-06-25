# Mocha Chromium — a real, usable browser (full Chromium via CEF)

This is the **usable browser** companion to the from-scratch `mocha-browser`
engine. It embeds **full Chromium** through **CEF (Chromium Embedded
Framework)** — the same engine Chrome and Edge ship — so it loads **Google,
YouTube, and any modern site**, with JavaScript, modern CSS, video, and (where
the Widevine component is present) DRM.

It is intentionally **not** from scratch. The `mocha-browser` engine in the
parent repo is the from-scratch experiment and will never be a daily driver
(see that project's `CLAUDE.md`). This directory is the pragmatic answer to
"I want a browser I can actually use": do what Brave, Edge, Arc, and Electron
do — embed an engine that already exists, rather than rebuild Chromium's ~35M
lines from nothing.

---

## ⚠️ Read this first — it cannot be built in the cloud session

This was authored in a **headless, network-restricted** Claude Code session:

- **No display** (`DISPLAY`/`WAYLAND_DISPLAY` unset) — a GUI browser can't open.
- **CEF's binary CDN is blocked** (`cef-builds.spotifycdn.com` → `403
  host_not_allowed`), and so is `static.crates.io`.

So I could **not compile or run this here** — there's no way for me to do so in
that sandbox. **You build and run it on your own machine** (which has a screen
and open network). The scripts below are written to do the whole job; if a step
fails on your setup, paste the error and we'll fix it together.

---

## What you need (one-time)

| Platform | Tools |
|----------|-------|
| **Windows** | Visual Studio 2022 (Desktop C++ workload) + CMake. `tar` is built in. |
| **macOS**   | Xcode command-line tools (`xcode-select --install`) + CMake (`brew install cmake`). |
| **Linux**   | `cmake`, a C++ toolchain (`build-essential`), and GTK/X11 dev libs. On Debian/Ubuntu: `sudo apt install cmake build-essential libgtk-3-dev libnss3 libnspr4 libxss1 libasound2`. |

CMake: <https://cmake.org/download/>.

---

## Fastest path to a working browser

```bash
cd chromium-browser

# 1. Download the official full-Chromium CEF framework for your OS (~1 GB).
./setup.sh                       # Windows: powershell -ExecutionPolicy Bypass -File .\setup.ps1

# 2. Build the bundled browsers (cefclient = full UI with URL bar + tabs).
./build.sh                       # Windows: .\build.ps1

# 3. Launch a real Chromium browser:
#   Linux:   ./cef/build/tests/cefclient/Release/cefclient
#   macOS:   open ./cef/build/tests/cefclient/Release/cefclient.app
#   Windows: .\cef\build\tests\cefclient\Release\cefclient.exe
```

`cefclient` is a fully functional Chromium browser — address bar, back/forward,
tabs, devtools. Type `https://www.youtube.com` and it plays.

> Why the CEF sample apps? `cefclient`/`cefsimple` are maintained by the CEF
> project and handle all the gnarly per-OS process model, sandbox, and window
> setup correctly. Building on them is the **highest-confidence** way to get a
> working Chromium browser — far more reliable than hand-rolling the boilerplate.

---

## Make it *yours*: "Mocha Chromium"

Turn the minimal `cefsimple` sample into your own named browser with a homepage:

```bash
./make-mocha-app.sh https://www.youtube.com   # pick any homepage
./build.sh
./cef/build/tests/mocha_chromium/Release/mocha_chromium
```

`make-mocha-app.sh` copies `cefsimple` (so it keeps CEF's known-good
boilerplate), renames the executable to `mocha_chromium`, and sets your default
homepage. From there, editing `cef/tests/mocha_chromium/*.cc` lets you add a
URL bar, bookmarks, etc. — that's where "our browser" grows.

You can also just point the minimal sample at any URL without rebuilding:

```bash
./cef/build/tests/cefsimple/Release/cefsimple --url=https://www.google.com
```

---

## YouTube / DRM note

Most of YouTube works out of the box. DRM-protected playback needs the
**Widevine CDM** component; recent CEF builds fetch it via Chromium's component
updater on first run. If a specific protected video won't play, that's the
Widevine component — tell me and I'll add the flags to enable/bundle it.

---

## Files

| File | Purpose |
|------|---------|
| `setup.sh` / `setup.ps1` | Download the latest stable CEF framework for your OS into `cef/`. |
| `build.sh` / `build.ps1` | Configure + build `cefclient`, `cefsimple` (and `mocha_chromium`). |
| `make-mocha-app.sh` | Generate the rebranded `mocha_chromium` app with your homepage. |
| `.gitignore` | Keeps the ~1 GB `cef/` download and build output out of git. |

Nothing here is wired into the parent Cargo workspace, so it does not affect the
from-scratch engine's `cargo test --all` or its no-Chromium rule.
