mod expr;

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::ops::Range;
use std::os::linux::raw::stat;
use std::str::FromStr;

use clap::builder::Str;

use crate::ast::{
    Assign, Ast, ConditionBlock, Define, Expr, Function, Property, PropertyGroup, State, Statement,
    Struct, Type, Variable,
};
use crate::lexer::{Literal, Token, TokenKind};

#[derive(Clone, Debug)]
pub struct Error<'a> {
    pub span: Range<usize>,
    pub kind: ErrorKind<'a>,
}

#[derive(Clone, Debug)]
pub enum ErrorKind<'a> {
    UnexpectedToken {
        expected: &'static [ParseToken<'static>],
        found: &'a Token<'a>,
    },
}

impl Display for ErrorKind<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedToken { expected, found } => {
                write!(f, "expected one of: {:?}, found {:?}", expected, found)
            }
        }
    }
}

pub struct Parser<'a> {
    tokens: &'a [Token<'a>],
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token<'a>]) -> Self {
        Self { tokens }
    }

    pub fn parse(&mut self) -> Result<Ast, Error<'a>> {
        let header = self.parse_header()?;

        let mut property_groups = Vec::new();
        let mut functions = Vec::new();
        let mut properties = Vec::new();
        let mut globals = Vec::new();
        let mut structs = Vec::new();
        let mut states = Vec::new();

        while let Some(token) = self.peek() {
            match &token.kind {
                _ if token.keyword() == Some(Keyword::Group) => {
                    let gp = self.parse_property_group()?;
                    property_groups.push(gp);
                }
                _ if token.keyword() == Some(Keyword::Struct) => {
                    let st = self.parse_struct()?;
                    structs.push(st);
                }
                _ if token.keyword() == Some(Keyword::State) => {
                    let state = self.parse_state()?;
                    states.push(state);
                }
                _ if matches!(token.keyword(), Some(Keyword::Function | Keyword::Event)) => {
                    let func = self.parse_function()?;
                    functions.push(func);
                }
                TokenKind::Eol => {
                    self.consume();
                }
                TokenKind::Ident(_) => {
                    // Standalone property
                    match self.peek_after_type() {
                        Some(token) if token.keyword() == Some(Keyword::Property) => {
                            let prop = self.parse_property()?;
                            properties.push(prop);
                        }
                        Some(token)
                            if matches!(
                                token.keyword(),
                                Some(Keyword::Function | Keyword::Event)
                            ) =>
                        {
                            let func = self.parse_function()?;
                            functions.push(func);
                        }
                        _ => {
                            let global = self.parse_define_stmt()?;
                            globals.push(global);
                        }
                    }
                }
                _ => {
                    todo!()
                }
            }

            // dbg!(&self.tokens);
            // match self.consume_next(&[
            //     ParseToken::Keyword(Keyword::Group),
            //     ParseToken::Keyword(Keyword::Function),
            //     ParseToken::Keyword(Keyword::Event),
            //     ParseToken::Ident,
            //     ParseToken::Token(&TokenKind::Eol),
            // ])? {
            //     ParseToken::Token(TokenKind::Eol) => continue,
            //     ParseToken::Keyword(Keyword::Group) => {
            //         let gp = self.parse_property_group()?;
            //         property_groups.push(gp);
            //     }
            //     ParseToken::Keyword(Keyword::Function) => {
            //         let f = self.parse_function(&[ParseToken::Keyword(Keyword::EndFunction)])?;
            //         functions.push(f);
            //     }
            //     ParseToken::Keyword(Keyword::Event) => {
            //         let f = self.parse_function(&[ParseToken::Keyword(Keyword::EndEvent)])?;
            //         functions.push(f);
            //     }
            //     ParseToken::Token(TokenKind::Ident(ty)) => {
            //         // Standalone property
            //         if self
            //             .peek()
            //             .is_some_and(|token| token.keyword() == Some(Keyword::Property))
            //         {
            //             self.consume();
            //         }
            //     }
            //     _ => unreachable!(),
            // }
        }

        Ok(Ast {
            script_name: header.script_name,
            extends: header.extends,
            property_groups: Vec::new(),
            functions,
            properties,
            globals,
            structs,
            states,
        })
    }

    fn parse_header(&mut self) -> Result<Header, Error<'a>> {
        self.consume_next(&[ParseToken::Keyword(Keyword::ScriptName)])?;
        let name = self.consume_ident()?;

        self.consume_next(&[ParseToken::Keyword(Keyword::Extends)])?;
        let parent = self.consume_ident()?;

        while self
            .peek()
            .is_some_and(|token| token.kind != TokenKind::Eol)
        {
            match self.consume_next(&[
                ParseToken::Keyword(Keyword::Const),
                ParseToken::Keyword(Keyword::Default),
                ParseToken::Keyword(Keyword::Conditional),
            ])? {
                ParseToken::Keyword(Keyword::Const) => {}
                ParseToken::Keyword(Keyword::Default) => {}
                ParseToken::Keyword(Keyword::Conditional) => {}
                _ => unreachable!(),
            }
        }

        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        Ok(Header {
            script_name: name.to_owned(),
            extends: parent.to_owned(),
        })
    }

    fn parse_state(&mut self) -> Result<State, Error<'a>> {
        self.consume_next(&[ParseToken::Keyword(Keyword::State)])?;
        let name = self.consume_ident()?.to_owned();
        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        let mut functions = Vec::new();
        while self
            .peek()
            .is_some_and(|tok| tok.keyword() != Some(Keyword::EndState))
        {
            let func = self.parse_function()?;
            functions.push(func);
        }

        self.consume_next(&[ParseToken::Keyword(Keyword::EndState)])?;

        Ok(State { name, functions })
    }

    fn parse_property_group(&mut self) -> Result<PropertyGroup, Error<'a>> {
        self.consume_next(&[ParseToken::Keyword(Keyword::Group)])?;

        let name = self.consume_ident()?;

        // while self
        //     .peek()
        //     .is_some_and(|token| token.kind != TokenKind::Eol)
        // {
        //     match self.consume_next(&[ParseToken::Keyword(Keyword::Optional)])? {
        //         ParseToken::Keyword(Keyword::Optional) => {}
        //         _ => unreachable!(),
        //     }
        // }

        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        let mut properties = Vec::new();
        while self
            .peek()
            .is_some_and(|token| token.keyword() != Some(Keyword::EndGroup))
        {
            let prop = self.parse_property()?;
            properties.push(prop);
        }

        self.consume_next(&[ParseToken::Keyword(Keyword::EndGroup)])?;

        Ok(PropertyGroup {
            name: name.to_owned(),
            properties,
        })
    }

    fn parse_property(&mut self) -> Result<Property, Error<'a>> {
        let ty = self.parse_type()?;
        self.consume_next(&[ParseToken::Keyword(Keyword::Property)])?;
        let name = self.consume_ident()?;

        let mut value = None;
        if self.peek().is_some_and(|token| token.kind == TokenKind::Eq) {
            self.consume();

            value = Some(self.parse_ident_or_literal()?);
        }

        let mut is_auto = false;
        let mut is_const = false;

        while self
            .peek()
            .is_some_and(|token| token.kind != TokenKind::Eol)
        {
            match self.consume_next(&[
                ParseToken::Keyword(Keyword::Auto),
                ParseToken::Keyword(Keyword::Const),
                ParseToken::Keyword(Keyword::Mandatory),
                ParseToken::Keyword(Keyword::Hidden),
                ParseToken::Keyword(Keyword::Conditional),
            ])? {
                ParseToken::Keyword(Keyword::Auto) => {
                    is_auto = true;
                }
                ParseToken::Keyword(Keyword::Const) => {
                    is_const = true;
                }
                ParseToken::Keyword(Keyword::Mandatory) => {}
                ParseToken::Keyword(Keyword::Hidden) => {}
                ParseToken::Keyword(Keyword::Conditional) => {}
                _ => unreachable!(),
            }
        }

        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        Ok(Property {
            name: name.to_owned(),
            ty,
            value,
            is_auto,
            is_const,
        })
    }

    fn parse_struct(&mut self) -> Result<Struct, Error<'a>> {
        self.consume_next(&[ParseToken::Keyword(Keyword::Struct)])?;

        let name = self.consume_ident()?.to_owned();
        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        let mut members = Vec::new();
        while self
            .peek()
            .is_some_and(|tok| tok.keyword() != Some(Keyword::EndStruct))
        {
            let member = self.parse_define_stmt()?;
            members.push(member);
        }

        self.consume_next(&[ParseToken::Keyword(Keyword::EndStruct)])?;

        Ok(Struct { name, members })
    }

    fn parse_type(&mut self) -> Result<Type, Error<'a>> {
        let ty = self.consume_ident()?;
        let mut ty = Type::Ty(ty.to_owned());

        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::OpenBracket)
        {
            self.consume();
            self.consume_next(&[ParseToken::Token(&TokenKind::CloseBracket)])?;
            ty = Type::Array(Box::new(ty));
        }

        Ok(ty)
    }

    fn parse_inline_type(&mut self) -> Result<Type, Error<'a>> {
        let ty = self.consume_ident()?;
        let mut ty = Type::Ty(ty.to_owned());

        while self
            .tokens
            .get(0)
            .is_some_and(|tok| tok.kind == TokenKind::OpenBracket)
            && self
                .tokens
                .get(1)
                .is_some_and(|tok| tok.kind == TokenKind::CloseBracket)
        {
            self.consume();
            self.consume();
            ty = Type::Array(Box::new(ty));
        }

        Ok(ty)
    }

    fn parse_function(&mut self) -> Result<Function, Error<'a>> {
        let mut return_ty = None;
        if let Some(token) = self.peek()
            && !matches!(token.keyword(), Some(Keyword::Function | Keyword::Event))
        {
            return_ty = Some(self.parse_type()?);
        }

        let terminator = match self.consume_next(&[
            ParseToken::Keyword(Keyword::Event),
            ParseToken::Keyword(Keyword::Function),
        ])? {
            ParseToken::Keyword(Keyword::Function) => &[ParseToken::Keyword(Keyword::EndFunction)],
            ParseToken::Keyword(Keyword::Event) => &[ParseToken::Keyword(Keyword::EndEvent)],
            _ => unreachable!(),
        };

        let mut script = None;
        let mut name = self.consume_ident()?.to_owned();

        if self.peek().is_some_and(|tok| tok.kind == TokenKind::Dot) {
            self.consume();
            script = Some(name);
            name = self.consume_ident()?.to_owned();
        }

        self.consume_next(&[ParseToken::Token(&TokenKind::OpenParen)])?;

        let mut params = Vec::new();
        while self
            .peek()
            .is_some_and(|token| token.kind != TokenKind::CloseParen)
        {
            let ty = self.parse_type()?;
            let name = self.consume_ident()?.to_owned();
            let mut value = None;

            // Default parameter value.
            if self.peek().is_some_and(|tok| tok.kind == TokenKind::Eq) {
                self.consume();
                value = Some(self.parse_ident_or_literal()?);
            }

            params.push(Define { ty, name, value });

            if self.peek().is_some_and(|tok| tok.kind != TokenKind::Comma) {
                break;
            }

            self.consume();
        }

        self.consume_next(&[ParseToken::Token(&TokenKind::CloseParen)])?;

        let mut is_global = false;
        let mut is_native = false;
        let mut is_debug_only = false;
        while self.peek().is_some_and(|tok| tok.kind != TokenKind::Eol) {
            match self.consume_next(&[
                ParseToken::Keyword(Keyword::Global),
                ParseToken::Keyword(Keyword::Native),
                ParseToken::Keyword(Keyword::DebugOnly),
            ])? {
                ParseToken::Keyword(Keyword::Global) => is_global = true,
                ParseToken::Keyword(Keyword::Native) => is_native = true,
                ParseToken::Keyword(Keyword::DebugOnly) => is_debug_only = true,
                _ => unreachable!(),
            }
        }

        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

        let mut statements = Vec::new();
        while self.peek().is_some_and(|token| {
            !matches!(
                token.keyword(),
                Some(Keyword::EndFunction | Keyword::EndEvent)
            )
        }) {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }

        self.consume_next(terminator)?;
        self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;
        Ok(Function {
            script,
            name,
            params,
            statements,
            return_ty,
            is_global,
            is_native,
            is_debug_only,
        })
    }

    fn parse_define_stmt(&mut self) -> Result<Define, Error<'a>> {
        let ty = self.parse_type()?;
        let name = self.consume_ident()?;

        let mut value = None;
        if self.peek().is_some_and(|tok| tok.kind == TokenKind::Eq) {
            self.consume();
            value = Some(self.parse_expr()?);
        } else {
            self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;
        }

        Ok(Define {
            ty,
            name: name.to_owned(),
            value,
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, Error<'a>> {
        match self.peek() {
            Some(token) if token.keyword() == Some(Keyword::If) => self.parse_if(),
            Some(token) if token.keyword() == Some(Keyword::While) => {
                self.consume();

                let condition = self.parse_expr()?;
                let mut then_block = Vec::new();

                while self
                    .peek()
                    .is_some_and(|token| token.keyword() != Some(Keyword::EndWhile))
                {
                    let stmt = self.parse_statement()?;
                    then_block.push(stmt);
                }

                Ok(Statement::While {
                    condition,
                    then_block,
                })
            }
            Some(token) if token.keyword() == Some(Keyword::Return) => {
                self.consume();

                let mut expr = None;
                if self.peek().is_some_and(|tok| tok.kind != TokenKind::Eol) {
                    expr = Some(self.parse_expr()?);
                } else {
                    self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;
                }

                Ok(Statement::Return(expr))
            }
            _ => {
                // There are three forms of statements:
                // 1. <Type> <Ident> [ = <Expr> ]
                // 2. <LValueExpr> <AssignOp> <Expr>
                // 3. <Expr> (typically in the form of a single function call)

                match self.tokens.first().map(|token| &token.kind) {
                    // Form 1, 2 or 3.
                    Some(TokenKind::Ident(ident)) => {
                        // '[]' indicates an array type, only valid in form 1.
                        if matches!(
                            self.tokens.get(1).map(|token| &token.kind),
                            Some(TokenKind::OpenBracket)
                        ) && matches!(
                            self.tokens.get(2).map(|token| &token.kind),
                            Some(TokenKind::CloseBracket)
                        ) {
                            let ty = self.parse_type()?;
                            let ident = self.consume_ident()?;
                            let value = match self.consume_next(&[
                                ParseToken::Token(&TokenKind::Eq),
                                ParseToken::Token(&TokenKind::Eol),
                            ])? {
                                ParseToken::Token(TokenKind::Eq) => {
                                    let expr = self.parse_expr()?;
                                    Some(expr)
                                }
                                ParseToken::Token(TokenKind::Eol) => None,
                                _ => unreachable!(),
                            };

                            return Ok(Statement::Define(Define {
                                ty,
                                name: ident.to_owned(),
                                value,
                            }));
                        }

                        let expr = self.parse_expr()?;
                        return Ok(Statement::Expr(expr));

                        // // Form 2
                        // match self.tokens.get(1).map(|token| &token.kind) {
                        //     Some(TokenKind::Eq) => {
                        //         self.consume();
                        //         let expr = self.parse_expr()?;
                        //         return Statement::Assign(Assign { src: (), dst: () });
                        //     }
                        //     Some(TokenKind::PlusEq) => {
                        //         self.consume();
                        //     }
                        //     Some(TokenKind::MinusEq) => {
                        //         self.consume();
                        //     }
                        //     Some(TokenKind::StarEq) => {
                        //         self.consume();
                        //     }
                        //     Some(TokenKind::SlashEq) => {
                        //         self.consume();
                        //     }
                        //     Some(TokenKind::PercentEq) => {
                        //         self.consume();
                        //     }
                        //     _ => {}
                        // }
                    }
                    // Always form 3.
                    _ => {
                        let expr = self.parse_expr()?;
                        Ok(Statement::Expr(expr))
                    }
                }

                // let ty = self.parse_type()?;

                // let mut name = String::new();
                // if self.peek().is_some_and(|token| {
                //     matches!(
                //         token.kind,
                //         TokenKind::Eq
                //             | TokenKind::PlusEq
                //             | TokenKind::MinusEq
                //             | TokenKind::StarEq
                //             | TokenKind::SlashEq
                //             | TokenKind::PercentEq
                //     )
                // }) {
                //     match &ty {
                //         Type::Ty(ty) => name = ty.clone(),
                //         Type::Array(_) => todo!(),
                //     }
                // }

                // if self
                //     .peek()
                //     .is_some_and(|token| token.kind == TokenKind::Eol)
                // {}

                // // match self.consume().map(|token| token.kind) {
                // //     Some(TokenKind::Eol) => Ok(Statement::Define {
                // //         ty,
                // //         name: name,
                // //         value: None,
                // //     }),
                // //     Some(TokenKind::Eq) => Ok(Statement::Assign(Assign { src })),
                // //     _ => todo!(),
                // // }
            }
        }
    }

    fn parse_if(&mut self) -> Result<Statement, Error<'a>> {
        self.consume_next(&[ParseToken::Keyword(Keyword::If)])?;
        let mut condition = self.parse_expr()?;
        let mut statements = Vec::new();

        let mut blocks = Vec::new();
        let mut else_block = Vec::new();

        loop {
            match self.peek() {
                Some(token) if token.keyword() == Some(Keyword::ElseIf) => {
                    blocks.push(ConditionBlock {
                        condition,
                        statements,
                    });

                    self.consume();
                    condition = self.parse_expr()?;
                    statements = Vec::new();

                    continue;
                }
                Some(token) if token.keyword() == Some(Keyword::Else) => {
                    self.consume();
                    self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

                    while self
                        .peek()
                        .is_some_and(|token| token.keyword() != Some(Keyword::EndIf))
                    {
                        let stmt = self.parse_statement()?;
                        else_block.push(stmt);
                    }

                    self.consume_next(&[ParseToken::Keyword(Keyword::EndIf)])?;
                    self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

                    break;
                }
                Some(token) if token.keyword() == Some(Keyword::EndIf) => {
                    blocks.push(ConditionBlock {
                        condition,
                        statements,
                    });

                    self.consume();
                    self.consume_next(&[ParseToken::Token(&TokenKind::Eol)])?;

                    break;
                }
                _ => {
                    let stmt = self.parse_statement()?;
                    statements.push(stmt);
                }
            }
        }

        Ok(Statement::If {
            then_blocks: blocks,
            else_block,
        })
    }

    fn parse_ident_or_literal(&mut self) -> Result<Expr, Error<'a>> {
        match self.consume_next(&[ParseToken::Ident, ParseToken::Literal])? {
            ParseToken::Token(TokenKind::Ident(ident)) => Ok(Expr::Ident {
                ident: ident.to_string(),
            }),
            ParseToken::Token(TokenKind::Literal(literal)) => match literal {
                Literal::NumberInt(lit) => Ok(Expr::Literal(Literal::NumberInt(*lit))),
                Literal::NumberDecimal { integer, fraction } => {
                    Ok(Expr::Literal(Literal::NumberDecimal {
                        integer: *integer,
                        fraction: *fraction,
                    }))
                }
                Literal::String(lit) => {
                    Ok(Expr::Literal(Literal::String(Cow::Owned(lit.to_string()))))
                }
            },
            _ => unreachable!(),
        }
    }

    fn peek(&self) -> Option<&'a Token<'a>> {
        self.tokens.first()
    }

    /// Returns the next token after a complete type.
    fn peek_after_type(&self) -> Option<&Token<'a>> {
        let (name, mut tokens) = self.tokens.split_first()?;
        if !matches!(name.kind, TokenKind::Ident(_)) {
            return None;
        }

        while matches!(
            tokens.first().map(|token| &token.kind),
            Some(TokenKind::OpenBracket)
        ) {
            let open_bracket = tokens.split_off_first()?;
            let close_bracket = tokens.split_off_first()?;
            if open_bracket.kind != TokenKind::OpenBracket
                || close_bracket.kind != TokenKind::CloseBracket
            {
                return None;
            }
        }

        tokens.first()
    }

    fn consume(&mut self) -> Option<&'a Token<'a>> {
        self.tokens.split_off_first()
    }

    fn consume_ident(&mut self) -> Result<&'a str, Error<'a>> {
        let token = self.tokens.split_off_first().unwrap();

        match token.kind {
            TokenKind::Ident(id) => Ok(id),
            _ => Err(Error {
                span: token.span.clone(),
                kind: ErrorKind::UnexpectedToken {
                    expected: &[ParseToken::Ident],
                    found: token,
                },
            }),
        }
    }

    fn consume_next(
        &mut self,
        tokens: &'static [ParseToken<'static>],
    ) -> Result<ParseToken<'_>, Error<'a>> {
        let token = self.tokens.split_off_first().unwrap();

        let keywords: HashSet<Keyword> = tokens
            .iter()
            .filter_map(|t| match t {
                ParseToken::Keyword(kw) => Some(*kw),
                _ => None,
            })
            .collect();

        for t in tokens {
            let matches = match t {
                ParseToken::Token(t) => **t == token.kind,
                ParseToken::Ident => matches!(token.kind, TokenKind::Ident(_)),
                ParseToken::Literal => matches!(token.kind, TokenKind::Literal(_)),
                ParseToken::Keyword(keyword) => {
                    let TokenKind::Ident(token) = token.kind else {
                        continue;
                    };

                    token
                        .parse::<Keyword>()
                        .is_ok_and(|found| found == *keyword)
                }
            };

            if matches {
                match token.kind {
                    TokenKind::Ident(ident) if let Ok(kw) = ident.parse() => {
                        if keywords.contains(&kw) {
                            return Ok(ParseToken::Keyword(kw));
                        }
                    }
                    _ => return Ok(ParseToken::Token(&token.kind)),
                }
            }
        }

        Err(Error {
            span: token.span.clone(),
            kind: ErrorKind::UnexpectedToken {
                expected: tokens,
                found: token,
            },
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Keyword {
    ScriptName,
    Extends,
    Default,
    Conditional,
    Mandatory,
    Hidden,
    Optional,
    State,
    EndState,
    Group,
    EndGroup,
    Property,
    Auto,
    Const,
    Function,
    EndFunction,
    Event,
    EndEvent,
    If,
    ElseIf,
    Else,
    EndIf,
    While,
    EndWhile,
    Return,
    New,
    Struct,
    EndStruct,
    Global,
    Native,
    DebugOnly,
}

impl FromStr for Keyword {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("ScriptName") => Ok(Self::ScriptName),
            s if s.eq_ignore_ascii_case("Extends") => Ok(Self::Extends),
            s if s.eq_ignore_ascii_case("Default") => Ok(Self::Default),
            s if s.eq_ignore_ascii_case("Conditional") => Ok(Self::Conditional),
            s if s.eq_ignore_ascii_case("Mandatory") => Ok(Self::Mandatory),
            s if s.eq_ignore_ascii_case("Hidden") => Ok(Self::Mandatory),
            s if s.eq_ignore_ascii_case("Optional") => Ok(Self::Optional),
            s if s.eq_ignore_ascii_case("State") => Ok(Self::State),
            s if s.eq_ignore_ascii_case("EndState") => Ok(Self::EndState),
            s if s.eq_ignore_ascii_case("Group") => Ok(Self::Group),
            s if s.eq_ignore_ascii_case("EndGroup") => Ok(Self::EndGroup),
            s if s.eq_ignore_ascii_case("Property") => Ok(Self::Property),
            s if s.eq_ignore_ascii_case("Auto") => Ok(Self::Auto),
            s if s.eq_ignore_ascii_case("Const") => Ok(Self::Const),
            s if s.eq_ignore_ascii_case("Function") => Ok(Self::Function),
            s if s.eq_ignore_ascii_case("EndFunction") => Ok(Self::EndFunction),
            s if s.eq_ignore_ascii_case("Event") => Ok(Self::Event),
            s if s.eq_ignore_ascii_case("EndEvent") => Ok(Self::EndEvent),
            s if s.eq_ignore_ascii_case("If") => Ok(Self::If),
            s if s.eq_ignore_ascii_case("ElseIf") => Ok(Self::ElseIf),
            s if s.eq_ignore_ascii_case("Else") => Ok(Self::Else),
            s if s.eq_ignore_ascii_case("EndIf") => Ok(Self::EndIf),
            s if s.eq_ignore_ascii_case("While") => Ok(Self::While),
            s if s.eq_ignore_ascii_case("EndWhile") => Ok(Self::EndWhile),
            s if s.eq_ignore_ascii_case("Return") => Ok(Self::Return),
            s if s.eq_ignore_ascii_case("New") => Ok(Self::New),
            s if s.eq_ignore_ascii_case("Struct") => Ok(Self::Struct),
            s if s.eq_ignore_ascii_case("EndStruct") => Ok(Self::EndStruct),
            s if s.eq_ignore_ascii_case("Global") => Ok(Self::Global),
            s if s.eq_ignore_ascii_case("Native") => Ok(Self::Native),
            s if s.eq_ignore_ascii_case("DebugOnly") => Ok(Self::DebugOnly),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
enum ParseToken<'a> {
    Token(&'a TokenKind<'a>),
    Keyword(Keyword),
    Ident,
    Literal,
}

trait IdentKeyword {
    fn keyword(&self) -> Option<Keyword>;
}

impl IdentKeyword for Token<'_> {
    fn keyword(&self) -> Option<Keyword> {
        match self.kind {
            TokenKind::Ident(ident) => ident.parse().ok(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum ExprOrType {
    Type(Type),
    Expr(Expr),
}

#[derive(Clone, Debug)]
struct Header {
    script_name: String,
    extends: String,
}
