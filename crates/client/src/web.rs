//! The browser build's entry (docs/architecture/client/web.md, #29): fetch an `.iwbz` package
//! over HTTP, then run the same client the native build runs. The native build's command line is
//! untouched; here every group takes its `Default`, which is the value clap gives when the flag
//! is absent.

/// The `.iwbz` the page plays when it gives no `?package=`.
pub const DEFAULT_PACKAGE: &str = "showcase.iwbz";

/// What the page's query string asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    /// `?ui-gallery`: the UI kit's gallery, as the wasm build opened before #29.
    Gallery,
    /// `?package=<url>` or no query: play the package at this URL.
    Package(String),
}

/// Whether the page asked for the startup measurement (`?measure`, #1489).
pub fn wants_measure(search: &str) -> bool {
    search
        .trim_start_matches('?')
        .split('&')
        .any(|pair| pair == "measure")
}

/// The page a query string asks for. The gallery wins over a package, as `--ui-gallery` does
/// natively (the gallery check comes before the package is read).
pub fn page_from_search(search: &str) -> Page {
    let mut package = None;
    let mut gallery = false;
    for pair in search.trim_start_matches('?').split('&') {
        if pair == "ui-gallery" {
            gallery = true;
        } else if let Some(url) = pair.strip_prefix("package=")
            && !url.is_empty()
        {
            package = Some(url.to_owned());
        }
    }
    match package {
        Some(url) if !gallery => Page::Package(url),
        _ if gallery => Page::Gallery,
        _ => Page::Package(DEFAULT_PACKAGE.to_owned()),
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod browser {
    use std::path::PathBuf;

    use wasm_bindgen_futures::JsFuture;
    use web_sys::wasm_bindgen::{JsCast, JsValue};

    use super::{Page, page_from_search};

    /// Starts the browser client: the gallery at once, or a package after its fetch.
    pub fn start() {
        let search = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        // `?measure` logs the startup phases for `crates/client/web/measure_startup.py` (#1489).
        if super::wants_measure(&search) {
            crate::startup_profile::enable();
        }
        match page_from_search(&search) {
            Page::Gallery => {
                // Without --screenshot the gallery has no size check, so it cannot fail here.
                let _ = crate::ui::gallery::run(None, false, None, None);
            }
            Page::Package(url) => {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = play(&url).await {
                        set_status(&e);
                        web_sys::console::error_1(&JsValue::from_str(&e));
                    }
                });
            }
        }
    }

    /// Fetches the `.iwbz`, loads it into `PackageFiles`, and runs the client with its defaults.
    async fn play(url: &str) -> Result<(), String> {
        set_status(&format!("Loading {url}…"));
        let clock = crate::startup_profile::start();
        let bytes = fetch(url).await?;
        crate::startup_profile::mark("fetch", clock);
        // Unpack and parse are separate so the measurement can time them apart; together they are
        // what `package::from_iwbz_bytes` does (#1489).
        let clock = crate::startup_profile::start();
        let files = crate::package::unpack_iwbz(&bytes).map_err(|e| format!("{url}: {e}"))?;
        crate::startup_profile::mark("unpack", clock);
        let clock = crate::startup_profile::start();
        let pkg = crate::package::from_files(files, false).map_err(|e| e.to_string())?;
        crate::startup_profile::mark("parse/masks", clock);
        // The headless smoke check reads this line from the browser console; it is the hash
        // `iwbtg pkg hash` prints (docs/architecture/client/web.md, Determinism).
        let message = format!(
            "package {} loaded: gameplay hash {}",
            pkg.def.id,
            hex(&pkg.gameplay_hash)
        );
        web_sys::console::log_1(&JsValue::from_str(&message));
        // Browser storage gives the bindings, settings, save slots and stats (storage.rs).
        let clock = crate::startup_profile::start();
        crate::settings::load(false);
        let startup = crate::registration::Startup {
            isolated: false,
            screenshot: None,
            package: Some(PathBuf::from(url)),
            library: None,
            ranked: false,
            trace_mode: false,
            window: None,
            ratio: None,
        };
        // No command line: every group takes its default, then the same module list runs the
        // same startup checks and adds the same plugins as native (registration.rs).
        let mut args = crate::Args::default();
        if let Some(result) = args.prepare(&startup) {
            return result;
        }
        set_status("");
        crate::startup_profile::mark("client startup", clock);
        let first = Some((pkg, PathBuf::from(url)));
        crate::view::run_package(first, crate::view::ViewArgs::default(), move |app| {
            args.add_plugins(app, &startup)
        })
    }

    /// The bytes at `url`. A fetch failure (offline, 404, CORS) is an error with the browser's
    /// message; the archive limit is checked before the zip is unpacked.
    async fn fetch(url: &str) -> Result<Vec<u8>, String> {
        let window = web_sys::window().ok_or("the page has no window")?;
        let response = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|e| format!("{url}: {}", js_error(&e)))?
            .dyn_into::<web_sys::Response>()
            .map_err(|_| format!("{url}: fetch did not return a response"))?;
        if !response.ok() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|e| format!("{url}: {}", js_error(&e)))?,
        )
        .await
        .map_err(|e| format!("{url}: {}", js_error(&e)))?;
        let bytes = js_sys::Uint8Array::new(&buffer);
        if bytes.length() as u64 > iwbtg_format::iwbz::MAX_ARCHIVE_BYTES {
            return Err(format!(
                "{url}: over {} bytes",
                iwbtg_format::iwbz::MAX_ARCHIVE_BYTES
            ));
        }
        Ok(bytes.to_vec())
    }

    fn js_error(value: &JsValue) -> String {
        value
            .clone()
            .dyn_into::<js_sys::Error>()
            .map(|e| e.message().into())
            .unwrap_or_else(|v| format!("{v:?}"))
    }

    /// Shows a message under the canvas: the status line in `web/index.html`.
    fn set_status(message: &str) {
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("status"))
        {
            element.set_text_content(Some(message));
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use browser::start;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_picks_the_package_or_the_gallery() {
        assert_eq!(page_from_search(""), Page::Package(DEFAULT_PACKAGE.into()));
        assert_eq!(page_from_search("?x=1"), Page::Package(DEFAULT_PACKAGE.into()));
        assert_eq!(
            page_from_search("?package=rely_the_needle.iwbz"),
            Page::Package("rely_the_needle.iwbz".into())
        );
        assert_eq!(page_from_search("?ui-gallery"), Page::Gallery);
        assert_eq!(
            page_from_search("?ui-gallery&package=games/maps.iwbz"),
            Page::Gallery,
            "the gallery wins, as --ui-gallery does natively"
        );
        assert_eq!(
            page_from_search("?package="),
            Page::Package(DEFAULT_PACKAGE.into())
        );
    }

    /// `?measure` turns on the startup phases without changing which page plays (#1489).
    #[test]
    fn the_page_asks_for_the_startup_measurement() {
        assert!(wants_measure("?measure"));
        assert!(wants_measure("?package=a.iwbz&measure"));
        assert!(!wants_measure(""));
        assert!(!wants_measure("?package=measure.iwbz"), "a package named measure");
        assert!(!wants_measure("?measured"), "a different flag");
        assert_eq!(
            page_from_search("?measure"),
            Page::Package(DEFAULT_PACKAGE.into())
        );
    }
}
