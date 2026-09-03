use std::collections::HashMap;
use std::fmt::Display;
use std::ops::Range;

use crate::ast::{Ast, Define, Expr, Function, Spanned, Statement, Struct, Type};
use crate::lexer::Literal;
use crate::registry::Registry;

#[derive(Clone, Debug)]
pub struct Error<'a> {
    pub span: Range<usize>,
    pub kind: ErrorKind<'a>,
}

#[derive(Clone, Debug)]
pub enum ErrorKind<'a> {
    VarRedeclared {
        var: &'a str,
        declared: Range<usize>,
    },
    InvalidBinopTypes {
        op: Binop,
        lhs: Type,
        rhs: Type,
    },
    UndeclaredVar {
        var: &'a str,
    },
    UnknownFunction {
        ty: Type,
        func: &'a str,
    },
    MismatchedTypes {
        expected: Type,
        found: Type,
        defined: Option<Range<usize>>,
    },
    UnknownType {
        ty: Type,
    },
    UnknownVarOrType {
        name: &'a str,
    },
    NoPlace,
    InvalidNeg {
        ty: Type,
    },
    IndexNonArray {
        ty: Type,
    },
}

impl<'a> ErrorKind<'a> {
    pub fn error_class(&self) -> &'static str {
        match self {
            Self::VarRedeclared {
                var: _,
                declared: _,
            } => "variable redeclaration happens here",
            Self::InvalidBinopTypes {
                op: _,
                lhs: _,
                rhs: _,
            } => "",
            Self::UndeclaredVar { var: _ } => "undeclared variable used here",
            Self::UnknownFunction { ty: _, func: _ } => "unknown function used here",
            Self::MismatchedTypes {
                expected: _,
                found: _,
                defined: _,
            } => "mismatched types",
            Self::UnknownType { ty: _ } => "unknown type used here",
            Self::UnknownVarOrType { name: _ } => "unknown variable or type used here",
            Self::NoPlace => "",
            Self::InvalidNeg { ty: _ } => "",
            Self::IndexNonArray { ty: _ } => "",
        }
    }

    pub fn help(&self) -> Vec<Spanned<String>> {
        match self {
            Self::VarRedeclared { var, declared } => {
                vec![Spanned::new(
                    declared.clone(),
                    format!("previous declaration of `{var}` happens here"),
                )]
            }
            Self::InvalidBinopTypes {
                op: _,
                lhs: _,
                rhs: _,
            } => Vec::new(),
            Self::UndeclaredVar { var: _ } => Vec::new(),
            Self::UnknownFunction { ty: _, func: _ } => Vec::new(),
            Self::MismatchedTypes {
                expected,
                found: _,
                defined,
            } => {
                if let Some(defined) = defined {
                    vec![Spanned::new(
                        defined.clone(),
                        format!("type `{expected}` expected due to definition here"),
                    )]
                } else {
                    Vec::new()
                }
            }
            Self::UnknownType { ty: _ } => Vec::new(),
            Self::UnknownVarOrType { name: _ } => Vec::new(),
            Self::NoPlace => Vec::new(),
            Self::InvalidNeg { ty: _ } => Vec::new(),
            Self::IndexNonArray { ty: _ } => Vec::new(),
        }
    }

    pub fn notes(&self) -> Vec<String> {
        match self {
            Self::InvalidNeg { ty: _ } => {
                vec!["negation is only implemented for `int` and `float` types".to_owned()]
            }
            _ => Vec::new(),
        }
    }
}

impl Display for ErrorKind<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VarRedeclared { var, declared: _ } => {
                write!(f, "variable `{var}` cannot be redeclared")
            }
            Self::InvalidBinopTypes { op, lhs, rhs } => match op {
                Binop::Add => {
                    write!(f, "cannot add {lhs} to {rhs}")
                }
                Binop::Sub => {
                    write!(f, "cannot subtract {lhs} from {rhs}")
                }
                Binop::Mul => {
                    write!(f, "cannot multiply {lhs} by {rhs}")
                }
                Binop::Div => {
                    write!(f, "cannot divide {lhs} by {rhs}")
                }
            },
            Self::UndeclaredVar { var } => {
                write!(f, "cannot find variable `{var}` in scope")
            }
            Self::UnknownFunction { ty: class, func } => {
                write!(f, "no function named `{}` for type `{}`", func, class)
            }
            Self::MismatchedTypes {
                expected,
                found,
                defined: _,
            } => {
                write!(
                    f,
                    "mismatched types: expected `{expected}`, found `{found}`"
                )
            }
            Self::UnknownType { ty } => {
                write!(f, "cannot find type `{ty}` in scope")
            }
            Self::UnknownVarOrType { name } => {
                write!(f, "cannot find variable or type `{name}` in scope")
            }
            Self::NoPlace => {
                write!(f, "right-hand side must be a place expression")
            }
            Self::InvalidNeg { ty } => {
                write!(f, "cannot negate `{ty}`")
            }
            Self::IndexNonArray { ty } => {
                write!(f, "mismatched types: expected `T[]`, found `{ty}`")
            }
        }
    }
}

pub fn typecheck<'a>(registry: &'a Registry, ast: &'a Ast) -> Result<(), Vec<Error<'a>>> {
    let mut visitor = Visitor {
        registry,
        errors: Vec::new(),
        idents: HashMap::new(),
        ast,
    };

    visitor.visit(ast);

    if visitor.errors.is_empty() {
        Ok(())
    } else {
        Err(visitor.errors)
    }
}
pub struct Visitor<'a> {
    registry: &'a Registry,
    errors: Vec<Error<'a>>,
    idents: HashMap<&'a str, Def<'a>>,
    ast: &'a Ast,
}

impl<'a> Visitor<'a> {
    fn visit(&mut self, ast: &'a Ast) {
        for def in &ast.globals {
            self.visit_define(def);
        }
        self.idents.clear();

        for st in &ast.structs {
            self.visit_struct(st);
            self.idents.clear();
        }

        for func in &ast.functions {
            self.visit_function(func);
            self.idents.clear();
        }
    }

    fn visit_struct(&mut self, st: &'a Struct) {
        for member in &st.members {
            self.visit_define(member);
        }
    }

    fn visit_function(&mut self, func: &'a Function) {
        for param in &func.params {
            if let Some(prev) = self.idents.get(&*param.name.value) {
                self.errors.push(Error {
                    span: param.name.span.clone(),
                    kind: ErrorKind::VarRedeclared {
                        var: &param.name,
                        declared: prev.name.span.clone(),
                    },
                });

                continue;
            }

            self.idents.insert(
                &param.name.value,
                Def {
                    name: &param.name,
                    ty: &param.ty,
                },
            );
        }

        self.visit_block(func, &func.statements);
    }

    fn visit_block(&mut self, func: &'a Function, stmts: &'a [Statement]) {
        for stmt in stmts {
            match stmt {
                Statement::Define(def) => {
                    self.visit_define(def);
                }
                Statement::Expr(expr) => {
                    self.visit_expr(expr).and_then(|ty| self.load_place(ty));
                }
                Statement::If {
                    then_blocks,
                    else_block,
                } => {
                    for block in then_blocks {
                        if let Some(ty) = self
                            .visit_expr(&block.condition)
                            .and_then(|ty| self.load_place(ty))
                        {
                            self.perform_assign(
                                &ty,
                                &Type::Bool,
                                block.condition.span.clone(),
                                None,
                            );
                        }

                        self.visit_block(func, &block.statements);
                    }

                    self.visit_block(func, else_block);
                }
                Statement::While {
                    condition,
                    then_block,
                } => {
                    if let Some(ty) = self
                        .visit_expr(condition)
                        .and_then(|ty| self.load_place(ty))
                    {
                        self.perform_assign(&ty, &Type::Bool, condition.span.clone(), None);
                    }

                    self.visit_block(func, then_block);
                }
                Statement::Return(expr) => {
                    let (Some(expr), Some(return_ty)) = (expr, &func.return_ty) else {
                        continue;
                    };

                    if let Some(ty) = self.visit_expr(expr).and_then(|ty| self.load_place(ty)) {
                        self.perform_assign(
                            &ty,
                            &return_ty,
                            expr.span.clone(),
                            Some(return_ty.span.clone()),
                        );
                    }
                }
            }
        }
    }

    fn visit_define(&mut self, def: &'a Define) {
        if let Some(prev) = self.idents.get(&*def.name.value) {
            self.errors.push(Error {
                span: def.name.span.clone(),
                kind: ErrorKind::VarRedeclared {
                    var: &def.name,
                    declared: prev.name.span.clone(),
                },
            });

            return;
        }

        self.idents.insert(
            &def.name,
            Def {
                name: &def.name,
                ty: &def.ty,
            },
        );

        let mut ty = &def.ty.value;
        while let Type::Array(next) = ty {
            ty = next;
        }
        if let Type::Ty(name) = ty {
            if !self.registry.classes.contains_key(name)
                && !self.registry.structs.contains_key(name)
            {
                self.errors.push(Error {
                    span: def.ty.span.clone(),
                    kind: ErrorKind::UnknownType { ty: ty.clone() },
                });

                return;
            }
        }

        let Some(value) = &def.value else {
            return;
        };

        let Some(ty) = self.visit_expr(value).and_then(|ty| self.load_place(ty)) else {
            return;
        };

        self.perform_assign(
            &ty,
            &def.ty.value,
            value.span.clone(),
            Some(def.ty.span.clone()),
        );
    }

    fn visit_expr(&mut self, expr: &'a Spanned<Expr>) -> Option<Spanned<TypeOrPlace<'a>>> {
        let ty = match &expr.value {
            Expr::Ident { ident } => {
                if let Some(def) = self.idents.get(&**ident) {
                    let span = def.ty.span.clone();
                    return Some(Spanned::new(span, TypeOrPlace::Type(def.ty.value.clone())));
                }

                Some(TypeOrPlace::Place(Place {
                    prefix: None,
                    segments: vec![Spanned::new(expr.span.clone(), ident)],
                }))
            }
            Expr::Literal(literal) => match literal {
                Literal::None => Some(TypeOrPlace::Type(Type::None)),
                Literal::Bool(_) => Some(TypeOrPlace::Type(Type::Bool)),
                Literal::NumberInt(_) => Some(TypeOrPlace::Type(Type::Int)),
                Literal::NumberDecimal(_) => Some(TypeOrPlace::Type(Type::Float)),
                Literal::String(_) => Some(TypeOrPlace::Type(Type::String)),
            },
            Expr::Add { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                let res = self.perform_coersion_binop(Binop::Add, expr.span.clone(), &lhs, &rhs)?;
                Some(TypeOrPlace::Type(res))
            }
            Expr::Sub { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                let res = self.perform_coersion_binop(Binop::Sub, expr.span.clone(), &lhs, &rhs)?;
                Some(TypeOrPlace::Type(res))
            }
            Expr::Mul { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                let res = self.perform_coersion_binop(Binop::Mul, expr.span.clone(), &lhs, &rhs)?;
                Some(TypeOrPlace::Type(res))
            }
            Expr::Div { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                let res = self.perform_coersion_binop(Binop::Div, expr.span.clone(), &lhs, &rhs)?;
                Some(TypeOrPlace::Type(res))
            }
            Expr::Rem { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                let res = self.perform_coersion_binop(Binop::Div, expr.span.clone(), &lhs, &rhs)?;
                Some(TypeOrPlace::Type(res))
            }
            Expr::Neg { value } => {
                let value = self.visit_expr(value).and_then(|ty| self.load_place(ty))?;
                if !matches!(value.value, Type::Int | Type::Float) {
                    self.errors.push(Error {
                        span: expr.span.clone(),
                        kind: ErrorKind::InvalidNeg { ty: value.value },
                    });
                    return None;
                }

                Some(TypeOrPlace::Type(value.value))
            }
            Expr::CmpEq { lhs, rhs }
            | Expr::CmpNe { lhs, rhs }
            | Expr::CmpLt { lhs, rhs }
            | Expr::CmpLe { lhs, rhs }
            | Expr::CmpGt { lhs, rhs }
            | Expr::CmpGe { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                if self.coerce_symmetric(&lhs, &rhs).is_none() {
                    self.errors.push(Error {
                        span: rhs.span,
                        kind: ErrorKind::MismatchedTypes {
                            expected: lhs.value,
                            found: rhs.value,
                            defined: None,
                        },
                    });
                }

                Some(TypeOrPlace::Type(Type::Bool))
            }
            Expr::LogicalNot { value } => {
                let value_ty = self.visit_expr(value).and_then(|ty| self.load_place(ty))?;
                self.perform_assign(&value_ty, &Type::Bool, value.span.clone(), None);
                Some(TypeOrPlace::Type(Type::Bool))
            }
            Expr::LogicalAnd { lhs, rhs } | Expr::LogicalOr { lhs, rhs } => {
                let lhs = self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                let rhs = self.visit_expr(rhs).and_then(|ty| self.load_place(ty))?;
                self.perform_assign(&lhs, &Type::Bool, lhs.span.clone(), None);
                self.perform_assign(&rhs, &Type::Bool, rhs.span.clone(), None);
                Some(TypeOrPlace::Type(Type::Bool))
            }
            Expr::Assign { src, dst }
            | Expr::AddAssign { src, dst }
            | Expr::SubAssign { src, dst }
            | Expr::MulAssign { src, dst }
            | Expr::DivAssign { src, dst }
            | Expr::RemAssign { src, dst } => {
                let src_ty = self.visit_expr(src).and_then(|ty| self.load_place(ty))?;
                let dst_ty = self.visit_expr(dst).and_then(|ty| self.load_place(ty))?;
                self.perform_assign(
                    &src_ty,
                    &dst_ty,
                    src.span.clone(),
                    Some(dst_ty.span.clone()),
                );
                Some(TypeOrPlace::Type(Type::None))
            }
            Expr::Deref { lhs, rhs } => {
                let lhs = self.visit_expr(lhs)?;
                let rhs = self.visit_expr(rhs)?;

                let mut place = match lhs.value {
                    TypeOrPlace::Type(ty) => Place {
                        prefix: Some(ty),
                        segments: Vec::new(),
                    },
                    TypeOrPlace::Place(place) => place,
                };

                let TypeOrPlace::Place(rhs) = rhs.value else {
                    self.errors.push(Error {
                        span: rhs.span.clone(),
                        kind: ErrorKind::NoPlace,
                    });
                    return None;
                };

                debug_assert!(rhs.prefix.is_none());
                place.segments.extend(rhs.segments);

                Some(TypeOrPlace::Place(place))
            }
            Expr::Index { array, index } => {
                let array = self.visit_expr(array).and_then(|ty| self.load_place(ty))?;
                let index = self.visit_expr(index).and_then(|ty| self.load_place(ty))?;
                self.perform_assign(&index, &Type::Int, index.span.clone(), None);

                let Type::Array(ty) = array.value else {
                    self.errors.push(Error {
                        span: array.span.clone(),
                        kind: ErrorKind::IndexNonArray { ty: array.value },
                    });
                    return None;
                };

                Some(TypeOrPlace::Type(*ty))
            }
            Expr::Call { callee, args } => {
                let callee = self.visit_expr(callee)?;
                let callee = self.load_place_as_func(callee.value)?;

                let return_ty = callee
                    .return_ty
                    .as_ref()
                    .map(|ty| ty.value.clone())
                    .unwrap_or(Type::None);
                Some(TypeOrPlace::Type(return_ty))
            }
            Expr::Cast { lhs, rhs } => {
                self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                if !self.check_type_exists(rhs) {
                    return None;
                }

                Some(TypeOrPlace::Type(rhs.value.clone()))
            }
            Expr::Is { lhs, rhs } => {
                self.visit_expr(lhs).and_then(|ty| self.load_place(ty))?;
                self.check_type_exists(rhs);
                Some(TypeOrPlace::Type(Type::Bool))
            }
            Expr::New { ty } => {
                if self.registry.structs.contains_key(&ty.to_string()) {
                    Some(TypeOrPlace::Type(ty.value.clone()))
                } else {
                    self.errors.push(Error {
                        span: ty.span.clone(),
                        kind: ErrorKind::UnknownType {
                            ty: ty.value.clone(),
                        },
                    });

                    None
                }
            }
            Expr::NewArray { ty, len } => {
                let len = self.visit_expr(len).and_then(|ty| self.load_place(ty))?;
                self.perform_assign(&len, &Type::Int, len.span.clone(), None);

                if self.check_type_exists(ty) {
                    Some(TypeOrPlace::Type(ty.value.clone()))
                } else {
                    None
                }
            }
        };

        ty.map(|ty| Spanned::new(expr.span.clone(), ty))
    }

    fn perform_coersion_binop(
        &mut self,
        op: Binop,
        span: Range<usize>,
        lhs: &Type,
        rhs: &Type,
    ) -> Option<Type> {
        if let Some(ty) = self.coerce_symmetric(lhs, rhs) {
            Some(ty.clone())
        } else {
            self.errors.push(Error {
                span: span.clone(),
                kind: ErrorKind::InvalidBinopTypes {
                    op,
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                },
            });
            None
        }
    }

    fn perform_assign(
        &mut self,
        src: &Type,
        dst: &Type,
        src_span: Range<usize>,
        def_site: Option<Range<usize>>,
    ) {
        if self.can_coerce(src, dst) {
            return;
        }

        self.errors.push(Error {
            span: src_span,
            kind: ErrorKind::MismatchedTypes {
                expected: dst.clone(),
                found: src.clone(),
                defined: def_site,
            },
        });
    }

    fn coerce_symmetric<'t>(&self, lhs: &'t Type, rhs: &'t Type) -> Option<&'t Type> {
        for [src, dst] in [[lhs, rhs], [rhs, lhs]] {
            if self.can_coerce(src, dst) {
                return Some(dst);
            }
        }

        None
    }

    fn can_coerce(&self, src: &Type, dst: &Type) -> bool {
        if src == dst {
            return true;
        }

        // None can be coerced to arrays, class objects and struct objects.
        if matches!(src, Type::None) && matches!(dst, Type::Array(_) | Type::Ty(_)) {
            return true;
        }

        if matches!(src, Type::Int) && matches!(dst, Type::Float) {
            return true;
        }

        // Anything can be coered into a bool or string.
        if matches!(dst, Type::Bool | Type::String) {
            return true;
        }

        // FIXME: Downcast/upcast class objects / class object arrays.

        false
    }

    fn load_place(&mut self, place: Spanned<TypeOrPlace<'a>>) -> Option<Spanned<Type>> {
        match place.value {
            TypeOrPlace::Type(ty) => Some(Spanned::new(place.span, ty)),
            TypeOrPlace::Place(place) => {
                debug_assert!(!place.segments.is_empty());
                self.load_place_segments(place.prefix.as_ref(), &place.segments)
            }
        }
    }

    fn load_place_as_func(&mut self, place: TypeOrPlace<'a>) -> Option<&'a Function> {
        match place {
            TypeOrPlace::Type(_) => todo!(),
            TypeOrPlace::Place(place) => {
                debug_assert!(!place.segments.is_empty());

                let (func, path) = place.segments.split_last().unwrap();
                let class = if !path.is_empty() {
                    let first = path.first()?;

                    match self.try_load_place_segments(place.prefix.as_ref(), path) {
                        Ok(ty) => ty.value,
                        Err(Some(seg)) if seg.span != first.span => {
                            self.errors.push(Error {
                                span: seg.span.clone(),
                                kind: ErrorKind::UndeclaredVar { var: seg.value },
                            });

                            return None;
                        }
                        _ => {
                            if !self.registry.classes.contains_key(first.value) {
                                self.errors.push(Error {
                                    span: first.span.clone(),
                                    kind: ErrorKind::UnknownVarOrType { name: first.value },
                                });

                                return None;
                            }

                            Type::Ty(first.value.to_owned())
                        }
                    }
                } else {
                    place
                        .prefix
                        .unwrap_or_else(|| Type::Ty(self.ast.script_name.clone()))
                };

                if let Some(func) = self.registry.func(&class.to_string(), func.value) {
                    return Some(func);
                }

                self.errors.push(Error {
                    span: func.span.clone(),
                    kind: ErrorKind::UnknownFunction {
                        ty: class,
                        func: func.value,
                    },
                });

                None
            }
        }
    }

    fn load_place_segments(
        &mut self,
        prefix: Option<&Type>,
        segments: &[Spanned<&'a str>],
    ) -> Option<Spanned<Type>> {
        self.try_load_place_segments(prefix, segments)
            .inspect_err(|seg| {
                if let Some(seg) = seg {
                    self.errors.push(Error {
                        span: seg.span.clone(),
                        kind: ErrorKind::UndeclaredVar { var: seg.value },
                    });
                }
            })
            .ok()
    }

    fn try_load_place_segments(
        &self,
        prefix: Option<&Type>,
        segments: &[Spanned<&'a str>],
    ) -> Result<Spanned<Type>, Option<Spanned<&'a str>>> {
        let (mut ty, skip_first) = match prefix {
            Some(ty) => (ty.clone(), false),
            None => {
                let ty;
                let skip_first;
                let seg = segments.first().ok_or(None)?;
                if let Some(def) = self.idents.get(seg.value)
                    && !seg.eq_ignore_ascii_case("self")
                {
                    ty = def.ty.value.clone();
                    skip_first = true;
                } else {
                    ty = Type::Ty(self.ast.script_name.clone());
                    skip_first = seg.eq_ignore_ascii_case("self");
                }

                (ty, skip_first)
            }
        };

        for seg in segments.iter().skip(usize::from(skip_first)) {
            if let Some(global) = self.registry.global(&ty.to_string(), seg.value) {
                ty = global.clone();
                continue;
            }

            if let Some(prop) = self.registry.property(&ty.to_string(), seg.value) {
                ty = prop.clone();
                continue;
            }

            if let Some(member) = self.registry.struct_member(&ty.to_string(), seg.value) {
                ty = member.clone();
                continue;
            }

            return Err(Some(Spanned::new(seg.span.clone(), seg.value)));
        }

        let span = segments.last().unwrap().span.clone();
        Ok(Spanned::new(span, ty))
    }

    fn check_type_exists(&mut self, ty: &'a Spanned<Type>) -> bool {
        let mut inner = &ty.value;
        while let Type::Array(next) = inner {
            inner = next;
        }

        let Type::Ty(name) = inner else {
            return true;
        };

        if self.registry.classes.contains_key(name) || self.registry.structs.contains_key(name) {
            return true;
        }

        self.errors.push(Error {
            span: ty.span.clone(),
            kind: ErrorKind::UnknownType { ty: inner.clone() },
        });
        false
    }
}

#[derive(Copy, Clone, Debug)]
struct Def<'a> {
    name: &'a Spanned<String>,
    ty: &'a Spanned<Type>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Binop {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug)]
enum TypeOrPlace<'a> {
    Type(Type),
    Place(Place<'a>),
}

#[derive(Clone, Debug)]
struct Place<'a> {
    prefix: Option<Type>,
    segments: Vec<Spanned<&'a str>>,
}
