mod condition;
mod func;
mod lowering;
mod reg;
mod ssa;

use std::collections::HashMap;
use std::debug_assert_matches;

use pex::VariableDefinition;

use crate::ast::{Ast, Expr, Function, Statement, Type};
use crate::codegen::condition::lower_condition_expr;
use crate::codegen::func::{Block, ControlFlowGraph};
use crate::codegen::reg::Register;
use crate::codegen::ssa::{BlockId, SsaBuilder, Value, Var};
use crate::lexer::Literal;
use crate::{StringBuilder, ast};

#[derive(Clone, Debug)]
pub enum Instruction<T> {
    AddParams(Vec<Param<T>>),
    Literal(Literal<'static>, T),
    IAdd(Binop<T>),
    ISub(Binop<T>),
    IMul(Binop<T>),
    IDiv(Binop<T>),
    IRem(Binop<T>),
    INeg(Unop<T>),
    FAdd(Binop<T>),
    FSub(Binop<T>),
    FMul(Binop<T>),
    FDiv(Binop<T>),
    FRem(Binop<T>),
    FNeg(Unop<T>),
    CmpEq(Binop<T>),
    CmpNe(Binop<T>),
    CmpLt(Binop<T>),
    CmpLe(Binop<T>),
    CmpGt(Binop<T>),
    CmpGe(Binop<T>),
    Call(Call<T>),
    Jmp(BlockId, Vec<T>),
    Brif(T, BlockId, Vec<T>, BlockId, Vec<T>),
    Store(Unop<T>),
    Return(T),
    LoadGlobal(String, T),
    StoreGlobal(String, T),
    Cast(Cast<T>),
    StringConcat(Binop<T>),
    GetProperty(PropertyRef<T>, T),
    SetProperty(PropertyRef<T>, T),
    BNot(Unop<T>),
    ArrayLen(Unop<T>),
    ArrayLoad(ArrayLoad<T>),
    ArrayStore(ArrayStore<T>),
    ArrayAlloc(Unop<T>),
}

#[derive(Clone, Debug)]
pub struct Unop<T> {
    pub src: T,
    pub dst: T,
}

#[derive(Clone, Debug)]
pub struct Binop<T> {
    pub lhs: T,
    pub rhs: T,
    pub dst: T,
}

#[derive(Clone, Debug)]
struct Call<T> {
    callee: Callee<T>,
    args: Vec<T>,
    returns: T,
}

#[derive(Clone, Debug)]
struct Cast<T> {
    pub src: T,
    pub dst: T,
    src_ty: Type,
    dst_ty: Type,
}

#[derive(Copy, Clone, Debug)]
pub struct ArrayLoad<T> {
    pub array: T,
    pub index: T,
    pub dst: T,
}

#[derive(Copy, Clone, Debug)]
pub struct ArrayStore<T> {
    pub array: T,
    pub index: T,
    pub value: T,
}

#[derive(Clone, Debug)]
struct Tranlsator<'a> {
    ssa: SsaBuilder,
    variables: HashMap<String, Var>,
    blocks: HashMap<BlockId, Vec<Instruction<Value>>>,
    cur_block: Option<BlockId>,
    entry: BlockId,
    ast: &'a Ast,
    func: &'a Function,
    types: HashMap<Value, TypeOrPlace>,
    var_types: HashMap<Var, TypeOrPlace>,
}

impl<'a> Tranlsator<'a> {
    fn new(ast: &'a Ast, func: &'a Function) -> Self {
        let mut ssa = SsaBuilder::new();
        let entry = ssa.create_block();
        ssa.seal(entry);
        ssa.switch_block(entry);

        let mut blocks = HashMap::new();
        blocks.insert(entry, Vec::new());

        Self {
            ssa,
            variables: HashMap::new(),
            blocks,
            cur_block: Some(entry),
            ast,
            func,
            entry,
            types: HashMap::new(),
            var_types: HashMap::new(),
        }
    }

    pub fn get_or_create_var(&mut self, ident: &str) -> Var {
        *self
            .variables
            .entry(ident.to_owned())
            .or_insert_with(|| self.ssa.create_var())
    }

    pub fn push_ins(&mut self, ins: Instruction<Value>) {
        // If this is `None` we are in unreachable code.
        // This usually happens after more code exists after
        // a return.
        // Ignoring unrechable code here already is easy and
        // reduces pressure on all later passes.
        if let Some(block) = self.cur_block {
            self.blocks.entry(block).or_default().push(ins);
        }
    }

    pub fn create_block(&mut self) -> BlockId {
        let block = self.ssa.create_block();
        self.blocks.insert(block, Vec::new());
        block
    }

    pub fn switch_to_block(&mut self, block: BlockId) {
        self.ssa.switch_block(block);
        self.cur_block = Some(block);
    }

    fn load_place(&mut self, value: Value, skip_last: usize) -> Value {
        match self.types.get(&value) {
            Some(TypeOrPlace::Place(segments)) => {
                let mut value = value;

                'out: for seg in segments
                    .clone()
                    .into_iter()
                    .take(segments.len().saturating_sub(skip_last))
                {
                    match seg {
                        PathSegment::Local(local) => value = local,
                        PathSegment::Item(item) => {
                            if let Some(global) = self.ast.globals.iter().find(|v| v.name == item) {
                                let dst = self.create_value(global.ty.clone());
                                self.push_ins(Instruction::LoadGlobal(global.name.clone(), dst));
                                value = dst;
                                continue;
                            }

                            if let Some(prop) = self.ast.properties.iter().find(|v| v.name == item)
                            {
                                let dst = self.create_value(prop.ty.clone());
                                self.push_ins(Instruction::GetProperty(
                                    PropertyRef::Local(prop.name.to_owned()),
                                    dst,
                                ));
                                value = dst;
                                continue;
                            };

                            for group in &self.ast.property_groups {
                                if let Some(prop) = group.properties.iter().find(|v| v.name == item)
                                {
                                    let dst = self.create_value(prop.ty.clone());
                                    self.push_ins(Instruction::GetProperty(
                                        PropertyRef::Local(prop.name.to_owned()),
                                        dst,
                                    ));
                                    value = dst;
                                    continue 'out;
                                };
                            }

                            if let Some(func) = self.ast.functions.iter().find(|v| v.name == item) {
                                unreachable!(
                                    "attempted to load function {} as part of place",
                                    func.name
                                );
                            };

                            if matches!(self.types[&value], TypeOrPlace::Type(Type::Array(_)))
                                && item.eq_ignore_ascii_case("length")
                            {
                                let dst = self.create_value(Type::Int);
                                self.push_ins(Instruction::ArrayLen(Unop { src: value, dst }));
                                value = dst;
                                continue;
                            }

                            unreachable!("unknown item {}", item)
                        }
                        PathSegment::Index { array, index } => {
                            let TypeOrPlace::Type(Type::Array(ty)) = &self.types[&array] else {
                                unreachable!();
                            };

                            let dst = self.create_value(*ty.clone());
                            self.push_ins(Instruction::ArrayLoad(ArrayLoad { array, index, dst }));
                            value = dst;
                        }
                    }
                }

                value
            }
            Some(TypeOrPlace::Type(_)) => value,
            None => unreachable!(),
        }
    }

    fn create_value(&mut self, ty: Type) -> Value {
        let value = self.ssa.create_value();
        self.types.insert(value, TypeOrPlace::Type(ty));
        value
    }

    fn create_place(&mut self, place: Vec<PathSegment>) -> Value {
        let value = self.ssa.create_value();
        self.types.insert(value, TypeOrPlace::Place(place));
        value
    }

    fn load_place_as_func(&mut self, value: Value) -> Callee<Value> {
        match &self.types[&value] {
            TypeOrPlace::Place(segments) => {
                let mut segments: &[PathSegment] = &segments;
                debug_assert_matches!(segments.len(), 1 | 2);

                // The function name is the last segment.
                let PathSegment::Item(func) = segments.split_off_last().unwrap() else {
                    unreachable!();
                };

                let func = func.to_owned();
                match segments.get(0) {
                    Some(PathSegment::Local(local)) => Callee::Local {
                        object: *local,
                        func,
                    },
                    Some(PathSegment::Item(item)) => {
                        if let Some(global) = self.ast.globals.iter().find(|v| &v.name == item) {
                            let dst = self.create_value(global.ty.clone());
                            self.push_ins(Instruction::LoadGlobal(global.name.clone(), dst));
                            return Callee::Local { object: dst, func };
                        }

                        Callee::Static {
                            class: item.to_owned(),
                            func,
                        }
                    }
                    None => Callee::LocalSelf { func },
                    _ => unreachable!(),
                }
            }
            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CodegenOptions {
    /// Whether to preserve the exact names of parameters in the compiled function.
    pub preserve_parameter_names: bool,
}

pub fn lower_function(
    ast: &Ast,
    func: &Function,
    strings: &mut StringBuilder,
    opts: &CodegenOptions,
) -> pex::Function {
    if func.is_native {
        return lower_native_function(func, strings, opts);
    }

    let mut translator = Tranlsator::new(ast, func);

    let mut params = Vec::new();
    for param in &func.params {
        let var = translator.get_or_create_var(&param.name);
        let value = translator.create_value(param.ty.clone());
        translator.ssa.def_var(var, value);
        translator
            .var_types
            .insert(var, TypeOrPlace::Type(param.ty.clone()));

        params.push(Param {
            ty: param.ty.clone(),
            value,
        });
    }

    translator.push_ins(Instruction::AddParams(params.clone()));

    for stmt in &func.statements {
        lower_stmt(&mut translator, stmt);
    }

    // Make sure the last instruction is a return.
    // The value we return will always be none, for all other types an
    // explicit return is always required.
    if translator.cur_block.is_some() {
        let value = translator.create_value(Type::None);
        translator.push_ins(Instruction::Literal(Literal::None, value));
        translator.push_ins(Instruction::Return(value));
    }

    let mut blocks = HashMap::new();
    for (id, mut instructions) in translator.blocks {
        for ins in &mut instructions {
            match ins {
                Instruction::Brif(_, then_block, then_args, else_block, else_args) => {
                    then_args.extend(translator.ssa.branch_args(id, *then_block));
                    else_args.extend(translator.ssa.branch_args(id, *else_block));
                }
                Instruction::Jmp(block, args) => {
                    args.extend(translator.ssa.branch_args(id, *block));
                }
                _ => {}
            }
        }

        blocks.insert(
            id,
            Block {
                instructions,
                params: translator.ssa.block_params(id).to_vec(),
            },
        );
    }

    let cfg = ControlFlowGraph {
        entry: translator.entry,
        blocks,
    };

    // Places are a compile-time only concept and do not exist at runtime.
    // There are no instructions that define or use place "values" in the CFG.
    let mut value_types = HashMap::new();
    for (value, ty) in translator.types {
        if let TypeOrPlace::Type(ty) = ty {
            value_types.insert(value, ty);
        }
    }

    let (regcfg, regtys) = reg::ssa_to_reg(&cfg, value_types);

    let func = FuncInfo {
        name: func.name.clone(),
        cfg: regcfg.clone(),
        is_static: func.is_global,
        return_ty: func.return_ty.clone(),
        regtys,
    };

    lowering::lower_to_pex(&func, strings)
}

fn lower_stmt(translator: &mut Tranlsator, stmt: &Statement) {
    match stmt {
        Statement::Define(def) => {
            let var = translator.ssa.create_var();
            translator.variables.insert(def.name.clone(), var);

            translator
                .var_types
                .insert(var, TypeOrPlace::Type(def.ty.clone()));

            if let Some(expr) = &def.value {
                let value = load_place_expr(translator, expr);
                translator.ssa.def_var(var, value);
            }
        }
        Statement::Expr(expr) => {
            lower_expr(translator, expr);
        }
        Statement::If {
            then_blocks,
            else_block,
        } => {
            let merge_block = translator.create_block();

            let mut next_block = translator.create_block();
            for block in then_blocks {
                let then_block = translator.create_block();

                lower_condition_expr(translator, &block.condition, then_block, next_block);

                translator.ssa.seal(then_block);
                translator.switch_to_block(then_block);
                for stmt in &block.statements {
                    lower_stmt(translator, stmt);
                }

                translator.push_ins(Instruction::Jmp(merge_block, Vec::new()));
                translator.ssa.branch(&[merge_block]);

                translator.switch_to_block(next_block);
                translator.ssa.seal(next_block);

                next_block = translator.ssa.create_block();
            }

            for stmt in else_block {
                lower_stmt(translator, stmt);
            }

            translator.push_ins(Instruction::Jmp(merge_block, Vec::new()));
            translator.ssa.branch(&[merge_block]);

            translator.ssa.seal(merge_block);
            translator.switch_to_block(merge_block);
        }
        Statement::While {
            condition,
            then_block,
        } => {
            let cond_block = translator.create_block();
            let body_block = translator.create_block();
            let merge_block = translator.create_block();

            translator.push_ins(Instruction::Jmp(cond_block, Vec::new()));
            translator.ssa.branch(&[cond_block]);

            translator.switch_to_block(cond_block);
            lower_condition_expr(translator, condition, body_block, merge_block);

            translator.ssa.seal(body_block);
            translator.switch_to_block(body_block);
            for stmt in then_block {
                lower_stmt(translator, stmt);
            }

            translator.push_ins(Instruction::Jmp(cond_block, Vec::new()));
            translator.ssa.branch(&[cond_block]);
            translator.ssa.seal(cond_block);

            translator.ssa.seal(merge_block);
            translator.switch_to_block(merge_block);
        }
        Statement::Return(expr) => {
            let value = match expr {
                Some(expr) => load_place_expr(translator, expr),
                None => translator.ssa.create_value(),
            };
            translator.push_ins(Instruction::Return(value));
            translator.cur_block = None;
        }
    }
}

fn lower_expr(translator: &mut Tranlsator, expr: &Expr) -> Value {
    match expr {
        Expr::Literal(literal) => {
            let ty = match literal {
                Literal::None => Type::None,
                Literal::Bool(_) => Type::Bool,
                Literal::NumberInt(_) => Type::Int,
                Literal::NumberDecimal {
                    integer: _,
                    fraction: _,
                } => Type::Float,
                Literal::String(_) => Type::String,
            };

            let value = translator.create_value(ty);
            translator.push_ins(Instruction::Literal(literal.clone(), value));
            value
        }
        Expr::Ident { ident } => {
            // Identifier is a regular stack-local variable.
            if let Some(var) = translator.variables.get(ident) {
                let value = translator.ssa.use_var(*var);

                // During SSA construction a variable use may insert additional
                // values as phi parameters in other blocks.
                // We must keep track of the type of all implicitly created values.
                for val in translator.ssa.var_values(*var) {
                    translator
                        .types
                        .insert(*val, translator.var_types[var].clone());
                }

                return value;
            }

            // Otherwise the identifier is a plain type/value name that needs to
            // be reconstructed into a value by a deref operation.
            let value = translator.create_place(vec![PathSegment::Item(ident.to_owned())]);
            value
        }
        Expr::Index { array, index } => {
            let array = load_place_expr(translator, array);
            let index = load_place_expr(translator, index);
            translator.create_place(vec![PathSegment::Index { array, index }])
        }
        Expr::Add { lhs, rhs } => {
            lower_arithmetic_binop(translator, ArithmeticBinop::Add, lhs, rhs)
        }
        Expr::Sub { lhs, rhs } => {
            lower_arithmetic_binop(translator, ArithmeticBinop::Sub, lhs, rhs)
        }
        Expr::Mul { lhs, rhs } => {
            lower_arithmetic_binop(translator, ArithmeticBinop::Mul, lhs, rhs)
        }
        Expr::Div { lhs, rhs } => {
            lower_arithmetic_binop(translator, ArithmeticBinop::Mul, lhs, rhs)
        }
        Expr::Rem { lhs, rhs } => {
            lower_arithmetic_binop(translator, ArithmeticBinop::Div, lhs, rhs)
        }
        Expr::CmpEq { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpEq(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpNe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpNe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLt { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpLt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpLe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGt { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpGt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::CmpGe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Call { callee, args } => {
            let callee = lower_expr(translator, callee);
            let callee = translator.load_place_as_func(callee);

            let args: Vec<_> = args
                .iter()
                .map(|arg| load_place_expr(translator, arg))
                .collect();
            let dst = translator.create_value(Type::None);
            translator.push_ins(Instruction::Call(Call {
                callee,
                args,
                returns: dst,
            }));
            dst
        }
        Expr::Deref { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);

            let TypeOrPlace::Place(rhs) = &translator.types[&rhs] else {
                unreachable!();
            };

            let mut segments = match &translator.types[&lhs] {
                TypeOrPlace::Place(segments) => segments.clone(),
                TypeOrPlace::Type(_) => vec![PathSegment::Local(lhs)],
            };

            segments.extend(rhs.clone());
            translator.create_place(segments)
        }
        Expr::Assign { src, dst } => {
            let src = load_place_expr(translator, src);
            store_place(translator, src, dst);
            translator.create_value(Type::None)
        }
        Expr::AddAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Add, dst, src);
            store_place(translator, res, dst);
            translator.create_value(Type::None)
        }
        Expr::SubAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Sub, dst, src);
            store_place(translator, res, dst);
            translator.create_value(Type::None)
        }
        Expr::MulAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Mul, dst, src);
            store_place(translator, res, dst);
            translator.create_value(Type::None)
        }
        Expr::DivAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Div, dst, src);
            store_place(translator, res, dst);
            translator.create_value(Type::None)
        }
        Expr::RemAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Rem, dst, src);
            store_place(translator, res, dst);
            translator.create_value(Type::None)
        }
        Expr::Neg { value } => {
            let src = load_place_expr(translator, value);
            let TypeOrPlace::Type(ty) = translator.types[&src].clone() else {
                unreachable!();
            };

            let dst = translator.create_value(ty.clone());
            match ty {
                Type::Int => {
                    translator.push_ins(Instruction::INeg(Unop { src, dst }));
                }
                Type::Float => {
                    translator.push_ins(Instruction::FNeg(Unop { src, dst }));
                }
                _ => unreachable!(),
            }

            dst
        }
        Expr::LogicalNot { value } => {
            let src = load_place_expr(translator, value);
            let src = convert_to(translator, src, Type::Bool);
            let dst = translator.create_value(Type::Bool);
            translator.push_ins(Instruction::BNot(Unop { src, dst }));
            dst
        }
        // Only allowed in condition expressions.
        Expr::LogicalAnd { lhs: _, rhs: _ } | Expr::LogicalOr { lhs: _, rhs: _ } => {
            unreachable!()
        }
        Expr::NewArray { ty, len } => {
            let len = load_place_expr(translator, len);
            let dst = translator.create_value(Type::Array(Box::new(ty.clone())));
            translator.push_ins(Instruction::ArrayAlloc(Unop { src: len, dst }));
            dst
        }
        x => todo!("{:?}", x),
    }
}

fn convert_to(translator: &mut Tranlsator, src: Value, want: Type) -> Value {
    let TypeOrPlace::Type(is) = &translator.types[&src] else {
        unreachable!();
    };

    if *is == want {
        return src;
    }

    let is = is.clone();

    let dst = translator.create_value(want.clone());
    translator.push_ins(Instruction::Cast(Cast {
        src,
        dst,
        src_ty: is,
        dst_ty: want,
    }));

    dst
}

fn load_place_expr(translator: &mut Tranlsator, expr: &Expr) -> Value {
    let place = lower_expr(translator, expr);
    translator.load_place(place, 0)
}

/// Stores the given `value` into the given place expression `dst`.
fn store_place(translator: &mut Tranlsator, value: Value, dst: &Expr) {
    let place = lower_expr(translator, dst);

    match &translator.types[&place] {
        TypeOrPlace::Place(segments) => {
            let segments = segments.clone();

            let mut place = place;
            for seg in segments.iter().take(segments.len().saturating_sub(2)) {
                match seg {
                    PathSegment::Local(local) => place = *local,
                    PathSegment::Item(item) => {
                        if let Some(global) =
                            translator.ast.globals.iter().find(|v| &v.name == item)
                        {
                            let dst = translator.create_value(global.ty.clone());
                            translator.push_ins(Instruction::LoadGlobal(global.name.clone(), dst));
                            place = dst;
                            continue;
                        }
                    }
                    PathSegment::Index { array, index } => {
                        let TypeOrPlace::Type(Type::Array(ty)) = &translator.types[&array] else {
                            unreachable!();
                        };

                        let dst = translator.create_value(*ty.clone());
                        translator.push_ins(Instruction::ArrayLoad(ArrayLoad {
                            array: *array,
                            index: *index,
                            dst,
                        }));
                        place = dst;
                    }
                }
            }

            match segments.last().unwrap() {
                PathSegment::Local(local) => {
                    todo!()
                }
                PathSegment::Item(item) => {
                    if let Some(global) = translator.ast.globals.iter().find(|v| &v.name == item) {
                        let value = convert_to(translator, value, global.ty.clone());
                        translator.push_ins(Instruction::StoreGlobal(global.name.clone(), value));
                        return;
                    }

                    if let Some(prop) = translator
                        .ast
                        .properties
                        .iter()
                        .chain(
                            translator
                                .ast
                                .property_groups
                                .iter()
                                .flat_map(|g| &g.properties),
                        )
                        .find(|v| &v.name == item)
                    {
                        let value = convert_to(translator, value, prop.ty.clone());
                        translator.push_ins(Instruction::SetProperty(
                            PropertyRef::Local(prop.name.clone()),
                            value,
                        ));
                        return;
                    }

                    todo!();
                }
                PathSegment::Index { array, index } => {
                    let TypeOrPlace::Type(Type::Array(ty)) = &translator.types[&array] else {
                        unreachable!();
                    };

                    let value = convert_to(translator, value, *ty.clone());
                    translator.push_ins(Instruction::ArrayStore(ArrayStore {
                        array: *array,
                        index: *index,
                        value,
                    }));
                    return;
                }
            }
        }
        TypeOrPlace::Type(_) => {}
    }

    // The place is a local on-stack variable.
    if let Expr::Ident { ident } = dst {
        let var = translator.variables[ident];
        let TypeOrPlace::Type(want_ty) = &translator.var_types[&var] else {
            unreachable!();
        };
        let value = convert_to(translator, value, want_ty.clone());
        translator.ssa.def_var(var, value);
        return;
    }

    todo!();
}

fn lower_arithmetic_binop(
    translator: &mut Tranlsator,
    op: ArithmeticBinop,
    lhs: &Expr,
    rhs: &Expr,
) -> Value {
    let lhs = load_place_expr(translator, lhs);
    let rhs = load_place_expr(translator, rhs);

    let TypeOrPlace::Type(lhs_ty) = &translator.types[&lhs] else {
        unreachable!();
    };
    let TypeOrPlace::Type(rhs_ty) = &translator.types[&rhs] else {
        unreachable!();
    };

    // Only the add operator is allows for strings.
    if op == ArithmeticBinop::Add {
        if *lhs_ty == Type::String || *rhs_ty == Type::String {
            let lhs = convert_to(translator, lhs, Type::String);
            let rhs = convert_to(translator, rhs, Type::String);
            let dst = translator.create_value(Type::String);
            translator.push_ins(Instruction::StringConcat(Binop { lhs, rhs, dst }));
            return dst;
        }
    }

    if *lhs_ty == Type::Float || *rhs_ty == Type::Float {
        let lhs = convert_to(translator, lhs, Type::Float);
        let rhs = convert_to(translator, rhs, Type::Float);
        let dst = translator.create_value(Type::Float);

        let ins = match op {
            ArithmeticBinop::Add => Instruction::FAdd(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Sub => Instruction::FSub(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Mul => Instruction::FMul(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Div => Instruction::FDiv(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Rem => Instruction::FRem(Binop { lhs, rhs, dst }),
        };

        translator.push_ins(ins);
        return dst;
    }

    let dst = translator.create_value(Type::Int);
    let ins = match op {
        ArithmeticBinop::Add => Instruction::IAdd(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Sub => Instruction::ISub(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Mul => Instruction::IMul(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Div => Instruction::IDiv(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Rem => Instruction::IRem(Binop { lhs, rhs, dst }),
    };

    translator.push_ins(ins);
    dst
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum ArithmeticBinop {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Debug)]
pub struct FuncInfo {
    name: String,
    is_static: bool,
    cfg: ControlFlowGraph<Register>,
    return_ty: Option<Type>,
    regtys: HashMap<Register, Type>,
}

#[derive(Clone, Debug)]
struct Param<T> {
    ty: Type,
    value: T,
}

#[derive(Clone, Debug)]
enum TypeOrPlace {
    Type(Type),
    Place(Vec<PathSegment>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathSegment {
    Local(Value),
    Item(String),
    Index { array: Value, index: Value },
}

#[derive(Clone, Debug)]
enum Callee<T> {
    LocalSelf { func: String },
    Local { object: T, func: String },
    Static { class: String, func: String },
}

#[derive(Clone, Debug)]
enum PropertyRef<T> {
    Local(String),
    Other(T, String),
}

fn lower_native_function(
    func: &Function,
    strings: &mut StringBuilder,
    opts: &CodegenOptions,
) -> pex::Function {
    let return_ty = match &func.return_ty {
        Some(ty) => strings.insert(&ty.to_string()),
        None => strings.insert("none"),
    };

    let params = func
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let name = if opts.preserve_parameter_names {
                strings.insert(&param.name)
            } else {
                strings.insert(&format!("r{}", index))
            };

            VariableDefinition {
                name,
                ty: strings.insert(&param.ty.to_string()),
            }
        })
        .collect();

    pex::Function {
        return_ty,
        docstring: strings.insert(""),
        user_flags: 0,
        is_static: func.is_global,
        is_native: true,
        params,
        locals: Vec::new(),
        instructions: Vec::new(),
    }
}

pub fn lower_property(
    prop: &ast::Property,
    strings: &mut StringBuilder,
) -> (pex::Property, pex::Variable) {
    if prop.is_auto {
        // Unlike function locals, globals and properties must be preserved
        // across script versions. This means a new version of the script
        // may be used with data from an older version.
        // We must ensure that the mangling scheme is consistent.
        // This current scheme is the same as used by the offical papyrus
        // compiler in FO4.
        let global = format!("::{}", prop.name);

        let value = match &prop.value {
            Some(expr) => eval_constant_expr(expr, strings),
            None => match prop.ty {
                Type::None => pex::Value::Null,
                Type::Bool => pex::Value::Bool(false),
                Type::Int => pex::Value::Int(0),
                Type::Float => pex::Value::Float(0.0),
                Type::String => pex::Value::String(strings.insert("")),
                _ => pex::Value::Null,
            },
        };

        return (
            pex::Property {
                name: strings.insert(&prop.name),
                ty: strings.insert(&prop.ty.to_string()),
                docstring: strings.insert(""),
                user_flags: 0,
                autovar_name: Some(strings.insert(&global)),
                getter: None,
                setter: None,
            },
            pex::Variable {
                def: VariableDefinition {
                    name: strings.insert(&global),
                    ty: strings.insert(&prop.ty.to_string()),
                },
                user_flags: 0,
                value,
            },
        );
    }

    todo!()
}

pub fn lower_global(global: &ast::Define, strings: &mut StringBuilder) -> pex::Variable {
    let value = match &global.value {
        Some(expr) => eval_constant_expr(expr, strings),
        None => match global.ty {
            Type::None => pex::Value::Null,
            Type::Bool => pex::Value::Bool(false),
            Type::Int => pex::Value::Int(0),
            Type::Float => pex::Value::Float(0.0),
            Type::String => pex::Value::String(strings.insert("")),
            _ => pex::Value::Null,
        },
    };

    pex::Variable {
        def: VariableDefinition {
            name: strings.insert(&global.name),
            ty: strings.insert(&global.ty.to_string()),
        },
        user_flags: 0,
        value,
    }
}

fn eval_constant_expr(expr: &Expr, strings: &mut StringBuilder) -> pex::Value {
    match expr {
        Expr::Literal(Literal::None) => pex::Value::Null,
        Expr::Literal(Literal::Bool(v)) => pex::Value::Bool(*v),
        Expr::Literal(Literal::NumberInt(v)) => pex::Value::Int(*v),
        Expr::Literal(Literal::NumberDecimal { integer, fraction }) => {
            // todo!()
            pex::Value::Float(0.0)
        }
        Expr::Literal(Literal::String(v)) => pex::Value::String(strings.insert(v)),
        _ => todo!(),
    }
}
