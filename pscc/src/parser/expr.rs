use std::borrow::Cow;

use crate::ast::{Expr, Type};
use crate::lexer::{Literal, TokenKind};
use crate::parser::{Error, IdentKeyword, Keyword, ParseToken, Parser};

impl<'a> Parser<'a> {
    pub fn parse_expr(&mut self) -> Result<Expr, Error<'a>> {
        let mut stacks = vec![ExprParseStack::default()];

        while let Some(token) = self.peek() {
            // Some tokens are only allowed in an interior place as part of an expression
            // (e.g. ',' is only allowed when a function call is open, ')' is only allowed
            // when an '(' existed before).
            // These tokens indicate that the expression is over.
            // Note that we must not consume the token itself.
            if matches!(token.kind, TokenKind::Comma | TokenKind::CloseParen) && stacks.len() == 1 {
                break;
            }

            self.consume();

            let mut stack = stacks.last_mut().unwrap();

            match &token.kind {
                // New struct in the form `new Type`.
                // New array in the form `new <Type>[size]`.
                _ if token.keyword() == Some(Keyword::New) => {
                    let ty = self.parse_inline_type()?;

                    if self
                        .peek()
                        .is_some_and(|tok| tok.kind == TokenKind::OpenBracket)
                    {
                        self.consume_next(&[ParseToken::Token(&TokenKind::OpenBracket)])?;
                        stack.new_op = Some(ty);

                        stacks.push(ExprParseStack::default());
                        continue;
                    } else {
                        stack.shift(ExprOrToken::Expr(Expr::New { ty }));
                    }
                }
                TokenKind::OpenBracket => {
                    stacks.push(ExprParseStack::default());
                    continue;
                }
                TokenKind::CloseBracket => {
                    let expr = stacks.pop().unwrap().finalize();

                    stack = stacks.last_mut().unwrap();

                    if let Some(ty) = stack.new_op.take() {
                        stack.shift(ExprOrToken::Expr(Expr::NewArray {
                            ty,
                            len: Box::new(expr),
                        }));
                    } else {
                        let array = stack.stack.pop().unwrap().into_expr();

                        stack.shift(ExprOrToken::Expr(Expr::Index {
                            array: Box::new(array),
                            index: Box::new(expr),
                        }));
                    }
                }
                TokenKind::OpenParen => {
                    if stack.ends_with_ident() {
                        stack.ends_with_call = true;

                        // Deref has higher precedence that the function call.
                        while stack.ends_with_op(Op::Deref) {
                            stack.reduce_top(Op::Deref);
                        }
                    }

                    stacks.push(ExprParseStack::default());
                    continue;
                }
                TokenKind::CloseParen => {
                    let expr = stacks
                        .pop()
                        .and_then(|stack| (!stack.stack.is_empty()).then(|| stack.finalize()));

                    stack = stacks.last_mut().unwrap();

                    if stack.ends_with_call {
                        if let Some(expr) = expr {
                            stack.call_params.push(expr);
                        }

                        stack.ends_with_call = false;

                        let callee = Box::new(stack.stack.pop().unwrap().into_expr());
                        let args = core::mem::take(&mut stack.call_params);
                        stack.shift(ExprOrToken::Expr(Expr::Call { callee, args }));
                    } else {
                        let expr = expr.unwrap();
                        stack.shift(ExprOrToken::Expr(expr));
                    }
                }
                TokenKind::Comma => {
                    let expr = stacks.pop().unwrap().finalize();

                    stack = stacks.last_mut().unwrap();
                    stack.call_params.push(expr);

                    stacks.push(ExprParseStack::default());
                    continue;
                }
                TokenKind::Eol => break,
                _ => {
                    stack.shift(ExprOrToken::Token(&token.kind));
                }
            }

            let next = self
                .peek()
                .map(|token| &token.kind)
                .unwrap_or(&TokenKind::Eol);
            stack.reduce(next);
        }

        debug_assert_eq!(stacks.len(), 1);
        Ok(stacks.pop().unwrap().finalize())
    }
}

#[derive(Clone, Debug)]
enum ExprOrToken<'a> {
    Token(&'a TokenKind<'a>),
    Expr(Expr),
}

impl ExprOrToken<'_> {
    fn into_expr(self) -> Expr {
        match self {
            Self::Expr(expr) => expr,
            Self::Token(TokenKind::Ident(ident)) => Expr::Ident {
                ident: ident.to_string(),
            },
            Self::Token(TokenKind::Literal(literal)) => match literal {
                Literal::NumberInt(lit) => Expr::Literal(Literal::NumberInt(*lit)),
                Literal::NumberDecimal { integer, fraction } => {
                    Expr::Literal(Literal::NumberDecimal {
                        integer: *integer,
                        fraction: *fraction,
                    })
                }
                Literal::String(lit) => Expr::Literal(Literal::String(Cow::Owned(lit.to_string()))),
            },
            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ExprParseStack<'a> {
    stack: Vec<ExprOrToken<'a>>,
    call_params: Vec<Expr>,
    ends_with_call: bool,
    new_op: Option<Type>,
}

impl<'a> ExprParseStack<'a> {
    fn shift(&mut self, next: ExprOrToken<'a>) {
        self.stack.push(next);
    }

    fn reduce(&mut self, next: &TokenKind<'_>) {
        const PRECEDENCE: &[&[Op]] = &[
            &[Op::Deref],
            &[Op::Neg, Op::LogicalNot],
            &[Op::Mul, Op::Div, Op::Rem],
            &[Op::Add, Op::Sub],
            &[
                Op::CmpEq,
                Op::CmpNe,
                Op::CmpGt,
                Op::CmpGe,
                Op::CmpLt,
                Op::CmpLe,
            ],
            &[Op::LogicalAnd],
            &[Op::LogicalOr],
            &[
                Op::Assign,
                Op::AddAssign,
                Op::SubAssign,
                Op::MulAssign,
                Op::DivAssign,
                Op::RemAssign,
            ],
        ];

        if *next == TokenKind::Eol {
            // Reduce all operators with higher or equal precedence.
            loop {
                let mut found = false;
                for op in PRECEDENCE.iter().copied().flatten() {
                    if self.ends_with_op(*op) {
                        found = true;
                        self.reduce_top(*op);
                    }
                }

                if !found {
                    break;
                }
            }

            return;
        }

        for (index, tokens) in PRECEDENCE.iter().enumerate() {
            if !tokens.iter().any(|token| token.operator() == *next) {
                continue;
            }

            // Reduce all operators with higher or equal precedence.
            loop {
                let mut found = false;

                // Reduce all operators with higher precedence than the next peekded
                // operator.
                for op in PRECEDENCE[..index].iter().copied().flatten() {
                    if self.ends_with_op(*op) {
                        found = true;
                        self.reduce_top(*op);
                    }
                }

                // Reduce all operators with equal precedence as the next peeked operator.
                // We cannot reduce right associative operators, they can only be reduced
                // once we shifted all operators with the same precedence.
                for op in PRECEDENCE[index] {
                    if self.ends_with_op(*op) && !op.is_right_associative() {
                        found = true;
                        self.reduce_top(*op);
                    }
                }

                if !found {
                    break;
                }
            }

            break;
        }
    }

    fn ends_with_op(&self, op: Op) -> bool {
        let len = self.stack.len();

        let Some(ExprOrToken::Token(token)) = self.stack.get(len.wrapping_sub(2)) else {
            return false;
        };

        let expect = op.operator();

        **token == expect
    }

    fn ends_with_ident(&self) -> bool {
        match self.stack.last() {
            Some(ExprOrToken::Expr(_)) => true,
            Some(ExprOrToken::Token(TokenKind::Ident(_))) => true,
            _ => false,
        }
    }

    fn reduce_top(&mut self, op: Op) {
        let expr = match op {
            Op::Neg | Op::LogicalNot => {
                let value = Box::new(self.stack.pop().unwrap().into_expr());
                let op = self.stack.pop().unwrap();

                match op {
                    ExprOrToken::Token(TokenKind::Minus) => Expr::Neg { value },
                    ExprOrToken::Token(TokenKind::Bang) => Expr::LogicalNot { value },
                    _ => todo!(),
                }
            }
            _ => {
                let rhs = Box::new(self.stack.pop().unwrap().into_expr());
                let op = self.stack.pop().unwrap();
                let lhs = Box::new(self.stack.pop().unwrap().into_expr());

                match op {
                    ExprOrToken::Token(TokenKind::Plus) => Expr::Add { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Minus) => Expr::Sub { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Star) => Expr::Mul { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Slash) => Expr::Div { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Percent) => Expr::Rem { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Dot) => Expr::Deref { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Eq) => Expr::Assign { src: rhs, dst: lhs },
                    ExprOrToken::Token(TokenKind::PlusEq) => Expr::AddAssign { src: rhs, dst: lhs },
                    ExprOrToken::Token(TokenKind::MinusEq) => {
                        Expr::SubAssign { src: rhs, dst: lhs }
                    }
                    ExprOrToken::Token(TokenKind::StarEq) => Expr::MulAssign { src: rhs, dst: lhs },
                    ExprOrToken::Token(TokenKind::SlashEq) => {
                        Expr::DivAssign { src: rhs, dst: lhs }
                    }
                    ExprOrToken::Token(TokenKind::PercentEq) => {
                        Expr::RemAssign { src: rhs, dst: lhs }
                    }
                    ExprOrToken::Token(TokenKind::LogicalAnd) => Expr::LogicalAnd { lhs, rhs },
                    ExprOrToken::Token(TokenKind::LogicalOr) => Expr::LogicalOr { lhs, rhs },
                    ExprOrToken::Token(TokenKind::EqEq) => Expr::CmpEq { lhs, rhs },
                    ExprOrToken::Token(TokenKind::BangEq) => Expr::CmpNe { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Gt) => Expr::CmpGt { lhs, rhs },
                    ExprOrToken::Token(TokenKind::GtEq) => Expr::CmpGe { lhs, rhs },
                    ExprOrToken::Token(TokenKind::Lt) => Expr::CmpLt { lhs, rhs },
                    ExprOrToken::Token(TokenKind::LtEq) => Expr::CmpLe { lhs, rhs },
                    x => todo!("{:?}", x),
                }
            }
        };

        self.stack.push(ExprOrToken::Expr(expr));
    }

    fn finalize(mut self) -> Expr {
        self.reduce(&TokenKind::Eol);

        if let Some(s) = self.stack.pop_if(|e| {
            matches!(
                e,
                ExprOrToken::Token(TokenKind::Ident(_) | TokenKind::Literal(_))
            )
        }) {
            self.stack.push(ExprOrToken::Expr(s.into_expr()));
        }

        match self.stack.pop().unwrap() {
            ExprOrToken::Expr(expr) => expr,
            _ => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum Op {
    Neg,
    LogicalNot,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    LogicalAnd,
    LogicalOr,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    Deref,
    CmpEq,
    CmpNe,
    CmpGt,
    CmpGe,
    CmpLt,
    CmpLe,
}

impl Op {
    fn operator(&self) -> TokenKind<'static> {
        match self {
            Self::Neg => TokenKind::Minus,
            Self::LogicalNot => TokenKind::Bang,
            Self::Add => TokenKind::Plus,
            Self::Sub => TokenKind::Minus,
            Self::Mul => TokenKind::Star,
            Self::Div => TokenKind::Slash,
            Self::Rem => TokenKind::Percent,
            Self::LogicalAnd => TokenKind::LogicalAnd,
            Self::LogicalOr => TokenKind::LogicalOr,
            Self::Assign => TokenKind::Eq,
            Self::AddAssign => TokenKind::PlusEq,
            Self::SubAssign => TokenKind::MinusEq,
            Self::MulAssign => TokenKind::StarEq,
            Self::DivAssign => TokenKind::SlashEq,
            Self::RemAssign => TokenKind::PercentEq,
            Self::Deref => TokenKind::Dot,
            Self::CmpEq => TokenKind::EqEq,
            Self::CmpNe => TokenKind::BangEq,
            Self::CmpGt => TokenKind::Gt,
            Self::CmpGe => TokenKind::GtEq,
            Self::CmpLt => TokenKind::Lt,
            Self::CmpLe => TokenKind::LtEq,
        }
    }

    fn is_right_associative(&self) -> bool {
        matches!(
            self,
            Self::Assign
                | Self::AddAssign
                | Self::SubAssign
                | Self::MulAssign
                | Self::DivAssign
                | Self::RemAssign
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Expr, Type};
    use crate::lexer::{Token, TokenKind};
    use crate::parser::Parser;

    fn token(kind: TokenKind) -> Token {
        Token { span: 0..0, kind }
    }

    #[test]
    fn parse_unary() {
        let tokens = [
            token(TokenKind::Minus),
            token(TokenKind::Ident("x")),
            token(TokenKind::Eol),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Neg {
                value: Box::new(Expr::Ident {
                    ident: "x".to_owned()
                })
            }
        );
    }

    #[test]
    fn parse_unary_precedence() {
        let tokens = [
            token(TokenKind::Ident("x")),
            token(TokenKind::Minus),
            token(TokenKind::Minus),
            token(TokenKind::Ident("y")),
            token(TokenKind::Eol),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Sub {
                lhs: Box::new(Expr::Ident {
                    ident: "x".to_owned()
                }),
                rhs: Box::new(Expr::Neg {
                    value: Box::new(Expr::Ident {
                        ident: "y".to_owned()
                    })
                })
            }
        );
    }

    #[test]
    fn parse_right_associative() {
        let tokens = [
            token(TokenKind::Ident("x")),
            token(TokenKind::Eq),
            token(TokenKind::Ident("y")),
            token(TokenKind::Eq),
            token(TokenKind::Ident("z")),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Assign {
                src: Box::new(Expr::Assign {
                    src: Box::new(Expr::Ident {
                        ident: "z".to_owned()
                    }),
                    dst: Box::new(Expr::Ident {
                        ident: "y".to_owned()
                    })
                }),
                dst: Box::new(Expr::Ident {
                    ident: "x".to_owned()
                })
            }
        );
    }

    #[test]
    fn parse_new() {
        let tokens = [
            token(TokenKind::Ident("new")),
            token(TokenKind::Ident("Type")),
            token(TokenKind::Eol),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::New {
                ty: Type::Ty("Type".to_owned())
            }
        );
    }

    #[test]
    fn parse_new_array() {
        let tokens = [
            token(TokenKind::Ident("new")),
            token(TokenKind::Ident("Type")),
            token(TokenKind::OpenBracket),
            token(TokenKind::Ident("x")),
            token(TokenKind::Plus),
            token(TokenKind::Ident("y")),
            token(TokenKind::CloseBracket),
            token(TokenKind::Eol),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::NewArray {
                ty: Type::Ty("Type".to_owned()),
                len: Box::new(Expr::Add {
                    lhs: Box::new(Expr::Ident {
                        ident: "x".to_owned()
                    }),
                    rhs: Box::new(Expr::Ident {
                        ident: "y".to_owned()
                    })
                })
            }
        );
    }

    #[test]
    fn parse_array_index() {
        let tokens = [
            token(TokenKind::Ident("arr")),
            token(TokenKind::OpenBracket),
            token(TokenKind::Ident("x")),
            token(TokenKind::Plus),
            token(TokenKind::Ident("y")),
            token(TokenKind::CloseBracket),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Index {
                array: Box::new(Expr::Ident {
                    ident: "arr".to_owned()
                }),
                index: Box::new(Expr::Add {
                    lhs: Box::new(Expr::Ident {
                        ident: "x".to_owned()
                    }),
                    rhs: Box::new(Expr::Ident {
                        ident: "y".to_owned()
                    })
                })
            }
        );
    }

    #[test]
    fn parse_expr_until_comma() {
        let tokens = [
            token(TokenKind::Ident("x")),
            token(TokenKind::OpenParen),
            token(TokenKind::Ident("y")),
            token(TokenKind::Comma),
            token(TokenKind::CloseParen),
            token(TokenKind::Comma),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Call {
                callee: Box::new(Expr::Ident {
                    ident: "x".to_owned()
                }),
                args: vec![Expr::Ident {
                    ident: "y".to_owned()
                }]
            }
        );
        assert_eq!(parser.tokens[0], token(TokenKind::Comma));
    }

    #[test]
    fn parse_deref_call() {
        let tokens = [
            token(TokenKind::Ident("x")),
            token(TokenKind::Dot),
            token(TokenKind::Ident("y")),
            token(TokenKind::OpenParen),
            token(TokenKind::CloseParen),
            token(TokenKind::Eol),
        ];
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr().unwrap();

        assert_eq!(
            expr,
            Expr::Call {
                callee: Box::new(Expr::Deref {
                    lhs: Box::new(Expr::Ident {
                        ident: "x".to_owned()
                    }),
                    rhs: Box::new(Expr::Ident {
                        ident: "y".to_owned()
                    }),
                }),
                args: Vec::new()
            }
        );
    }
}
