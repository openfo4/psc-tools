use core::fmt::Write;
use std::borrow::Cow;
use std::collections::HashMap;

use pex::instruction::{
    ArrayAdd, ArrayClear, ArrayFind, ArrayFindStruct, ArrayGet, ArrayInsert, ArrayPop, ArrayRemove,
    ArraySet, Binop, CallMethod, CallParent, CallStatic, ConditionalJump, Instruction, Is, Jump,
    PropertyGet, PropertySet, StructCreate, StructGet, StructSet, Unop,
};
use pex::{
    Class, Endianess, Function, Module, NamedFunction, State, StringKey, StringTable, Value,
    VariableDefinition,
};

use crate::Error;
use crate::lexer::{Literal, Token, TokenKind};

pub struct Parser<'a> {
    module: Module,
    tokens: &'a [Token<'a>],
    strings: HashMap<Cow<'a, str>, StringKey>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token<'a>]) -> Self {
        Self {
            module: Module {
                endian: Endianess::Little,
                strings: StringTable::default(),
                classes: Vec::new(),
            },
            tokens,
            strings: HashMap::new(),
        }
    }

    pub fn parse(mut self) -> Result<Module, Error> {
        while self.peek().is_some_and(|t| t.kind != TokenKind::Eol) {
            let class = self.parse_class()?;
            self.module.classes.push(class);
        }

        Ok(self.module)
    }

    pub fn parse_class(&mut self) -> Result<Class, Error> {
        self.consume_token(&TokenKind::Ident("class"))?;
        let name = self.consume_ident()?;
        self.consume_token(&TokenKind::Colon)?;
        self.consume_token(&TokenKind::Eol)?;

        let mut default_state_funcs = Vec::new();
        let mut states = Vec::<State>::new();

        while let Some(next) = self.peek() {
            if next.kind == TokenKind::Eol {
                self.consume_token(&TokenKind::Eol)?;
                continue;
            }

            match self.peek_t::<ClassChild>()? {
                ClassChild::Class => break,
                ClassChild::State => {
                    self.consume_token(&TokenKind::Ident("state"))?;
                    let name = self.parse_string()?;
                    self.consume_token(&TokenKind::Colon)?;

                    states.push(State {
                        name,
                        functions: Vec::new(),
                    });
                }
                ClassChild::Function => {
                    let func = self.parse_function()?;

                    if let Some(state) = states.last_mut() {
                        state.functions.push(func);
                    } else {
                        default_state_funcs.push(func);
                    }
                }
            }
        }

        if !default_state_funcs.is_empty() {
            states.push(State {
                name: self.make_str(""),
                functions: default_state_funcs,
            });
        }

        Ok(Class {
            name,
            states,
            docstring: self.make_str(""),
            auto_state_name: self.make_str(""),
            is_const: false,
            parent: self.make_str(""),
            properties: Vec::new(),
            structs: Vec::new(),
            user_flags: 0,
            variables: Vec::new(),
        })
    }

    fn parse_function(&mut self) -> Result<NamedFunction, Error> {
        self.consume_token(&TokenKind::Ident("function"))?;
        let name = self.consume_ident()?;
        self.consume_token(&TokenKind::OpenParen)?;

        let mut params = Vec::new();
        while self.peek().is_some_and(|t| t.kind != TokenKind::CloseParen) {
            let name = self.consume_ident()?;
            self.consume_token(&TokenKind::Colon)?;
            let ty = self.consume_ident()?;

            params.push(VariableDefinition { name, ty });

            if self.peek().is_none_or(|t| t.kind != TokenKind::Comma) {
                break;
            }
        }

        self.consume_token(&TokenKind::CloseParen)?;

        let mut return_ty = None;
        if self.peek().is_some_and(|t| t.kind == TokenKind::Arrow) {
            self.consume_token(&TokenKind::Arrow)?;
            return_ty = Some(self.consume_ident()?);
        }

        let mut is_static = false;
        let mut is_native = false;
        while self.peek().is_some_and(|t| t.kind != TokenKind::Colon) {
            match self.consume_t::<FuncAttribute>()? {
                FuncAttribute::Static => is_static = true,
                FuncAttribute::Native => is_native = true,
            }
        }

        self.consume_token(&TokenKind::Colon)?;

        let mut locals = Vec::new();
        let mut instructions = Vec::new();
        loop {
            let Some(next) = self.peek() else {
                break;
            };

            if next.kind == TokenKind::Eol {
                self.consume_token(&TokenKind::Eol)?;
                continue;
            }

            // Identifier for a new object outside our function.
            if matches!(
                next.kind,
                TokenKind::Ident("class")
                    | TokenKind::Ident("state")
                    | TokenKind::Ident("function")
                    | TokenKind::Ident("struct")
            ) {
                break;
            }

            if next.kind == TokenKind::Ident("local") {
                let local = self.parse_local()?;
                locals.push(local);
            } else {
                let inst = self.parse_instruction()?;
                self.consume_token(&TokenKind::Eol)?;
                instructions.push(inst);
            }
        }

        let return_ty = return_ty.unwrap_or_else(|| self.make_str("none"));

        Ok(NamedFunction {
            name,
            function: Function {
                return_ty,
                docstring: self.make_str(""),
                user_flags: 0,
                is_static,
                is_native,
                params,
                locals,
                instructions,
            },
        })
    }

    fn parse_local(&mut self) -> Result<VariableDefinition, Error> {
        self.consume_token(&TokenKind::Ident("local"))?;
        let name = self.consume_ident()?;
        self.consume_token(&TokenKind::Colon)?;
        let ty = self.consume_ident()?;
        self.consume_token(&TokenKind::Eol)?;

        Ok(VariableDefinition { name, ty })
    }

    fn parse_instruction(&mut self) -> Result<Instruction, Error> {
        if self.peek_t::<NoDstOp>().is_ok() {
            match self.consume_t::<NoDstOp>()? {
                NoDstOp::Nop => Ok(Instruction::Nop),
                NoDstOp::Jmp => {
                    let offset = self.parse_i32()?;
                    Ok(Instruction::Jump(Jump { offset }))
                }
                NoDstOp::JmpT => {
                    let condition = self.parse_value()?;
                    let offset = self.parse_i32()?;
                    Ok(Instruction::JumpIfTrue(ConditionalJump {
                        condition,
                        offset,
                    }))
                }
                NoDstOp::JmpF => {
                    let condition = self.parse_value()?;
                    let offset = self.parse_i32()?;
                    Ok(Instruction::JumpIfFalse(ConditionalJump {
                        condition,
                        offset,
                    }))
                }
                NoDstOp::Ret => {
                    let value = self.parse_value()?;
                    Ok(Instruction::Return(value))
                }
                NoDstOp::PropertySet => {
                    let object = self.parse_value()?;
                    let property = self.consume_ident()?;
                    let value = self.parse_value()?;

                    Ok(Instruction::PropertySet(PropertySet {
                        object,
                        property,
                        value,
                    }))
                }
                NoDstOp::ArraySet => {
                    let array = self.parse_value()?;
                    let index = self.parse_value()?;
                    let value = self.parse_value()?;

                    Ok(Instruction::ArraySet(ArraySet {
                        array,
                        index,
                        value,
                    }))
                }
                NoDstOp::StructSet => {
                    let struct_ = self.parse_value()?;
                    let member = self.consume_ident()?;
                    let value = self.parse_value()?;

                    Ok(Instruction::StructSet(StructSet {
                        struct_value: struct_,
                        member_name: member,
                        value,
                    }))
                }
                NoDstOp::ArrayAdd => {
                    let array = self.parse_value()?;
                    let value = self.parse_value()?;
                    let count = self.parse_value()?;

                    Ok(Instruction::ArrayAdd(ArrayAdd {
                        array,
                        value,
                        count,
                    }))
                }
                NoDstOp::ArrayInsert => {
                    let array = self.parse_value()?;
                    let index = self.parse_value()?;
                    let value = self.parse_value()?;

                    Ok(Instruction::ArrayInsert(ArrayInsert {
                        array,
                        index,
                        value,
                    }))
                }
                NoDstOp::ArrayRemove => {
                    let array = self.parse_value()?;
                    let index = self.parse_value()?;
                    let count = self.parse_value()?;

                    Ok(Instruction::ArrayRemove(ArrayRemove {
                        array_value: array,
                        index,
                        count,
                    }))
                }
                NoDstOp::ArrayPop => {
                    let array = self.parse_value()?;
                    Ok(Instruction::ArrayPop(ArrayPop { array }))
                }
                NoDstOp::ArrayClear => {
                    let array = self.parse_value()?;
                    Ok(Instruction::ArrayClear(ArrayClear { array }))
                }
            }
        } else {
            let dst = self.parse_value()?;
            self.consume_token(&TokenKind::Eq)?;
            let op = self.consume_t::<DstOp>()?;

            match op {
                DstOp::IAdd => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::IAdd(Binop { dst, lhs, rhs }))
                }
                DstOp::ISub => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::ISub(Binop { dst, lhs, rhs }))
                }
                DstOp::IMul => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::IMul(Binop { dst, lhs, rhs }))
                }
                DstOp::IDiv => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::IDiv(Binop { dst, lhs, rhs }))
                }
                DstOp::IMod => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::IMod(Binop { dst, lhs, rhs }))
                }
                DstOp::FAdd => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::FAdd(Binop { dst, lhs, rhs }))
                }
                DstOp::FSub => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::FSub(Binop { dst, lhs, rhs }))
                }
                DstOp::FMul => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::FMul(Binop { dst, lhs, rhs }))
                }
                DstOp::FDiv => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::FDiv(Binop { dst, lhs, rhs }))
                }
                DstOp::INeg => {
                    let src = self.parse_value()?;
                    Ok(Instruction::INeg(Unop { src, dst }))
                }
                DstOp::FNeg => {
                    let src = self.parse_value()?;
                    Ok(Instruction::FNeg(Unop { src, dst }))
                }
                DstOp::Assign => {
                    let src = self.parse_value()?;
                    Ok(Instruction::Store(Unop { src, dst }))
                }
                DstOp::Cast => {
                    let src = self.parse_value()?;
                    Ok(Instruction::Cast(Unop { src, dst }))
                }
                DstOp::Is => {
                    let object = self.parse_value()?;
                    let class = self.consume_ident()?;

                    Ok(Instruction::Is(Is {
                        return_assignment: dst,
                        object,
                        class,
                    }))
                }
                DstOp::Not => {
                    let src = self.parse_value()?;
                    Ok(Instruction::Not(Unop { src, dst }))
                }
                DstOp::CmpEq => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::CmpEq(Binop { dst, lhs, rhs }))
                }
                DstOp::CmpLt => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::CmpLt(Binop { dst, lhs, rhs }))
                }
                DstOp::CmpLe => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::CmpLe(Binop { dst, lhs, rhs }))
                }
                DstOp::CmpGt => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::CmpGt(Binop { dst, lhs, rhs }))
                }
                DstOp::CmpGe => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::CmpGe(Binop { dst, lhs, rhs }))
                }
                DstOp::CallMethod => {
                    let object = self.parse_value()?;
                    let function_name = self.consume_ident()?;

                    let mut args = Vec::new();
                    while self.peek().is_some_and(|t| t.kind != TokenKind::Eol) {
                        let arg = self.parse_value()?;
                        args.push(arg);
                    }

                    Ok(Instruction::CallMethod(CallMethod {
                        object,
                        function_name,
                        return_assignment: dst,
                        args,
                    }))
                }
                DstOp::CallParent => {
                    let function = self.consume_ident()?;

                    let mut args = Vec::new();
                    while self.peek().is_some_and(|t| t.kind != TokenKind::Eol) {
                        let arg = self.parse_value()?;
                        args.push(arg);
                    }

                    Ok(Instruction::CallParent(CallParent {
                        function,
                        return_assignment: dst,
                        args,
                    }))
                }
                DstOp::CallStatic => {
                    let class = self.consume_ident()?;
                    let function = self.consume_ident()?;

                    let mut args = Vec::new();
                    while self.peek().is_some_and(|t| t.kind != TokenKind::Eol) {
                        let arg = self.parse_value()?;
                        args.push(arg);
                    }

                    Ok(Instruction::CallStatic(CallStatic {
                        class,
                        function,
                        return_assignment: dst,
                        args,
                    }))
                }
                DstOp::StringConcat => {
                    let lhs = self.parse_value()?;
                    let rhs = self.parse_value()?;
                    Ok(Instruction::StringConcat(Binop { dst, lhs, rhs }))
                }
                DstOp::PropertyGet => {
                    let object = self.parse_value()?;
                    let property = self.consume_ident()?;

                    Ok(Instruction::PropertyGet(PropertyGet {
                        value: object,
                        property,
                        return_assignment: dst,
                    }))
                }
                DstOp::ArrayCreate => {
                    let len = self.parse_value()?;
                    Ok(Instruction::ArrayCreate(Unop { src: len, dst }))
                }
                DstOp::ArrayLength => {
                    let array = self.parse_value()?;
                    Ok(Instruction::ArrayLength(Unop { src: array, dst }))
                }
                DstOp::ArrayGet => {
                    let array = self.parse_value()?;
                    let index = self.parse_value()?;
                    Ok(Instruction::ArrayGet(ArrayGet {
                        array,
                        index,
                        return_assignment: dst,
                    }))
                }
                DstOp::ArrayFind => {
                    let array = self.parse_value()?;
                    let value = self.parse_value()?;
                    let start_index = self.parse_value()?;

                    Ok(Instruction::ArrayFind(ArrayFind {
                        array_value: array,
                        value,
                        start_index,
                        return_index: dst,
                    }))
                }
                DstOp::ArrayRFind => {
                    let array = self.parse_value()?;
                    let value = self.parse_value()?;
                    let start_index = self.parse_value()?;

                    Ok(Instruction::ArrayRFind(ArrayFind {
                        array_value: array,
                        return_index: dst,
                        value,
                        start_index,
                    }))
                }
                DstOp::StructCreate => Ok(Instruction::StructCreate(StructCreate {
                    return_assignment: dst,
                })),
                DstOp::StructGet => {
                    let struct_ = self.parse_value()?;
                    let member = self.consume_ident()?;

                    Ok(Instruction::StructGet(StructGet {
                        struct_value: struct_,
                        member_name: member,
                        return_assignment: dst,
                    }))
                }
                DstOp::ArrayFindStruct => {
                    let array = self.parse_value()?;
                    let member = self.consume_ident()?;
                    let value = self.parse_value()?;
                    let start_index = self.parse_value()?;

                    Ok(Instruction::ArrayFindStruct(ArrayFindStruct {
                        array_value: array,
                        member_name: member,
                        start_index,
                        value,
                        return_index: dst,
                    }))
                }
                DstOp::ArrayRFindStruct => {
                    let array = self.parse_value()?;
                    let member = self.consume_ident()?;
                    let value = self.parse_value()?;
                    let start_index = self.parse_value()?;

                    Ok(Instruction::ArrayRFindStruct(ArrayFindStruct {
                        array_value: array,
                        member_name: member,
                        start_index,
                        value,
                        return_index: dst,
                    }))
                }
            }
        }
    }

    fn parse_value(&mut self) -> Result<Value, Error> {
        let token = self.tokens.split_off_first().unwrap();
        match &token.kind {
            TokenKind::Literal(lit) => match lit {
                Literal::None => Ok(Value::Null),
                Literal::Bool(val) => Ok(Value::Bool(*val)),
                Literal::Int(val) => Ok(Value::Int(*val)),
                Literal::Float(val) => Ok(Value::Float(*val)),
                Literal::String(val) => {
                    let val = self.make_str(val.clone());
                    Ok(Value::String(val))
                }
            },
            TokenKind::Ident(name) => Ok(Value::Ident(self.make_str(*name))),
            _ => Err(Error {
                msg: format!("expected literal or identifier, found {}", token.kind),
                span: token.span,
            }),
        }
    }

    fn parse_i32(&mut self) -> Result<i32, Error> {
        let token = self.tokens.split_off_first().unwrap();
        match &token.kind {
            TokenKind::Literal(Literal::Int(val)) => Ok(*val),
            _ => Err(Error {
                msg: format!("expected integer literal, found {}", token.kind),
                span: token.span,
            }),
        }
    }

    fn parse_string(&mut self) -> Result<StringKey, Error> {
        let token = self.tokens.split_off_first().unwrap();
        match &token.kind {
            TokenKind::Literal(Literal::String(s)) => Ok(self.make_str(s.clone())),
            _ => Err(Error {
                msg: format!("expected string literal, found {}", token.kind),
                span: token.span,
            }),
        }
    }

    fn peek(&self) -> Option<&'a Token<'a>> {
        self.tokens.first()
    }

    fn peek_t<T>(&self) -> Result<T, Error>
    where
        T: FromToken,
    {
        let token = self.tokens.first().unwrap();
        T::extract(&token.kind).ok_or_else(|| Error {
            msg: T::make_error(&token.kind),
            span: token.span,
        })
    }

    fn consume_token(&mut self, kind: &TokenKind<'_>) -> Result<(), Error> {
        let token = self.tokens.split_off_first().unwrap();
        if token.kind != *kind {
            return Err(Error {
                msg: format!("expected {kind}, found {}", token.kind),
                span: token.span,
            });
        } else {
            Ok(())
        }
    }

    fn consume_ident(&mut self) -> Result<StringKey, Error> {
        let token = self.tokens.split_off_first().unwrap();

        match token.kind {
            TokenKind::Ident(s) => Ok(self.make_str(s)),
            _ => Err(Error {
                msg: format!("expected identifier, found {}", token.kind),
                span: token.span,
            }),
        }
    }

    fn consume_t<T>(&mut self) -> Result<T, Error>
    where
        T: FromToken,
    {
        let token = self.tokens.split_off_first().unwrap();
        T::extract(&token.kind).ok_or_else(|| Error {
            msg: T::make_error(&token.kind),
            span: token.span,
        })
    }

    fn make_str<T>(&mut self, s: T) -> StringKey
    where
        T: Into<Cow<'a, str>>,
    {
        let s = s.into();
        *self
            .strings
            .entry(s.clone())
            .or_insert_with(|| self.module.strings.insert((*s).into()))
    }
}

trait FromToken: Sized {
    const OPTIONS: &[TokenKind<'static>];

    fn extract(token: &TokenKind<'_>) -> Option<Self>;

    fn make_error(token: &TokenKind<'_>) -> String {
        match Self::OPTIONS.len() {
            0 => {
                format!("unexpected token {token}")
            }
            1 => {
                let expect = &Self::OPTIONS[0];
                format!("expected {expect}, found {token}")
            }
            _ => {
                let mut s = format!("expected one of: {}", Self::OPTIONS[0]);
                for v in Self::OPTIONS.iter().skip(1) {
                    write!(s, ", {}", v).unwrap();
                }

                write!(s, ", found {token}").unwrap();

                s
            }
        }
    }
}

macro_rules! define_from_token_ident {
    (
        enum $id:ident {
            $(
                $variant:ident = $name:expr,
            )*
        }
    ) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        enum $id {
            $(
                $variant,
            )*
        }

        impl FromToken for $id {
            const OPTIONS: &[TokenKind<'static>] = &[
                $(
                    TokenKind::Ident($name),
                )*
            ];

            fn extract(token: &TokenKind<'_>) -> Option<Self> {
                match token {
                    $(
                        TokenKind::Ident($name) => Some(Self::$variant),
                    )*
                    _ => None,
                }
            }
        }
    };
}

define_from_token_ident! {
    enum ClassChild {
        Class = "class",
        State = "state",
        Function = "function",
    }
}

define_from_token_ident! {
    enum FuncAttribute {
        Static = "static",
        Native = "native",
    }
}

define_from_token_ident! {
    enum NoDstOp {
        Nop = "nop",
        Jmp = "jmp",
        JmpT = "jmpt",
        JmpF = "jmpf",
        Ret = "ret",
        PropertySet = "property_set",
        ArraySet = "array_set",
        StructSet = "struct_set",
        ArrayAdd = "array_add",
        ArrayInsert = "array_insert",
        ArrayPop = "array_pop",
        ArrayRemove = "array_remove",
        ArrayClear = "array_clear",
    }
}

define_from_token_ident! {
    enum DstOp {
        IAdd = "iadd",
        ISub = "isub",
        IMul = "imul",
        IDiv = "idiv",
        IMod = "imod",
        FAdd = "fadd",
        FSub = "fsub",
        FMul = "fmul",
        FDiv = "fdiv",
        INeg = "ineg",
        FNeg = "fneg",
        Not = "not",
        Assign = "assign",
        Cast = "cast",
        Is = "is",
        CmpEq = "cmpeq",
        CmpLt = "cmplt",
        CmpLe = "cmple",
        CmpGt = "cmpgt",
        CmpGe = "cmpge",
        CallMethod = "call_method",
        CallParent = "call_parent",
        CallStatic = "call_static",
        StringConcat = "string_concat",
        PropertyGet = "property_get",
        ArrayCreate = "array_create",
        ArrayLength = "array_length",
        ArrayGet = "array_get",
        ArrayFind = "array_find",
        ArrayRFind = "array_rfind",
        StructCreate = "struct_create",
        StructGet = "struct_get",
        ArrayFindStruct = "array_find_struct",
        ArrayRFindStruct = "array_rfind_struct",
    }
}
