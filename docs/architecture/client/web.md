# The client: In a browser

Part of [the client](../client.md). Decided by [#29](https://github.com/nugamus/iwbtg-collection/issues/29).

The same client runs in a browser (wasm32-unknown-unknown) from `crates/client/web/`, a page plus what `wasm-bindgen` writes beside it. `web.rs` is the only entry: it reads the page's query string, fetches one [`.iwbz` package](../../format/iwbz.md) over HTTP, and hands the files to the same code the native build runs. There is no command line; every module's clap group takes its `Default`, which is the value clap gives when the flag is absent, and then the [module list](main.md) runs the same startup checks and adds the same plugins as native. The native build and its flags do not change.

## The page

`web/index.html` reads `location.search`:

- `?package=<url>` plays the `.iwbz` at that URL. A relative URL is served from the same folder as the page; any URL works when its server sends CORS headers.
- No query plays `showcase.iwbz` from the page's folder.
- `?ui-gallery` opens the [UI](ui.md) gallery, as the browser build did before the game path. It wins over `?package=`, as `--ui-gallery` does natively (the gallery runs before a package is read).

The page shows the load in its `#status` line and clears it when the game starts. A fetch or unpack failure is printed to the console and left in that line; the page keeps running.

The page loads the audio-unlock script before `init()` ([audio](audio.md), Web audio unlock): a browser keeps audio suspended until a key, click or touch, and the client holds its music until the page says a context runs. A controller press does not count. Keep that script when changing `index.html`.

## Loading a package

The fetch uses the browser's `fetch` (`web_sys`); the bytes go into `iwbtg_format::iwbz::unpack` and so into [`PackageFiles`], the same type the native folder read produces. `web.rs` checks the [archive limit](../../format/iwbz.md#limits) before unpacking and the zip reader checks every entry. An `.iwbz` already carries its engine pack, so the browser does not need the games repo's `_engines/` folder; a package folder cannot be fetched file by file because HTTP does not list a folder.

The native filesystem code in `package.rs` stays for the native build. The shared part is `package::from_iwbz_bytes` (bytes to `Package`) and `package::from_files` (files to game definition and hash), which `package::open` also uses for the `.iwbz` branch.

The browser has no OS clock: `std::time::Instant::now` and `SystemTime::now` panic on wasm32-unknown-unknown, as does `thread::sleep`. The client reads the clock through `time.rs`, which is `std::time` natively and `web-time` (`performance.now`, `Date.now`) in the browser, and the frame cap does not sleep there: the browser paces the window with animation frames and the sim's fixed clock does not depend on the frame rate. The import plugin's library copy of an opened `.iwbz` is native-only, since a fetched package has no file on disk to copy.

## Determinism

The package's `gameplay_hash` comes from the same `iwbtg_format::load_report` as native, over the same bytes: `iwbtg pkg pack` writes the `.iwbz` without changing file bytes, so packing a folder and loading it back keeps the hash (`package.rs`'s `an_iwbz_opens_like_its_folder_and_a_bad_one_is_refused`). After loading, the browser logs one line to the console:

```
package <id> loaded: gameplay hash <hex>
```

and native `iwbtg pkg hash <package>` prints the same hash. The check for #29 read that console line from a headless browser and compared it with the native value. Beyond the hash, the sim, its inputs and the replay writer are the same code the `wasm32-unknown-unknown` CI job already builds and tests under node; the browser adds the fetch and the page.

## Building and running

See [dev setup](../../dev-setup.md), The client in a browser. The page needs WebGL2; `localhost` and `127.0.0.1` are secure contexts for the Gamepad API ([view](view.md), controller).

## Limits

- There is no game select in the browser yet: the page plays the one package it is given ([shell](shell.md); the select screen is native-only until [#148](https://github.com/nugamus/iwbtg-collection/issues/148)).
- `--select`, `--bind`, `--show-bindings` and the screenshot flags are native-only; the browser has no command line. Bindings, settings, save slots and stats still come from browser storage ([storage](storage.md)).
- Drag-and-drop of a package ([import](import.md)) does not start a run in the browser; the page loads one URL.
- CI lints the client for `wasm32-unknown-unknown` but does not build the page's bundle or run a browser; that is [#1078](https://github.com/nugamus/iwbtg-collection/issues/1078).
