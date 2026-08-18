use crate::lexer::Literal;

pub struct Ast {
    pub script_name: String,
    pub extends: String,
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
    pub ty: Type,
    pub value: Option<Expr>,
    pub is_auto: bool,
    pub is_const: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Ty(String),
    Array(Box<Type>),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut t = self;
        let mut array_depth = 0;
        loop {
            match t {
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
    pub return_ty: Option<Type>,
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
    Ident { ident: String },
    Literal(Literal<'static>),
    Deref { lhs: Box<Expr>, rhs: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Neg { value: Box<Expr> },
    Add { lhs: Box<Expr>, rhs: Box<Expr> },
    Sub { lhs: Box<Expr>, rhs: Box<Expr> },
    Mul { lhs: Box<Expr>, rhs: Box<Expr> },
    Div { lhs: Box<Expr>, rhs: Box<Expr> },
    Rem { lhs: Box<Expr>, rhs: Box<Expr> },
    Assign { src: Box<Expr>, dst: Box<Expr> },
    AddAssign { src: Box<Expr>, dst: Box<Expr> },
    SubAssign { src: Box<Expr>, dst: Box<Expr> },
    MulAssign { src: Box<Expr>, dst: Box<Expr> },
    DivAssign { src: Box<Expr>, dst: Box<Expr> },
    RemAssign { src: Box<Expr>, dst: Box<Expr> },
    LogicalAnd { lhs: Box<Expr>, rhs: Box<Expr> },
    LogicalOr { lhs: Box<Expr>, rhs: Box<Expr> },
    LogicalNot { value: Box<Expr> },
    CmpEq { lhs: Box<Expr>, rhs: Box<Expr> },
    CmpNe { lhs: Box<Expr>, rhs: Box<Expr> },
    CmpGt { lhs: Box<Expr>, rhs: Box<Expr> },
    CmpGe { lhs: Box<Expr>, rhs: Box<Expr> },
    CmpLt { lhs: Box<Expr>, rhs: Box<Expr> },
    CmpLe { lhs: Box<Expr>, rhs: Box<Expr> },
    NewArray { ty: Type, len: Box<Expr> },
    New { ty: Type },
    Index { array: Box<Expr>, index: Box<Expr> },
}

#[derive(Clone, Debug)]
pub struct Binop {
    pub lhs: Expr,
    pub rhs: Expr,
}

#[derive(Clone, Debug)]
pub struct Assign {
    pub src: Expr,
    pub dst: Expr,
}

#[derive(Clone, Debug)]
pub enum Statement {
    Define(Define),
    Expr(Expr),
    Return(Option<Expr>),
    If {
        then_blocks: Vec<ConditionBlock>,
        else_block: Vec<Statement>,
    },
    While {
        condition: Expr,
        then_block: Vec<Statement>,
    },
}

#[derive(Clone, Debug)]
pub struct ConditionBlock {
    pub condition: Expr,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct Define {
    pub ty: Type,
    pub name: String,
    pub value: Option<Expr>,
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
