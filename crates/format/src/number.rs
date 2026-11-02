//! Decimal numbers in data files (docs/format/v0.md, Common rules for data files).
//!
//! A decimal field takes a TOML integer, a TOML float, or `{ bits = "3FDCCCCCCCCCCCCD" }`:
//! 16 hex digits of an IEEE double, for constants that must match GM8 bit for bit.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};

/// A decimal value read from a data file. Holds exactly the double the file names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Number(pub f64);

impl Number {
    /// Parses the `bits` form: exactly 16 hex digits, either case.
    pub fn from_bits_hex(hex: &str) -> Result<Self, String> {
        if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "`bits` must be 16 hex digits of an IEEE double, got `{hex}`"
            ));
        }
        u64::from_str_radix(hex, 16)
            .map(|bits| Number(f64::from_bits(bits)))
            .map_err(|e| e.to_string())
    }
}

/// The schema of a [`Number`]: a TOML number, or `{ bits = "<16 hex digits>" }`.
#[cfg(feature = "schema")]
impl schemars::JsonSchema for Number {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Number".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A number, or { bits = \"<16 hex digits of an IEEE double>\" } for a constant that must match bit for bit.",
            "anyOf": [
                { "type": "number" },
                {
                    "type": "object",
                    "properties": { "bits": { "type": "string", "pattern": "^[0-9a-fA-F]{16}$" } },
                    "required": ["bits"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NumberVisitor)
    }
}

struct NumberVisitor;

impl<'de> Visitor<'de> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number, or { bits = \"<16 hex digits>\" }")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Number, E> {
        exact_integer(v as f64, v as i128)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Number, E> {
        exact_integer(v as f64, i128::from(v))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Number, E> {
        Ok(Number(v))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Number, A::Error> {
        let mut bits: Option<String> = None;
        while let Some(key) = map.next_key::<String>()? {
            if key != "bits" {
                return Err(de::Error::unknown_field(&key, &["bits"]));
            }
            if bits.is_some() {
                return Err(de::Error::duplicate_field("bits"));
            }
            bits = Some(map.next_value()?);
        }
        let bits = bits.ok_or_else(|| de::Error::missing_field("bits"))?;
        Number::from_bits_hex(&bits).map_err(de::Error::custom)
    }
}

/// Integers above 2^53 do not fit a double exactly; refuse them instead of rounding silently.
fn exact_integer<E: de::Error>(as_f64: f64, exact: i128) -> Result<Number, E> {
    if as_f64 as i128 == exact {
        Ok(Number(as_f64))
    } else {
        Err(E::custom(format!(
            "integer {exact} cannot be stored exactly as a double; write it with {{ bits = ... }}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Doc {
        v: Number,
    }

    fn parse(text: &str) -> Result<f64, toml::de::Error> {
        toml::from_str::<Doc>(text).map(|d| d.v.0)
    }

    #[test]
    fn integer_float_and_bits() {
        assert_eq!(parse("v = 2").unwrap().to_bits(), 2.0f64.to_bits());
        assert_eq!(parse("v = 0.45").unwrap().to_bits(), 0.45f64.to_bits());
        assert_eq!(
            parse("v = { bits = \"3FDCCCCCCCCCCCCD\" }").unwrap().to_bits(),
            0x3FDC_CCCC_CCCC_CCCD
        );
        assert_eq!(
            parse("v = { bits = \"3fdccccccccccccd\" }").unwrap().to_bits(),
            0x3FDC_CCCC_CCCC_CCCD
        );
    }

    #[test]
    fn bad_forms() {
        assert!(parse("v = { bits = \"3FDC\" }").is_err());
        assert!(parse("v = { bits = \"3FDCCCCCCCCCCCCG\" }").is_err());
        assert!(parse("v = { bit = \"3FDCCCCCCCCCCCCD\" }").is_err());
        assert!(parse("v = \"1.0\"").is_err());
        assert!(parse("v = 9007199254740993").is_err());
    }
}
