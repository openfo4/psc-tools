use std::ops::{Deref, Range};

use crate::lexer::Literal;

#[derive(Clone, Debug)]
pub struct Ast {
    pub script_name: String,
    pub extends: Option<String>,
    pub property_groups: Vec<PropertyGroup>,
    pub functions: Vec<Function>,
    pub properties: Vec<Property>,
    pub globals: Vec<Define>,
    pub structs: Vec<Struct>,
    pub states: Vec<State>,
}

#[derive(Clone, Debug)]
pub struct PropertyGroup {
    pub name: String,
    pub properties: Vec<Property>,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub ty: Spanned<Type>,
    pub value: Option<Expr>,
    pub is_auto: bool,
    pub is_const: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    None,
    Bool,
    Int,
    Float,
    String,
    Ty(String),
    Array(Box<Type>),
}

impl Type {
    pub fn new(s: &str) -> Self {
        if s.eq_ignore_ascii_case("none") {
            Self::None
        } else if s.eq_ignore_ascii_case("bool") {
            Self::Bool
        } else if s.eq_ignore_ascii_case("int") {
            Self::Int
        } else if s.eq_ignore_ascii_case("float") {
            Self::Float
        } else if s.eq_ignore_ascii_case("string") {
            Self::String
        } else {
            Self::Ty(s.to_owned())
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut t = self;
        let mut array_depth = 0;
        loop {
            match t {
                Self::None => {
                    write!(f, "none")?;
                    break;
                }
                Self::Bool => {
                    write!(f, "bool")?;
                    break;
                }
                Self::Int => {
                    write!(f, "int")?;
                    break;
                }
                Self::Float => {
                    write!(f, "float")?;
                    break;
                }
                Self::String => {
                    write!(f, "string")?;
                    break;
                }
                Self::Ty(name) => {
                    write!(f, "{}", name)?;
                    break;
                }
                Self::Array(next) => {
                    t = next;
                    array_depth += 1;
                }
            }
        }

        for _ in 0..array_depth {
            write!(f, "[]")?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Function {
    pub script: Option<String>,
    pub name: String,
    pub params: Vec<Define>,
    pub statements: Vec<Statement>,
    pub return_ty: Option<Spanned<Type>>,
    pub is_global: bool,
    pub is_native: bool,
    pub is_debug_only: bool,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Ident {
        ident: String,
    },
    Literal(Literal<'static>),
    Deref {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Call {
        callee: Box<Spanned<Expr>>,
        args: Vec<Spanned<Expr>>,
    },
    Neg {
        value: Box<Spanned<Expr>>,
    },
    Add {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Sub {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Mul {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Div {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Rem {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    Assign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    AddAssign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    SubAssign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    MulAssign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    DivAssign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    RemAssign {
        src: Box<Spanned<Expr>>,
        dst: Box<Spanned<Expr>>,
    },
    LogicalAnd {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    LogicalOr {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    LogicalNot {
        value: Box<Spanned<Expr>>,
    },
    CmpEq {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    CmpNe {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    CmpGt {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    CmpGe {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    CmpLt {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    CmpLe {
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    NewArray {
        ty: Spanned<Type>,
        len: Box<Spanned<Expr>>,
    },
    New {
        ty: Spanned<Type>,
    },
    Index {
        array: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    Cast {
        lhs: Box<Spanned<Expr>>,
        rhs: Spanned<Type>,
    },
    Is {
        lhs: Box<Spanned<Expr>>,
        rhs: Spanned<Type>,
    },
}

#[derive(Clone, Debug)]
pub struct Assign {
    pub src: Spanned<Expr>,
    pub dst: Spanned<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Define(Define),
    Expr(Spanned<Expr>),
    Return(Option<Spanned<Expr>>),
    If {
        then_blocks: Vec<ConditionBlock>,
        else_block: Vec<Statement>,
    },
    While {
        condition: Spanned<Expr>,
        then_block: Vec<Statement>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConditionBlock {
    pub condition: Spanned<Expr>,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Define {
    pub ty: Spanned<Type>,
    pub name: Spanned<String>,
    pub value: Option<Spanned<Expr>>,
}

#[derive(Clone, Debug)]
pub struct Struct {
    pub name: String,
    pub members: Vec<Define>,
}

#[derive(Clone, Debug)]
pub struct State {
    pub name: String,
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub span: Range<usize>,
    pub value: T,
}

impl<T> Spanned<T> {
    pub const fn new(span: Range<usize>, value: T) -> Self {
        Self { span, value }
    }

    pub const fn erased(value: T) -> Self {
        Self { span: 0..0, value }
    }

    pub fn map<F, U>(self, f: F) -> Spanned<U>
    where
        F: FnOnce(T) -> U,
    {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
