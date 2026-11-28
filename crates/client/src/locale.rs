//! UI translations (#196): every player-visible string comes from `crates/client/locales/`, keyed
//! by a flat name (`results_back`), with `{name}` placeholders substituted by [`format`].
//!
//! The catalogue is read once from the compiled-in files; the language comes from the settings
//! file (`settings::Settings::language`, `language = "en"` or `"ja"`), resolved on first use. The
//! language setting applies on the next start, like the display settings; a settings screen that
//! changes it live will need a reload path once the screen exists (#53). English is the fallback
//! for any key a translation leaves out.
//!
//! Module strings are English keys in the locale files; game content (titles, room names, sign
//! text, sound names) is data and is never translated here.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::settings::Language;

const EN: &str = include_str!("../locales/en.toml");
const JA: &str = include_str!("../locales/ja.toml");

struct Catalogues {
    en: BTreeMap<String, String>,
    ja: BTreeMap<String, String>,
}

fn parse(text: &str) -> BTreeMap<String, String> {
    text.parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("locale file is not valid TOML: {e}"))
        .into_iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .unwrap_or_else(|| panic!("locale value `{key}` is not a string"))
                .to_owned();
            (key, value)
        })
        .collect()
}

fn catalogues() -> &'static Catalogues {
    static CATALOGUES: OnceLock<Catalogues> = OnceLock::new();
    CATALOGUES.get_or_init(|| Catalogues {
        en: parse(EN),
        ja: parse(JA),
    })
}

/// The language this process uses: the settings file's, read on first use.
static LANGUAGE: OnceLock<Language> = OnceLock::new();

/// The language in effect. Read from the settings file the first time anything asks for a string;
/// the language setting applies on the next start until a settings screen adds a reload (#53).
pub fn language() -> Language {
    *LANGUAGE.get_or_init(|| crate::settings::get().language)
}

/// `--language`: this run uses `language` whatever the settings file says (a screenshot check
/// reads no settings). Only before the first lookup; `false` when a string was already read.
pub fn set_for_run(language: Language) -> bool {
    LANGUAGE.set(language).is_ok()
}

/// The string for `key` in the language in effect.
pub fn text(key: &str) -> &'static str {
    text_in(language(), key)
}

/// The string for `key` in `language`, falling back to English and then to the key itself.
pub fn text_in(language: Language, key: &str) -> &'static str {
    let c = catalogues();
    let table = match language {
        Language::En => &c.en,
        Language::Ja => &c.ja,
    };
    if let Some(value) = table.get(key).or_else(|| c.en.get(key)) {
        return value;
    }
    // Not in either catalogue: the tests keep the two files in step, so this is a development
    // mistake. Show the key rather than an empty label, and leak it once for the `'static`
    // return type (a single small string, not a hot path).
    Box::leak(key.to_owned().into_boxed_str())
}

/// [`text`] with `{name}` placeholders replaced by the arguments' values.
pub fn format(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    format_in(language(), key, args)
}

/// [`text_in`] with `{name}` placeholders replaced. Unknown placeholders stay as written, so a
/// mismatch is visible rather than silently dropped.
pub fn format_in(language: Language, key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text_in(language, key).to_owned();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(language: Language) -> Vec<&'static str> {
        let c = catalogues();
        let table = match language {
            Language::En => &c.en,
            Language::Ja => &c.ja,
        };
        table.keys().map(String::as_str).collect()
    }

    fn placeholders(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else { break };
            out.push(&after[..close]);
            rest = &after[close + 1..];
        }
        out.sort_unstable();
        out
    }

    #[test]
    fn english_and_japanese_have_the_same_keys() {
        assert_eq!(keys(Language::En), keys(Language::Ja));
        assert!(keys(Language::En).len() > 40, "the catalogue is populated");
        for key in keys(Language::En) {
            assert!(!text_in(Language::En, key).is_empty(), "en {key}");
            assert!(!text_in(Language::Ja, key).is_empty(), "ja {key}");
        }
    }

    #[test]
    fn placeholders_match_between_languages() {
        for key in keys(Language::En) {
            assert_eq!(
                placeholders(text_in(Language::En, key)),
                placeholders(text_in(Language::Ja, key)),
                "{key}"
            );
        }
    }

    #[test]
    fn japanese_is_translated_not_a_copy() {
        assert_ne!(
            text_in(Language::En, "results_personal_best"),
            text_in(Language::Ja, "results_personal_best")
        );
        assert!(text_in(Language::Ja, "results_personal_best").contains('ベ'));
        // The month names and the date order really differ.
        assert_eq!(text_in(Language::Ja, "month_11"), "11月");
        assert_eq!(
            format_in(
                Language::Ja,
                "date_long",
                &[("d", &3), ("m", &"11月"), ("y", &2026)]
            ),
            "2026年11月3日"
        );
    }

    #[test]
    fn format_replaces_named_placeholders() {
        assert_eq!(
            format_in(Language::En, "results_stage_number", &[("n", &6)]),
            "Stage 6"
        );
        assert_eq!(
            format_in(
                Language::En,
                "results_cleared",
                &[("date", &"3 Nov 2026"), ("hour", &21), ("min", &"14")]
            ),
            "Cleared 3 Nov 2026, 21:14"
        );
        assert_eq!(
            format_in(Language::Ja, "results_hold_key", &[("key", &"R")]),
            "R を押し続ける"
        );
    }

    #[test]
    fn a_missing_key_falls_back_to_english_then_the_key() {
        assert_eq!(text_in(Language::Ja, "no_such_key"), "no_such_key");
        // Every real key resolves in both languages (checked above); this pins the fallback path.
        assert_eq!(text_in(Language::En, "results_back"), "Back");
    }
}
