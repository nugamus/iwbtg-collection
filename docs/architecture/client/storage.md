# The client: Storage

Part of [the client](../client.md).

Storage (`storage.rs`, #473): the one place the client keeps things between sessions, so no other module uses `cfg` for it. `Store::config()` (the settings file, with the bindings in its `[bindings]` table) and `Store::data()` (save slots, `saves/<game id>.save`, the local stats, `stats.toml`, and replays, `replays/...`) return a store, or `None` when there is nowhere to keep things; a store reads, writes and removes bytes or text by a `/`-separated name, and every failure is an error string the caller turns into `storage::warn` while the game keeps running. Natively a store is a folder (config: as in Settings; data: `iwbtg-collection` in `%LOCALAPPDATA%` on Windows, else `$XDG_DATA_HOME` or `~/.local/share`), and a write goes to `<name>.tmp` beside the file and is renamed over it, so a crash never leaves half a file; warnings go to stderr. On the web (`wasm32-unknown-unknown`) a store is the browser's `localStorage`, one entry per name under the key `iwbtg-collection/config/<name>` or `iwbtg-collection/data/<name>`; text is stored as is (the settings TOML stays readable in the browser's dev tools) and bytes as lowercase hex (a save slot is a few dozen bytes, far below the quota). Private browsing or a blocked site (no `localStorage`) is one console warning when the store opens, and the session then runs without keeping anything; a full quota on a write is a console warning and the next change tries again. The web build itself, and so a reload test in a browser, comes with #29.

Where things are kept (#107; `backend::root` picks the folder and the `native_folders_per_os` test pins it). An empty variable counts as unset; with neither the variables nor `HOME` there is no store and nothing is kept.

| What | Name in the store | Windows | Linux, macOS | Web |
|---|---|---|---|---|
| Settings and bindings | `settings.toml` (config) | `%APPDATA%\iwbtg-collection\` | `$XDG_CONFIG_HOME/iwbtg-collection/`, else `~/.config/iwbtg-collection/` | `localStorage["iwbtg-collection/config/settings.toml"]` |
| Games (library) | `library/<id>.iwbz` (data) | `%LOCALAPPDATA%\iwbtg-collection\` | `$XDG_DATA_HOME/iwbtg-collection/`, else `~/.local/share/iwbtg-collection/` | `localStorage["iwbtg-collection/data/library/<id>.iwbz"]` |
| Save slots | `saves/<game id>.save` (data) | as above | as above | `iwbtg-collection/data/saves/...` |
| Replays | `replays/...` (data) | as above | as above | `iwbtg-collection/data/replays/...` |
| Local stats | `stats.toml` (data) | as above | as above | `iwbtg-collection/data/stats.toml` |

`$IWBTG_CONFIG_DIR` replaces the whole config folder (no `iwbtg-collection` is added); tests and checks use it. macOS uses the Linux folders rather than `~/Library/Application Support`, so one rule covers both. Settings survive a restart because every change is written through the config store at once (`settings::update`; `settings::tests::file_write_and_read` reads back what it wrote).

A player never sees stderr or the browser console, so `storage::warn` also posts a notice (`ui::notice`, [UI](ui.md), #566): the first storage warning of a session shows a short card in the bottom left, "Not saved: Saves and settings may not be kept this session.", for 6 s; later ones only go to the console. The card does not show the message itself (file names and OS errors), which stays in the console line. On the web the private-browsing warning at store open goes through `warn` too, so it shows the card.
