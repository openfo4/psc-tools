use std::borrow::Cow;
use std::num::ParseIntError;
use std::ops::Range;

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token<'a> {
    pub span: Range<usize>,
    pub kind: TokenKind<'a>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind<'a> {
    Ident(&'a str),
    Literal(Literal<'a>),
    /// `&&`
    LogicalAnd,
    /// `||`
    LogicalOr,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `[`
    OpenBracket,
    /// `]`
    CloseBracket,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `=`
    Eq,
    /// `!=`
    BangEq,
    /// `==`
    EqEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `!`
    Bang,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `%=`
    PercentEq,
    /// End of line
    Eol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal<'a> {
    None,
    Bool(bool),
    NumberInt(i32),
    NumberDecimal(DecimalLiteral),
    String(Cow<'a, str>),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DecimalLiteral {
    pub integer: i32,
    pub fraction: u32,
}

impl DecimalLiteral {
    pub fn to_f32(&self) -> f32 {
        if self.fraction == 0 {
            self.integer as f32
        } else {
            let frac = 0.1_f32.powi(self.fraction.ilog10().cast_signed());
            self.integer as f32 + frac
        }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<Token<'_>>, Error> {
    let mut tokens: Vec<Token<'_>> = Vec::new();

    let mut cur_ident_start = 0;
    let mut cur_ident_end = 0;

    let mut offset = 0;

    loop {
        if input[offset..].is_empty() {
            break;
        }

        let (token, next) = match &input[offset..] {
            // Doc comment
            s if s.starts_with("{") => {
                let end = s.find("}").ok_or_else(|| Error {
                    span: offset..offset + "}".len(),
                    kind: ErrorKind::UnterminatedDocComment,
                })?;

                (None, &s[end + 1..])
            }
            // Block comment
            s if s.starts_with(";/") => {
                let end = s.find("/;").map(|i| i + "/;".len()).unwrap_or(s.len());
                (None, &s[end..])
            }
            // Line comment
            s if s.starts_with(";") => {
                let end = s.find("\n").unwrap_or(s.len());
                (None, &s[end..])
            }
            // Combined
            s if let Some(next) = s.strip_prefix("==") => (Some(TokenKind::EqEq), next),
            s if let Some(next) = s.strip_prefix("!=") => (Some(TokenKind::BangEq), next),
            s if let Some(next) = s.strip_prefix(">=") => (Some(TokenKind::GtEq), next),
            s if let Some(next) = s.strip_prefix("<=") => (Some(TokenKind::LtEq), next),
            s if let Some(next) = s.strip_prefix("+=") => (Some(TokenKind::PlusEq), next),
            s if let Some(next) = s.strip_prefix("-=") => (Some(TokenKind::MinusEq), next),
            s if let Some(next) = s.strip_prefix("*=") => (Some(TokenKind::StarEq), next),
            s if let Some(next) = s.strip_prefix("/=") => (Some(TokenKind::SlashEq), next),
            s if let Some(next) = s.strip_prefix("%=") => (Some(TokenKind::PercentEq), next),
            // Separate
            s if let Some(next) = s.strip_prefix("+") => (Some(TokenKind::Plus), next),
            s if let Some(next) = s.strip_prefix("-") => (Some(TokenKind::Minus), next),
            s if let Some(next) = s.strip_prefix("*") => (Some(TokenKind::Star), next),
            s if let Some(next) = s.strip_prefix("/") => (Some(TokenKind::Slash), next),
            s if let Some(next) = s.strip_prefix("%") => (Some(TokenKind::Percent), next),
            s if let Some(next) = s.strip_prefix("&&") => (Some(TokenKind::LogicalAnd), next),
            s if let Some(next) = s.strip_prefix("||") => (Some(TokenKind::LogicalOr), next),
            s if let Some(next) = s.strip_prefix("(") => (Some(TokenKind::OpenParen), next),
            s if let Some(next) = s.strip_prefix(")") => (Some(TokenKind::CloseParen), next),
            s if let Some(next) = s.strip_prefix("[") => (Some(TokenKind::OpenBracket), next),
            s if let Some(next) = s.strip_prefix("]") => (Some(TokenKind::CloseBracket), next),
            s if let Some(next) = s.strip_prefix(".") => (Some(TokenKind::Dot), next),
            s if let Some(next) = s.strip_prefix(",") => (Some(TokenKind::Comma), next),
            s if let Some(next) = s.strip_prefix("=") => (Some(TokenKind::Eq), next),
            s if let Some(next) = s.strip_prefix("<") => (Some(TokenKind::Lt), next),
            s if let Some(next) = s.strip_prefix(">") => (Some(TokenKind::Gt), next),
            s if let Some(next) = s.strip_prefix("!") => (Some(TokenKind::Bang), next),
            s if let Some(next) = s.strip_prefix(" ") => (None, next),
            s if let Some(next) = s.strip_prefix("\r") => (None, next),
            s if let Some(next) = s.strip_prefix("\n") => {
                if tokens
                    .last()
                    .is_some_and(|token| token.kind == TokenKind::Eol)
                    && cur_ident_start == cur_ident_end
                {
                    (None, next)
                } else {
                    (Some(TokenKind::Eol), next)
                }
            }
            s if let Some(next) = s.strip_prefix("\t") => (None, next),
            // Number
            s if cur_ident_start == cur_ident_end
                && s.chars().next().is_some_and(|v| v.is_ascii_digit()) =>
            {
                let end = s
                    .bytes()
                    .position(|b| !b.is_ascii_digit())
                    .unwrap_or(s.len());
                let integer = s[..end].parse().map_err(|err| Error {
                    span: offset..offset + end,
                    kind: ErrorKind::ParseIntLitError(err),
                })?;

                // We have a decimal number, the fraction follows after the dot.
                if let Some(frac_str) = s[end..].strip_prefix('.') {
                    let frac_end = frac_str
                        .bytes()
                        .position(|b| !b.is_ascii_digit())
                        .unwrap_or(frac_str.len());
                    let fraction = frac_str[..frac_end].parse().map_err(|err| Error {
                        span: offset..offset + end,
                        kind: ErrorKind::ParseIntLitError(err),
                    })?;

                    let end = end + frac_end + '.'.len_utf8();

                    (
                        Some(TokenKind::Literal(Literal::NumberDecimal(DecimalLiteral {
                            integer,
                            fraction,
                        }))),
                        &s[end..],
                    )
                } else {
                    (
                        Some(TokenKind::Literal(Literal::NumberInt(integer))),
                        &s[end..],
                    )
                }
            }
            // String Literal
            s if let Some(s) = s.strip_prefix("\"") => {
                let mut search_space = s;
                let mut end_offset = 0;
                let end = loop {
                    let end = search_space.find("\"").ok_or_else(|| Error {
                        span: offset..offset + "\"".len(),
                        kind: ErrorKind::UnterminatedStringLiteral,
                    })?;

                    if search_space[..end].ends_with("\\") {
                        end_offset += end + 1;
                        search_space = &search_space[end + 1..];
                        continue;
                    }

                    break end + end_offset;
                };

                let string = unescape_string(&s[..end]).map_err(|mut err| {
                    let shift = offset + "\"".len();
                    err.span = shift + err.span.start..shift + err.span.end;
                    err
                })?;

                (
                    Some(TokenKind::Literal(Literal::String(string))),
                    &s[end + 1..],
                )
            }
            s => {
                let end = s.chars().next().unwrap().len_utf8();
                cur_ident_end += end;
                offset += end;
                continue;
            }
        };

        let new_offset = offset + input[offset..].len() - next.len();

        if cur_ident_start != cur_ident_end {
            debug_assert!(cur_ident_end > cur_ident_start);

            let ident = &input[cur_ident_start..cur_ident_end];
            let kind = if ident.eq_ignore_ascii_case("none") {
                TokenKind::Literal(Literal::None)
            } else if ident.eq_ignore_ascii_case("true") {
                TokenKind::Literal(Literal::Bool(true))
            } else if ident.eq_ignore_ascii_case("false") {
                TokenKind::Literal(Literal::Bool(false))
            } else {
                TokenKind::Ident(ident)
            };

            tokens.push(Token {
                span: cur_ident_start..cur_ident_end,
                kind,
            });
        }

        cur_ident_start = new_offset;
        cur_ident_end = new_offset;

        if let Some(token) = token {
            tokens.push(Token {
                span: offset..new_offset,
                kind: token,
            });
        }

        offset = new_offset;
    }

    if cur_ident_start != cur_ident_end {
        debug_assert!(cur_ident_end > cur_ident_start);
        let ident = &input[cur_ident_start..cur_ident_end];
        let kind = if ident.eq_ignore_ascii_case("none") {
            TokenKind::Literal(Literal::None)
        } else if ident.eq_ignore_ascii_case("true") {
            TokenKind::Literal(Literal::Bool(true))
        } else if ident.eq_ignore_ascii_case("false") {
            TokenKind::Literal(Literal::Bool(false))
        } else {
            TokenKind::Ident(ident)
        };

        tokens.push(Token {
            span: cur_ident_start..cur_ident_end,
            kind,
        });
    }

    Ok(tokens)
}

#[derive(Clone, Debug)]
pub struct Error {
    pub span: Range<usize>,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, Error)]
pub enum ErrorKind {
    #[error("{0}")]
    ParseIntLitError(ParseIntError),
    #[error("unterminated doc comment")]
    UnterminatedDocComment,
    #[error("unterminated string literal")]
    UnterminatedStringLiteral,
    #[error("unknown escape code: {code}")]
    UnknownEscapeCode { code: char },
}

fn unescape_string(s: &str) -> Result<Cow<'_, str>, Error> {
    let mut s = Cow::Borrowed(s);

    let mut shift = 0;

    loop {
        let Some(index) = s.find("\\") else {
            return Ok(s);
        };

        let code = s[index + 1..].chars().next().unwrap();
        let replacement = match code {
            '\\' => "",
            'n' => "\n",
            'r' => "\r",
            't' => "\t",
            '\"' => "\"",
            _ => {
                return Err(Error {
                    span: shift + index..shift + index + code.len_utf8() + 1,
                    kind: ErrorKind::UnknownEscapeCode { code },
                });
            }
        };

        shift += code.len_utf8();
        shift += "\\".len();
        shift -= replacement.len();

        let s = s.to_mut();
        s.replace_range(index..index + code.len_utf8() + 1, replacement);
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{DecimalLiteral, Literal, Token, TokenKind, tokenize};

    #[test]
    fn decimal_literal() {
        let input = "501.623";
        let tokens = tokenize(&input).unwrap();
        assert_eq!(
            tokens,
            [Token {
                span: 0..7,
                kind: TokenKind::Literal(Literal::NumberDecimal(DecimalLiteral {
                    integer: 501,
                    fraction: 623
                }))
            }]
        );
    }

    #[test]
    fn eol_after_line_comment() {
        let input = "Hello ; Comment\n World";
        let tokens = tokenize(&input).unwrap();

        assert_eq!(
            tokens,
            [
                Token {
                    span: 0..5,
                    kind: TokenKind::Ident("Hello")
                },
                Token {
                    span: 15..16,
                    kind: TokenKind::Eol
                },
                Token {
                    span: 17..22,
                    kind: TokenKind::Ident("World")
                }
            ]
        );
    }

    #[test]
    fn eol_after_block_comment() {
        let input = "Hello ;/ Comment /;\nWorld";
        let tokens = tokenize(&input).unwrap();
        assert_eq!(
            tokens,
            [
                Token {
                    span: 0..5,
                    kind: TokenKind::Ident("Hello")
                },
                Token {
                    span: 19..20,
                    kind: TokenKind::Eol
                },
                Token {
                    span: 20..25,
                    kind: TokenKind::Ident("World")
                }
            ]
        );
    }
}
