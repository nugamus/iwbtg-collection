//! Id and reference syntax (docs/format/v0.md, Ids and references).

/// Longest allowed id, in bytes (ids are ASCII).
pub const MAX_ID_LEN: usize = 48;

/// Checks an id: lowercase snake_case ASCII `[a-z][a-z0-9_]*`, at most [`MAX_ID_LEN`] characters.
/// Returns why it is invalid.
pub fn check_id(id: &str) -> Result<(), String> {
    let Some(first) = id.bytes().next() else {
        return Err("id is empty".to_owned());
    };
    if !first.is_ascii_lowercase() {
        return Err(format!("id `{id}` must start with a lowercase letter a-z"));
    }
    if let Some(bad) = id
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
    {
        return Err(format!(
            "id `{id}` contains `{bad}`; ids use only a-z, 0-9 and `_`"
        ));
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!(
            "id `{id}` is {} characters long; the limit is {MAX_ID_LEN}",
            id.len()
        ));
    }
    Ok(())
}

/// A reference to an object kind, as written in `object_order` and room instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KindRef<'a> {
    /// A built-in kind, written bare: `spike`.
    BuiltIn(&'a str),
    /// A package object, written `@boss1`, defined in `objects/boss1.toml`.
    Package(&'a str),
}

impl<'a> KindRef<'a> {
    /// Parses `spike` or `@boss1`, checking the id syntax.
    pub fn parse(text: &'a str) -> Result<Self, String> {
        match text.strip_prefix('@') {
            Some(id) => check_id(id).map(|()| KindRef::Package(id)),
            None => check_id(text).map(|()| KindRef::BuiltIn(text)),
        }
    }
}

/// Checks a semantic version `MAJOR.MINOR.PATCH`, with optional `-pre.release` and `+build`
/// parts (semver.org 2.0.0).
pub fn check_semver(version: &str) -> Result<(), String> {
    let err = || format!("version `{version}` is not semver (`MAJOR.MINOR.PATCH`, for example `1.0.0`)");
    let (rest, build) = match version.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (version, None),
    };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    if numbers.len() != 3 || !numbers.iter().all(|n| is_numeric_identifier(n)) {
        return Err(err());
    }
    for (part, numeric_rule) in [(pre, true), (build, false)] {
        if let Some(part) = part {
            for ident in part.split('.') {
                let ok = !ident.is_empty()
                    && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    && (!numeric_rule
                        || !ident.bytes().all(|b| b.is_ascii_digit())
                        || is_numeric_identifier(ident));
                if !ok {
                    return Err(err());
                }
            }
        }
    }
    Ok(())
}

/// Digits with no leading zero (a lone `0` is fine).
fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(check_id("rely_the_needle").is_ok());
        assert!(check_id("a").is_ok());
        assert!(check_id("stage1_01").is_ok());
        assert!(check_id(&"a".repeat(48)).is_ok());
        for bad in ["", "1abc", "_a", "Abc", "a-b", "a b", "é", &"a".repeat(49)] {
            assert!(check_id(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(KindRef::parse("spike"), Ok(KindRef::BuiltIn("spike")));
        assert_eq!(KindRef::parse("@boss1"), Ok(KindRef::Package("boss1")));
        assert!(KindRef::parse("@").is_err());
        assert!(KindRef::parse("@@a").is_err());
        assert!(KindRef::parse("Spike").is_err());
    }

    #[test]
    fn semver() {
        for good in [
            "1.0.0",
            "0.1.0",
            "10.20.30",
            "1.0.0-alpha.1",
            "1.0.0+build.5",
            "1.0.0-rc-1+x",
        ] {
            assert!(check_semver(good).is_ok(), "{good:?} rejected");
        }
        for bad in [
            "1.0",
            "1",
            "01.0.0",
            "1.0.0-",
            "1.0.0-01",
            "1.0.0+",
            "v1.0.0",
            "1.0.0-a..b",
        ] {
            assert!(check_semver(bad).is_err(), "{bad:?} accepted");
        }
    }
}
