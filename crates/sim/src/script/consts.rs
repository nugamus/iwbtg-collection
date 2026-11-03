//! Top-level `const` items inside functions (#366).
//!
//! Rhai functions cannot see script variables, and the host calls them with a fresh scope, so
//! `const S = 4.0; fn f() { S * 2.0 }` does not compile ("Undefined variable: S"). A parse-token
//! hook fixes that while the script is lexed: a top-level `const NAME = <literal>;` is recorded
//! (a number, optionally negative, a string, `true` or `false`), and every later use of `NAME`
//! becomes that literal. So a function sees every const declared above it, and the cost of a use
//! is the cost of a literal.
//!
//! Limits, documented in the script docs: the value must be a literal (an expression such as
//! `2.0 * 3.0` makes the const usable at the top level only), a const must be declared before the
//! function that uses it, and its name cannot be reused for a variable or a parameter.

use std::collections::BTreeMap;

use rhai::Token;

/// The lexer-side state of the const hook. Reset at the start of every compile.
#[derive(Debug, Default)]
pub(crate) struct ConstScan {
    /// Bracket depth; consts are recorded at depth 0 only.
    depth: usize,
    stage: Stage,
    /// The name being declared.
    name: String,
    negative: bool,
    /// The literal read after `=`, committed at the `;`.
    value: Option<Token>,
    /// The recorded consts, by name.
    consts: BTreeMap<String, Token>,
    /// The previous token was one that declares or accesses a name, not a use of a const.
    skip_next: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    /// After `const`.
    Name,
    /// After the name.
    Equals,
    /// After `=`.
    Value,
}

impl ConstScan {
    pub(crate) fn reset(&mut self) {
        *self = ConstScan::default();
    }

    /// Sees one token and returns the token the parser should get.
    pub(crate) fn token(&mut self, tok: Token) -> Token {
        let skip = std::mem::take(&mut self.skip_next);
        if matches!(tok, Token::EOF) {
            self.reset();
            return tok;
        }
        let out = match &tok {
            Token::Identifier(name) if !skip => self
                .consts
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| tok.clone()),
            _ => tok.clone(),
        };
        self.skip_next = matches!(tok, Token::Const | Token::Let | Token::Period | Token::Fn);
        self.track(&tok, &out);
        out
    }

    fn track(&mut self, original: &Token, seen: &Token) {
        match original {
            Token::LeftBrace | Token::MapStart | Token::LeftParen | Token::LeftBracket => self.depth += 1,
            Token::RightBrace | Token::RightParen | Token::RightBracket => {
                self.depth = self.depth.saturating_sub(1);
            }
            _ => {}
        }
        if self.depth != 0 && !matches!(self.stage, Stage::Idle) {
            self.stage = Stage::Idle;
        }
        match (self.stage, original) {
            (_, Token::Const) if self.depth == 0 => {
                self.stage = Stage::Name;
                self.value = None;
                self.negative = false;
            }
            (Stage::Name, Token::Identifier(name)) => {
                self.name = name.to_string();
                self.stage = Stage::Equals;
            }
            (Stage::Equals, Token::Equals) => self.stage = Stage::Value,
            (Stage::Value, Token::Minus) if self.value.is_none() && !self.negative => self.negative = true,
            (Stage::Value, Token::SemiColon) => {
                if let Some(v) = self.value.take() {
                    self.consts.insert(std::mem::take(&mut self.name), v);
                }
                self.stage = Stage::Idle;
            }
            (Stage::Value, _) if self.value.is_none() => {
                self.value = literal(seen, self.negative);
                if self.value.is_none() {
                    self.stage = Stage::Idle;
                }
            }
            (Stage::Value, _) => {
                // More than a literal: not a usable const.
                self.value = None;
                self.stage = Stage::Idle;
            }
            _ => {}
        }
    }
}

/// The token as a const value, negated when `negative`; `None` when it is no literal.
fn literal(tok: &Token, negative: bool) -> Option<Token> {
    match tok {
        Token::IntegerConstant(i) if negative => i.checked_neg().map(Token::IntegerConstant),
        Token::FloatConstant(f) if negative => {
            let mut f = f.clone();
            f.0 = rhai::FloatWrapper::new(-*f.0);
            Some(Token::FloatConstant(f))
        }
        Token::IntegerConstant(_) | Token::FloatConstant(_) => Some(tok.clone()),
        Token::StringConstant(_) | Token::True | Token::False if !negative => Some(tok.clone()),
        _ => None,
    }
}
