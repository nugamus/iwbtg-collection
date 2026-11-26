//! String functions in the script API (#1532; docs/architecture/sim/scripts.md, census group
//! "strings").
//!
//! GML names and semantics: indices are 1-based, and `string()` of a real follows the GM8
//! formatting claim (NUM-053/NUM-054, register N6): an integer gets no decimals, anything else
//! exactly two, rounding the exact binary value to two decimals with ties to even. Rust's
//! `{:.2}` is that rounding on every recorded value.
//!
//! A "character" here is a Unicode scalar value (Rhai strings are UTF-8); GM8 counted ANSI
//! bytes, so the two agree on ASCII, which is what the census scripts build.

use rhai::{Dynamic, Engine, ImmutableString};

use super::api::runtime_error;
use super::docs::Docs;
use super::inst::num;
use crate::num::Real;

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// A string argument.
fn text(d: &Dynamic, what: &str) -> Res<String> {
    d.clone()
        .try_cast::<ImmutableString>()
        .map(|s| s.to_string())
        .ok_or_else(|| runtime_error(format!("`{what}` must be a string")))
}

/// A whole-number argument (may be negative: the index functions clamp it).
fn whole(d: &Dynamic, what: &str) -> Res<i64> {
    let f = num(d, what)?;
    let r = Real::new(f);
    // gm-compare: ok (a GML index is exactly whole or not)
    if !r.is_finite() || crate::num::round::frac(r).get() != 0.0 {
        return Err(runtime_error(format!("`{what}` must be a whole number")));
    }
    if f < i64::MIN as f64 || f > i64::MAX as f64 {
        return Err(runtime_error(format!("`{what}` is out of range")));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "checked to be a whole number inside the i64 range"
    )]
    Ok(f as i64)
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// GM8 `string()` of a real (NUM-053/NUM-054): no decimals for an integer, else exactly two,
/// rounding the exact binary value with ties to even (Rust's `{:.2}`).
fn real_to_text(x: f64) -> Res<String> {
    if !x.is_finite() {
        return Err(runtime_error("string() of a non-finite number"));
    }
    // gm-compare: ok (zero is an integer; `-0.0` prints as `0`)
    if x == 0.0 {
        return Ok("0".to_owned());
    }
    // gm-compare: ok (an integer gets no decimals, NUM-053)
    if crate::num::round::frac(Real::new(x)).get() == 0.0 {
        return Ok(format!("{x:.0}"));
    }
    Ok(format!("{x:.2}"))
}

fn gm_string(d: &Dynamic) -> Res<String> {
    if let Some(s) = d.clone().try_cast::<ImmutableString>() {
        return Ok(s.to_string());
    }
    if let Ok(b) = d.as_bool() {
        return Ok(if b { "1" } else { "0" }.to_owned());
    }
    if let Ok(f) = num(d, "string") {
        return real_to_text(f);
    }
    Err(runtime_error("string() takes a number, a bool or a string"))
}

/// The most characters `string_format` may pad to, so one call cannot build an enormous result.
const MAX_FORMAT_WIDTH: i64 = 1024;

fn string_format(x: f64, total: i64, dec: i64) -> Res<String> {
    if !Real::new(x).is_finite() {
        return Err(runtime_error("string_format() of a non-finite number"));
    }
    if !(0..=MAX_FORMAT_WIDTH).contains(&total) {
        return Err(runtime_error(format!(
            "string_format width {total} is not from 0 to {MAX_FORMAT_WIDTH}"
        )));
    }
    if !(0..=64).contains(&dec) {
        return Err(runtime_error(format!(
            "string_format decimals {dec} is not from 0 to 64"
        )));
    }
    let width = usize::try_from(total).expect("checked to 0..=1024 just above");
    let prec = usize::try_from(dec).expect("checked to 0..=64 just above");
    Ok(format!("{x:>width$.prec$}", width = width, prec = prec))
}

/// Registers the string API on `engine` (called from `register_instance_api`; the functions are
/// pure free functions over their arguments).
#[allow(clippy::too_many_lines, reason = "one registration per script-visible name")]
pub fn register_string_api(engine: &mut Engine, docs: &mut Docs) {
    docs.add(
        "string",
        "`string(x)`: a real as GM8 writes it (an integer with no decimals, anything else with exactly two, rounded ties-to-even), a bool as 1/0, or a string unchanged.",
    );
    engine.register_fn("string", |x: Dynamic| -> Res<ImmutableString> {
        gm_string(&x).map(ImmutableString::from)
    });
    docs.add(
        "string_format",
        "`string_format(x, total, decimals)`: `x` padded on the left to `total` characters with `decimals` places (0 to 64, total 0 to 1024); a number wider than `total` is not cut.",
    );
    engine.register_fn(
        "string_format",
        |x: Dynamic, total: Dynamic, dec: Dynamic| -> Res<ImmutableString> {
            let x = num(&x, "string_format")?;
            let total = whole(&total, "string_format total")?;
            let dec = whole(&dec, "string_format decimals")?;
            string_format(x, total, dec).map(ImmutableString::from)
        },
    );
    docs.add(
        "string_length",
        "`string_length(s)`: the number of characters (an INT as a FLOAT).",
    );
    engine.register_fn("string_length", |s: Dynamic| -> Res<f64> {
        Ok(text(&s, "string_length")?.chars().count() as f64)
    });
    docs.add(
        "string_char_at",
        "`string_char_at(s, index)`: the character at 1-based `index`, or \"\" outside the string.",
    );
    engine.register_fn(
        "string_char_at",
        |s: Dynamic, index: Dynamic| -> Res<ImmutableString> {
            let chars = chars(&text(&s, "string_char_at")?);
            let i = whole(&index, "string_char_at index")?;
            let c = usize::try_from(i - 1).ok().and_then(|i| chars.get(i)).copied();
            Ok(c.map_or_else(ImmutableString::new, |c| ImmutableString::from(c.to_string())))
        },
    );
    docs.add(
        "string_pos",
        "`string_pos(substr, s)`: the 1-based character position of the first `substr` in `s`, or 0 when it is not there.",
    );
    engine.register_fn("string_pos", |substr: Dynamic, s: Dynamic| -> Res<f64> {
        let (substr, s) = (text(&substr, "string_pos substr")?, text(&s, "string_pos")?);
        Ok(s.find(&substr)
            .map_or(0.0, |byte| (s[..byte].chars().count() + 1) as f64))
    });
    docs.add(
        "string_copy",
        "`string_copy(s, index, count)`: up to `count` characters of `s` from 1-based `index` (a shorter tail when it runs past the end).",
    );
    engine.register_fn(
        "string_copy",
        |s: Dynamic, index: Dynamic, count: Dynamic| -> Res<ImmutableString> {
            let chars = chars(&text(&s, "string_copy")?);
            let start = whole(&index, "string_copy index")?.max(1) - 1;
            let count = whole(&count, "string_copy count")?.max(0);
            let start = usize::try_from(start).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(count).unwrap_or(usize::MAX));
            let end = end.min(chars.len());
            if start >= chars.len() {
                return Ok(ImmutableString::new());
            }
            Ok(ImmutableString::from(
                chars[start..end].iter().collect::<String>(),
            ))
        },
    );
    docs.add(
        "string_delete",
        "`string_delete(s, index, count)`: `s` without `count` characters from 1-based `index`.",
    );
    engine.register_fn(
        "string_delete",
        |s: Dynamic, index: Dynamic, count: Dynamic| -> Res<ImmutableString> {
            let chars = chars(&text(&s, "string_delete")?);
            let start = whole(&index, "string_delete index")?.max(1) - 1;
            let count = whole(&count, "string_delete count")?.max(0);
            let start = usize::try_from(start).unwrap_or(usize::MAX);
            if start >= chars.len() {
                return Ok(ImmutableString::from(chars.iter().collect::<String>()));
            }
            let end = start
                .saturating_add(usize::try_from(count).unwrap_or(usize::MAX))
                .min(chars.len());
            let out: String = chars[..start].iter().chain(&chars[end..]).collect();
            Ok(ImmutableString::from(out))
        },
    );
    docs.add(
        "string_insert",
        "`string_insert(substr, s, index)`: `substr` put before 1-based `index` of `s`; an index past the end appends.",
    );
    engine.register_fn(
        "string_insert",
        |substr: Dynamic, s: Dynamic, index: Dynamic| -> Res<ImmutableString> {
            let substr = text(&substr, "string_insert substr")?;
            let chars = chars(&text(&s, "string_insert")?);
            let at = whole(&index, "string_insert index")?.max(1) - 1;
            let at = usize::try_from(at).unwrap_or(usize::MAX).min(chars.len());
            let mut out: String = chars[..at].iter().collect();
            out.push_str(&substr);
            out.extend(&chars[at..]);
            Ok(ImmutableString::from(out))
        },
    );
    docs.add(
        "string_replace",
        "`string_replace(s, substr, new)`: replaces the first `substr` in `s`.",
    );
    engine.register_fn(
        "string_replace",
        |s: Dynamic, substr: Dynamic, new: Dynamic| -> Res<ImmutableString> {
            let (s, substr, new) = (
                text(&s, "string_replace")?,
                text(&substr, "string_replace substr")?,
                text(&new, "string_replace new")?,
            );
            if substr.is_empty() {
                return Ok(ImmutableString::from(s));
            }
            Ok(ImmutableString::from(s.replacen(&substr, &new, 1)))
        },
    );
    docs.add(
        "string_replace_all",
        "`string_replace_all(s, substr, new)`: replaces every `substr` in `s`.",
    );
    engine.register_fn(
        "string_replace_all",
        |s: Dynamic, substr: Dynamic, new: Dynamic| -> Res<ImmutableString> {
            let (s, substr, new) = (
                text(&s, "string_replace_all")?,
                text(&substr, "string_replace_all substr")?,
                text(&new, "string_replace_all new")?,
            );
            if substr.is_empty() {
                return Ok(ImmutableString::from(s));
            }
            Ok(ImmutableString::from(s.replace(&substr, &new)))
        },
    );
    docs.add(
        "string_count",
        "`string_count(substr, s)`: how many times `substr` occurs in `s`, without overlap.",
    );
    engine.register_fn("string_count", |substr: Dynamic, s: Dynamic| -> Res<f64> {
        let (substr, s) = (text(&substr, "string_count substr")?, text(&s, "string_count")?);
        if substr.is_empty() {
            return Ok(0.0);
        }
        Ok(s.matches(&substr).count() as f64)
    });
    docs.add("string_upper", "`string_upper(s)`: `s` in upper case.");
    engine.register_fn("string_upper", |s: Dynamic| -> Res<ImmutableString> {
        Ok(ImmutableString::from(text(&s, "string_upper")?.to_uppercase()))
    });
    docs.add("string_lower", "`string_lower(s)`: `s` in lower case.");
    engine.register_fn("string_lower", |s: Dynamic| -> Res<ImmutableString> {
        Ok(ImmutableString::from(text(&s, "string_lower")?.to_lowercase()))
    });
    docs.add(
        "string_digits",
        "`string_digits(s)`: only the ASCII digits of `s`.",
    );
    engine.register_fn("string_digits", |s: Dynamic| -> Res<ImmutableString> {
        let s = text(&s, "string_digits")?;
        Ok(ImmutableString::from(
            s.chars().filter(char::is_ascii_digit).collect::<String>(),
        ))
    });
    docs.add(
        "string_letters",
        "`string_letters(s)`: only the ASCII letters of `s`.",
    );
    engine.register_fn("string_letters", |s: Dynamic| -> Res<ImmutableString> {
        let s = text(&s, "string_letters")?;
        Ok(ImmutableString::from(
            s.chars().filter(char::is_ascii_alphabetic).collect::<String>(),
        ))
    });
    docs.add(
        "string_lettersdigits",
        "`string_lettersdigits(s)`: only the ASCII letters and digits of `s`.",
    );
    engine.register_fn("string_lettersdigits", |s: Dynamic| -> Res<ImmutableString> {
        let s = text(&s, "string_lettersdigits")?;
        Ok(ImmutableString::from(
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>(),
        ))
    });
    docs.add(
        "chr",
        "`chr(code)`: the character with code 0 to 255 (a one-character string).",
    );
    engine.register_fn("chr", |code: Dynamic| -> Res<ImmutableString> {
        let code = whole(&code, "chr")?;
        let c = u32::try_from(code)
            .ok()
            .filter(|c| *c <= 255)
            .and_then(char::from_u32)
            .ok_or_else(|| runtime_error(format!("chr({code}) is not a character code (0 to 255)")))?;
        Ok(ImmutableString::from(c.to_string()))
    });
    docs.add(
        "ord",
        "`ord(s)`: the character code of the first character of `s`, or 0 when it is empty.",
    );
    engine.register_fn("ord", |s: Dynamic| -> Res<f64> {
        let s = text(&s, "ord")?;
        Ok(s.chars().next().map_or(0.0, |c| f64::from(c as u32)))
    });
}
