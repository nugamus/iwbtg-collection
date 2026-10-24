//! Diagnostics: errors and warnings that name file, line and column
//! (docs/format/v0.md, Validation).

use std::fmt;

/// How bad a diagnostic is. A package with any `Error` is refused by the client and the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The package is refused.
    Error,
    /// The package loads, but something should be fixed.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// One readable problem in a package, printed as `file:line:column: severity: message`.
///
/// `line` and `column` are 1-based; the column counts Unicode characters. A problem with a
/// whole file (missing, not UTF-8 before any line) points at line 1, column 1.
#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
#[error("{file}:{line}:{column}: {severity}: {message}")]
pub struct Diagnostic {
    /// Package-relative path with `/` separators, as in [`crate::PackageFiles`].
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, in characters.
    pub column: u32,
    /// Error or warning.
    pub severity: Severity,
    /// What is wrong, in one sentence.
    pub message: String,
}

impl Diagnostic {
    /// An error at byte `offset` of `text`, the contents of `file`.
    pub fn error_at(file: &str, text: &str, offset: usize, message: impl Into<String>) -> Self {
        Self::at(Severity::Error, file, text, offset, message)
    }

    /// A warning at byte `offset` of `text`, the contents of `file`.
    pub fn warning_at(file: &str, text: &str, offset: usize, message: impl Into<String>) -> Self {
        Self::at(Severity::Warning, file, text, offset, message)
    }

    /// An error about a whole file, pointing at line 1, column 1.
    pub fn file_error(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_owned(),
            line: 1,
            column: 1,
            severity: Severity::Error,
            message: message.into(),
        }
    }

    /// A diagnostic at byte `offset` of `text`.
    pub fn at(severity: Severity, file: &str, text: &str, offset: usize, message: impl Into<String>) -> Self {
        let (line, column) = line_col(text, offset);
        Self {
            file: file.to_owned(),
            line,
            column,
            severity,
            message: message.into(),
        }
    }

    /// True for [`Severity::Error`].
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// True when any diagnostic is an error, so the package must be refused.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}

/// 1-based line and character column of byte `offset` in `text`.
///
/// Offsets past the end clamp to the end; offsets inside a multi-byte character count as that
/// character. Counts saturate at `u32::MAX`.
pub fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count().saturating_add(1);
    let column = before[line_start..].chars().count().saturating_add(1);
    (to_u32(line), to_u32(column))
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_counts_from_one() {
        let text = "a = 1\nbb = 2\n";
        assert_eq!(line_col(text, 0), (1, 1));
        assert_eq!(line_col(text, 4), (1, 5));
        assert_eq!(line_col(text, 6), (2, 1));
        assert_eq!(line_col(text, 11), (2, 6));
        assert_eq!(line_col(text, 999), (3, 1));
    }

    #[test]
    fn line_col_counts_characters_not_bytes() {
        let text = "t = \"é\" x";
        // 'x' is byte 9 but the 9th character (column 9), because 'é' takes two bytes.
        assert_eq!(text.find('x'), Some(9));
        assert_eq!(line_col(text, 9), (1, 9));
        // The closing quote is byte 7, character 7.
        assert_eq!(line_col(text, 7), (1, 7));
        // Inside the two-byte 'é' counts as the 'é'.
        assert_eq!(line_col(text, 6), (1, 6));
    }

    #[test]
    fn display_is_file_line_column() {
        let d = Diagnostic::error_at("game.toml", "a\nbc", 3, "bad");
        assert_eq!(d.to_string(), "game.toml:2:2: error: bad");
    }
}
