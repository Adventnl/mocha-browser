# Profile Storage (Milestone 14)

Milestone 14 adds a **persistent browser profile foundation**: the `mocha_storage`
crate, an embedded-SQLite database holding visit history, bookmarks, settings,
download metadata, and a persisted session snapshot, plus a **private** (in-memory)
profile mode. This is **not** a full browser profile — there is no encryption, no
sync, no password manager, and no concurrency model beyond a single connection.
Milestone 15 added cookies and origin-keyed `localStorage` tables on top of this
profile foundation.

## Dependency: rusqlite (bundled)

`mocha_storage` depends on `rusqlite` with the `bundled` feature, which compiles
SQLite from source. Justification:

- A real, transactional, indexed local datastore with a schema and migrations —
  exactly what a browser profile needs — without hand-rolling a file format.
- `bundled` means **no system library and no network** at build or run time.
- **No ORM, no async runtime, no remote/network database.** SQL is written by
  hand; timestamps are caller-supplied (`now_ms: i64`) so the stores stay
  deterministic and testable (the crate never reads a clock).

This is the workspace's second third-party dependency (after `image`); the
optional desktop window dependency (`minifb`) is the third.

## Profiles

```rust
pub enum ProfileMode {
    Persistent { path: PathBuf }, // a profile directory on disk
    Private,                      // an in-memory database, never written to disk
}
```

- `Profile::persistent(path)` creates the profile directory if missing (erroring
  if the path exists but is a file), opens `<dir>/mocha.db`, and migrates it.
- `Profile::private()` opens an `:memory:` database. Nothing is written to disk;
  a fresh private profile is always empty.
- One `Connection` is shared by all stores (required for private mode, where each
  separate connection would be a different in-memory database). `rusqlite`'s
  methods take `&self`, so the stores borrow the connection.

## Schema and migrations

The schema version lives in a one-row `schema_version` table. Opening a database
runs every migration above the stored version, in order; migrations use
`CREATE TABLE IF NOT EXISTS` and are idempotent, so reopening an up-to-date
database is a no-op. **Migration 1** creates the Milestone 14 tables:

| Table | Purpose |
| --- | --- |
| `history` | `url`, `title`, `visit_count`, `last_visited_ms` |
| `bookmarks` | `url`, `title`, `created_ms`, `updated_ms` |
| `settings` | `key` → `value` (text; booleans as `"true"`/`"false"`) |
| `downloads` | `url`, `target_path`, `started_ms`, `finished_ms`, `status` |
| `session_tabs` | one row per tab: `position`, `url`, `title`, `scroll_y`, `history_json`, `current_history_index` |
| `session_meta` | `key` → `value` (the active-tab index) |

(Milestone 15 added `cookies` and `local_storage` tables via migration 2 — see
[cookies-and-web-storage.md](cookies-and-web-storage.md).)

## Stores

Each store is a thin borrowed view over the connection:

- **`HistoryStore`** — `record_visit` (first visit inserts `visit_count = 1`;
  repeats increment the count and update `last_visited_ms`), `recent_visits`
  (newest first), `find_by_url`, `clear_history`.
- **`BookmarkStore`** — `add_bookmark` (returns the row with its id),
  `remove_bookmark` (→ `bool`), `list_bookmarks`, `update_title`.
- **`SettingsStore`** — `set_string`/`get_string`, `set_bool`/`get_bool`;
  a missing key reads as `None`.
- **`DownloadStore`** — metadata only (no downloader): `insert_download`,
  `mark_complete`/`mark_failed`/`mark_canceled`, `list_downloads`,
  `clear_downloads`.
- **`SessionStore`** — `save_session`/`load_session`/`clear_session` over
  `StoredSession`/`StoredTab` DTOs. Per-tab history is stored as a newline-joined
  string in `session_tabs.history_json` (Mocha's normalized URLs never contain
  newlines), avoiding a JSON dependency.

## Session persistence and crate boundaries

`mocha_storage` defines `StoredSession`/`StoredTab` DTOs of the **same shape** as
the desktop shell's in-memory `SessionSnapshot`/`SessionTab` (Milestone 13) so
`mocha_storage` never depends on `mocha_desktop`. The desktop crate provides the
`From` conversions (`SessionSnapshot` ↔ `StoredSession`). Dependency direction is
one-way: `mocha_desktop` → `mocha_storage`.

## Desktop integration (minimal)

A headless command demonstrates the integration end-to-end:

```bash
cargo run -p mocha_desktop -- --profile ./profile --dump-session examples/basic/index.html
```

It opens a persistent profile, loads the target into a one-tab session, records
the visit in `history`, saves the session snapshot, then reads it back from the
database and prints it. The interactive window does not yet surface history,
bookmarks, or auto-restore — that UI wiring is deferred.

## Private mode and "what is not stored"

- A **private** profile keeps everything in memory and writes **no files**; a new
  private profile is always empty. The non-persistence is covered by tests for
  every store.
- Cookies and origin-keyed `localStorage` are stored as of Milestone 15 (see
  [cookies-and-web-storage.md](cookies-and-web-storage.md)). Still not stored:
  passwords/credentials, form autofill, full page content, the DOM/layout/display
  list, and favicons.

## Limitations

- Not a full or secure browser profile: no encryption, no integrity protection,
  no multi-process access. Cookies and origin-keyed `localStorage` are stored as
  of Milestone 15.
- Single SQLite connection; not designed for concurrent writers.
- The desktop UI integration is intentionally minimal (one headless command);
  the interactive shell does not yet record history or restore sessions.
- `restore_session`/`homepage`/window-size settings exist as keys but are not yet
  consumed by the shell.
- JS `localStorage` is not yet wired to `LocalStorageStore`, and default page
  loads are not yet wired through the persistent cookie store.
