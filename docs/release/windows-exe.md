# Windows `.exe` release packaging

Mocha Browser ships as a plain folder containing a double-clickable
`Mocha.exe` — no installer, no auto-update, no code signing. The packaged
binary is the `mocha_desktop` GUI build renamed; the crate itself is not
renamed.

## How to build

```powershell
cargo build --release -p mocha_desktop --features gui
```

Requirements: the pinned stable Rust toolchain (`rust-toolchain.toml`) and a C
compiler for the bundled SQLite and `ring` builds (MinGW-w64 `gcc` on the
default `x86_64-pc-windows-gnu` toolchain).

## How to package

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

This produces:

```text
dist/MochaBrowser/
  Mocha.exe     # the desktop browser
  README.txt    # copy of README.md
  LICENSE
  profile/      # fallback profile dir (see "Profile and logs" below)
  logs/         # placeholder; Mocha does not write log files yet
```

## How to run

```powershell
.\dist\MochaBrowser\Mocha.exe
```

With no argument, Mocha opens its internal home/new-tab page (no network
needed). To open a page directly:

```powershell
.\dist\MochaBrowser\Mocha.exe examples\basic\index.html
.\dist\MochaBrowser\Mocha.exe https://example.com/
```

If the argument fails to load — unreachable host, unsupported HTML such as
`<head>`, a non-HTML content type, a bad path — the window still opens and
shows an internal error page with the failure message; it does not panic and
does not silently render something wrong.

## Profile and logs

- `--profile <dir>` uses that directory, as before.
- Otherwise the app uses `%APPDATA%\MochaBrowser\profile` (created on first
  GUI launch, together with `%APPDATA%\MochaBrowser\logs`).
- If `APPDATA` is unset, the fallback is `.\profile` / `.\logs` relative to
  the working directory (this is why the dist folder ships empty `profile/`
  and `logs/` directories).
- The GUI initializes the profile store (SQLite) at startup; if that fails
  (e.g. a read-only location), Mocha prints a warning and keeps running
  without persistence. Mocha writes nothing into the repository unless you
  pass `--profile` pointing there.
- Nothing writes to `logs/` yet; it exists so future logging has a stable,
  documented home.

## Runtime dependencies

The release executable is audited with `objdump -p` after each packaging
change. It imports only Windows system DLLs — ADVAPI32, GDI32, KERNEL32,
USER32, USERENV, WS2_32, bcrypt, bcryptprimitives, ntdll, and the
api-ms-win-core/crt-* UCRT forwarders — all present on a stock Windows 10
install. No MinGW runtime DLLs (`libgcc_s_seh-1.dll`, `libwinpthread-1.dll`,
`libstdc++-6.dll`) are required, so the folder is self-contained on the build
host's architecture (x86-64).

This has **not** yet been verified on a clean Windows machine without the Rust
toolchain installed; treat portability as "expected, unverified". For a
broadly distributed release later, prefer `x86_64-pc-windows-msvc` or a
carefully verified windows-gnu package.

## Known limitations

- Mocha is experimental. Many real websites will not render fully.
- HTTPS works at the network layer (rustls, Mozilla roots), but HTML/CSS/JS
  compatibility is limited; e.g. `https://example.com/` currently stops at the
  unsupported `<head>` tag and shows an error page.
- Navigation errors *inside* the window (address bar / link clicks to pages
  that fail) print to the console rather than showing an in-window error page;
  only the initial document shows the internal error page today.
- No installer yet (MSI / NSIS / Inno Setup deferred).
- No auto-update.
- No code signing — Windows SmartScreen may warn because the exe is unsigned.
- No file associations or default-browser registration.
- Windows icon/installer deferred (no icon or version-info resource is
  embedded; that needs a resource-compiler build step we deliberately avoid
  for now).
