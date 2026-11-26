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

## Measuring startup

`?measure` turns on startup phase timing (`startup_profile.rs`): the page logs one
`startup phase <name>: <ms> ms` line per phase and a final `startup total sprites ready: <ms> ms`,
which the client does not compute otherwise. `crates/client/web/measure_startup.py` serves the
page, loads it in headless Chrome and prints those lines:

```
python -I crates/client/web/measure_startup.py --package showcase.iwbz
python -I crates/client/web/measure_startup.py --synthetic 400 --size 256
```

The first needs the bundle and `showcase.iwbz` from "Building and running". The second packs its
own package of 400 solid 256×256 sprites (100 MiB RGBA), the content
`shell::tests::startup_phases` uses (#1023), with the native tools, and removes the archive
afterwards. Each phase is the work between two clock reads:

- **fetch** — the HTTP response and its array buffer. The measurement begins in `start()`, so the
  wasm download and instantiation are not in it.
- **unpack** — `iwbtg_format::iwbz::unpack` and the archive-limit check (`package::unpack_iwbz`).
- **parse/masks** — `package::from_files`: `load_report` (game.toml, masks, the gameplay hash) and
  the sound pass.
- **client startup** — `settings::load` and `Args::prepare`.
- **sprite decode** — `package::sprite_images` (every strip's PNG to RGBA).
- **script prepare** — `Sim::prepare`.
- **first room** — `Sim::new_or_halted_prepared` and the start room's Create.
- **bevy init** — `DefaultPlugins` and every module's plugin build, including the window and the
  renderer.
- **sprite build (cpu)** and **sprite queue (wall)** — `view::upload_textures`: image
  construction and insertion into `Assets<Image>` (CPU), and the frames it spans until
  `SpriteQueue` empties (wall). The GPU transfer is not a separate phase: the renderer uploads
  later and no browser-side boundary isolates it. The wall time is frame-bound, not CPU-bound.
- **startup total sprites ready** — from `start()` to the queue emptying, i.e. including the
  loading card's wait.

The clock is `performance.now`, whose 100 µs resolution makes a phase under that read
`0.000 ms`; `script prepare` usually does.

**Measured 2026-11-25** on Windows 11 Pro 10.0.26300, Intel Arc 130T, Google Chrome 158.0.8127.37
(`--headless=new` with SwiftShader WebGL2), from
`cargo build -p iwbtg-client --release --target wasm32-unknown-unknown` and wasm-bindgen 0.2.129,
without `wasm-opt` locally; three runs each:

| phase | showcase (15 sprites, 0.2 MiB) | synthetic (400 × 256, 100 MiB RGBA) |
| --- | --- | --- |
| fetch | 8–30 ms | 9–10 ms |
| unpack | 3–4 ms | 9–11 ms |
| parse/masks | 39–48 ms | 106–121 ms |
| client startup | 2.0–2.6 ms | 2.1–4.0 ms |
| sprite decode | 2.2–2.8 ms | 81–92 ms |
| script prepare | ~0.1 ms | ≤0.1 ms |
| first room | 3.2–3.7 ms | 2.5–3.2 ms |
| Bevy init | 193–289 ms | 224–242 ms |
| sprite build (CPU) | ≤0.1 ms, 1 frame | ~0.2 ms, 13 frames |
| sprite queue (wall) | 44–51 ms | 4.1–4.2 s |
| total | 448–526 ms | 4.66–4.80 s |

Compared with the native numbers in [shell](shell.md) (Startup latency, #1023), parse, decode and
the first room are the same order. The native measurement is a debug test that reads a folder;
the browser fetches one archive, so `open` is not like-for-like. The browser's dominant costs are
Bevy/window/renderer init and the software-rendered frames: the 8 MiB/frame upload is 13 frames
and ~0.2 ms of CPU, so its 4 s under SwiftShader is the renderer's frame rate, not the upload.

## Limits

- There is no game select in the browser yet: the page plays the one package it is given ([shell](shell.md); the select screen is native-only until [#148](https://github.com/nugamus/iwbtg-collection/issues/148)).
- `--select`, `--bind`, `--show-bindings` and the screenshot flags are native-only; the browser has no command line. Bindings, settings, save slots and stats still come from browser storage ([storage](storage.md)).
- Drag-and-drop of a package ([import](import.md)) does not start a run in the browser; the page loads one URL.
- Music drift correction is native-only: a browser tune starts at the tick-clock position but is not seeked back to the beat, because rodio's seek waits for an audio callback that cpal runs on the same main thread ([audio](audio.md), music sync; [#1127](https://github.com/nugamus/iwbtg-collection/issues/1127)).
- CI builds the page bundle, fails over the compressed-size budget, keeps the bundle as the `web-bundle` artifact for 7 days and runs a headless-browser hash smoke test (the `web` job in `.github/workflows/ci.yml`; [dev setup](../../dev-setup.md), CI).
