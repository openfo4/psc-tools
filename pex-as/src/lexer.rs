use core::fmt::{self, Display, Formatter};
use core::range::Range;
use std::borrow::Cow;

use crate::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind<'a> {
    Ident(&'a str),
    Literal(Literal<'a>),
    /// `:`
    Colon,
    /// `=`
    Eq,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `,`
    Comma,
    /// `->`
    Arrow,
    Eol,
}

impl Display for TokenKind<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ident(ident) => write!(f, "`{}`", ident),
            Self::Colon => write!(f, "`:`"),
            Self::Eq => write!(f, "`=`"),
            Self::OpenParen => write!(f, "`(`"),
            Self::CloseParen => write!(f, "`)`"),
            Self::Comma => write!(f, "`,`"),
            Self::Arrow => write!(f, "`->`"),
            Self::Eol => write!(f, "end of line"),
            Self::Literal(_) => todo!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal<'a> {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    String(Cow<'a, str>),
}

pub fn tokenize(input: &str) -> Result<Vec<Token<'_>>, Error> {
    let mut tokens = Vec::new();

    let mut buf = input;
    let mut ident = 0..0;
    let mut offset = 0;

    while !buf.is_empty() {
        // Comment
        if buf.starts_with("//") {
            match buf.find("\n") {
                Some(index) => buf = &buf[index + 1..],
                None => buf = "",
            }

            continue;
        }

        let (token, len) = match buf {
            buf if buf.starts_with(':') => (Some(TokenKind::Colon), ':'.len_utf8()),
            buf if buf.starts_with(',') => (Some(TokenKind::Comma), ','.len_utf8()),
            buf if buf.starts_with('=') => (Some(TokenKind::Eq), '='.len_utf8()),
            buf if buf.starts_with('(') => (Some(TokenKind::OpenParen), '('.len_utf8()),
            buf if buf.starts_with(')') => (Some(TokenKind::CloseParen), ')'.len_utf8()),
            buf if buf.starts_with("->") => (Some(TokenKind::Arrow), "->".len()),
            buf if buf.starts_with("\n") => (Some(TokenKind::Eol), "\n".len()),
            buf if buf.starts_with("\r\n") => (Some(TokenKind::Eol), "\r\n".len()),
            buf if buf.starts_with("\"") => {
                let Some(end) = buf[1..].find(|c| c == '"') else {
                    return Err(Error {
                        msg: "unterminated string literal".to_owned(),
                        span: Range {
                            start: offset,
                            end: input.len(),
                        },
                    });
                };

                let s = &buf[1..end + 1];

                (
                    Some(TokenKind::Literal(Literal::String(s.into()))),
                    s.len() + 2,
                )
            }
            buf if let Some(next) = buf.strip_prefix("none")
                && starts_with_whitespace_or_eol(next) =>
            {
                (Some(TokenKind::Literal(Literal::None)), "none".len())
            }
            buf if let Some(next) = buf.strip_prefix("true")
                && starts_with_whitespace_or_eol(next) =>
            {
                (Some(TokenKind::Literal(Literal::Bool(true))), "true".len())
            }
            buf if let Some(next) = buf.strip_prefix("false")
                && starts_with_whitespace_or_eol(next) =>
            {
                (
                    Some(TokenKind::Literal(Literal::Bool(false))),
                    "false".len(),
                )
            }
            buf => match buf.chars().next() {
                Some(c) if c.is_ascii_digit() || c == '-' => {
                    let minus_offset = usize::from(c == '-');

                    let end = buf[minus_offset..]
                        .chars()
                        .position(|v| !v.is_ascii_digit())
                        .map(|end| end + minus_offset)
                        .unwrap_or(buf.len());
                    let int = buf[..end].parse().unwrap();

                    match buf.get(end..).and_then(|s| s.chars().next()) {
                        Some('.') => {
                            let start = end + 1;
                            let end = buf[start..]
                                .chars()
                                .position(|v| !v.is_ascii_digit())
                                .map(|end| start + end)
                                .unwrap_or(buf.len());

                            let frac: u32 = buf[start..end].parse().unwrap();
                            let frac =
                                frac as f32 * 0.1_f32.powi(ilog10_roundup(frac).cast_signed());
                            let value = int as f32 + frac;

                            (
                                Some(TokenKind::Literal(Literal::Float(value))),
                                buf[..end].len(),
                            )
                        }
                        Some('f') => (
                            Some(TokenKind::Literal(Literal::Float(int as f32))),
                            buf[..end].len() + 1,
                        ),
                        Some(c) if c.is_whitespace() => (
                            Some(TokenKind::Literal(Literal::Int(int))),
                            buf[..end].len(),
                        ),
                        _ => todo!(),
                    }
                }
                Some(c) if c.is_whitespace() => {
                    if !ident.is_empty() {
                        tokens.push(Token {
                            kind: TokenKind::Ident(&input[ident.clone()]),
                            span: ident.into(),
                        });

                        ident = 0..0;
                    }

                    (None, c.len_utf8())
                }
                Some(c) => {
                    if ident.is_empty() {
                        ident.start = offset;
                        ident.end = offset;
                    }

                    ident.end += c.len_utf8();

                    (None, c.len_utf8())
                }
                None => break,
            },
        };

        if let Some(token) = token {
            if !ident.is_empty() {
                tokens.push(Token {
                    kind: TokenKind::Ident(&input[ident.clone()]),
                    span: ident.into(),
                });

                ident = 0..0;
            }

            // We don't care about multiple sequential linebreaks.
            if tokens
                .last()
                .is_none_or(|token| token.kind != TokenKind::Eol)
                || token != TokenKind::Eol
            {
                tokens.push(Token {
                    kind: token,
                    span: Range {
                        start: offset,
                        end: offset + len,
                    },
                });
            }
        }

        buf = &buf[len..];
        offset += len;
    }

    if !ident.is_empty() {
        tokens.push(Token {
            kind: TokenKind::Ident(&input[ident.clone()]),
            span: ident.into(),
        });
    }

    Ok(tokens)
}

fn starts_with_whitespace_or_eol(s: &str) -> bool {
    match s.chars().next() {
        Some(c) => c.is_whitespace(),
        None => true,
    }
}

fn ilog10_roundup(x: u32) -> u32 {
    x.saturating_sub(1)
        .checked_ilog10()
        .map(|x| x + 1)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::lexer::{Literal, Token, TokenKind, tokenize};

    #[test]
    fn tokenize_int_lit() {
        let tokens = tokenize("x 123 y").unwrap();
        assert_eq!(
            tokens,
            [
                Token {
                    kind: TokenKind::Ident("x"),
                    span: (0..1).into(),
                },
                Token {
                    kind: TokenKind::Literal(Literal::Int(123)),
                    span: (2..5).into(),
                },
                Token {
                    kind: TokenKind::Ident("y"),
                    span: (6..7).into(),
                }
            ]
        );
    }

    #[test]
    fn tokenize_int_lit_negative() {
        let tokens = tokenize("x -12 y").unwrap();
        assert_eq!(
            tokens,
            [
                Token {
                    kind: TokenKind::Ident("x"),
                    span: (0..1).into(),
                },
                Token {
                    kind: TokenKind::Literal(Literal::Int(-12)),
                    span: (2..5).into(),
                },
                Token {
                    kind: TokenKind::Ident("y"),
                    span: (6..7).into(),
                }
            ]
        );
    }

    #[test]
    fn tokenize_float_lit() {
        let tokens = tokenize("x 12.34 y").unwrap();
        assert_eq!(
            tokens,
            [
                Token {
                    kind: TokenKind::Ident("x"),
                    span: (0..1).into()
                },
                Token {
                    kind: TokenKind::Literal(Literal::Float(12.34)),
                    span: (2..7).into(),
                },
                Token {
                    kind: TokenKind::Ident("y"),
                    span: (8..9).into(),
                }
            ]
        );
    }

    #[test]
    fn tokenize_float_lit_suffix() {
        let tokens = tokenize("12f").unwrap();
        assert_eq!(
            tokens,
            [Token {
                kind: TokenKind::Literal(Literal::Float(12.0)),
                span: (0..3).into(),
            }]
        );
    }
}
