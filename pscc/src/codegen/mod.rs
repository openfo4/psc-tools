mod func;
mod lowering;
mod reg;
mod ssa;

use std::collections::HashMap;

use crate::ast::{Ast, Expr, Function, Statement, Type};
use crate::codegen::func::{Block, ControlFlowGraph};
use crate::codegen::reg::Register;
use crate::codegen::ssa::{BlockId, SsaBuilder, Value, Var};
use crate::lexer::Literal;

#[derive(Clone, Debug)]
pub enum Instruction<T> {
    AddParams(Vec<Param<T>>),
    Literal(Literal<'static>, T),
    IAdd(Binop<T>),
    ISub(Binop<T>),
    IMul(Binop<T>),
    IDiv(Binop<T>),
    IRem(Binop<T>),
    FAdd(Binop<T>),
    FSub(Binop<T>),
    FMul(Binop<T>),
    FDiv(Binop<T>),
    FRem(Binop<T>),
    CmpEq(Binop<T>),
    CmpNe(Binop<T>),
    CmpLt(Binop<T>),
    CmpLe(Binop<T>),
    CmpGt(Binop<T>),
    CmpGe(Binop<T>),
    Call(Call<T>),
    Jmp(BlockId, Vec<T>),
    Brif(T, BlockId, Vec<T>, BlockId, Vec<T>),
    Deref(Binop<T>),
    Index(Binop<T>),
    Store(Unop<T>),
    Return(T),
    LoadGlobal(String, T),
    Cast(Cast<T>),
    StringConcat(Binop<T>),
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
    callee: String,
    args: Vec<T>,
    returns: T,
}

#[derive(Clone, Debug)]
struct Cast<T> {
    pub src: T,
    pub dst: T,
    src_ty: DynType,
    dst_ty: DynType,
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
    types: HashMap<Value, DynType>,
    var_types: HashMap<Var, DynType>,
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
        self.blocks
            .entry(self.cur_block.unwrap())
            .or_default()
            .push(ins);
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

    fn load_place(&mut self, value: Value) -> Value {
        match self.types.get(&value) {
            Some(DynType::PlaceGlobal(global)) => {
                let global = self.ast.globals.iter().find(|v| &v.name == global).unwrap();

                let value = self.create_value(DynType::from(&global.ty));
                self.push_ins(Instruction::LoadGlobal(global.name.clone(), value));
                value
            }
            _ => value,
        }
    }

    fn create_value(&mut self, ty: DynType) -> Value {
        let value = self.ssa.create_value();
        self.types.insert(value, ty);
        value
    }
}

pub fn lower_function(ast: &Ast, func: &Function) -> ControlFlowGraph<Value> {
    let mut translator = Tranlsator::new(ast, func);

    let mut params = Vec::new();
    for param in &func.params {
        let var = translator.get_or_create_var(&param.name);
        let value = translator.create_value(DynType::from(&param.ty));
        translator.ssa.def_var(var, value);

        params.push(Param {
            ty: param.ty.clone(),
            value,
        });
    }

    translator.push_ins(Instruction::AddParams(params.clone()));

    for stmt in &func.statements {
        lower_stmt(&mut translator, stmt);
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

    println!("{}", cfg);
    let regcfg = reg::ssa_to_reg(&cfg);

    println!("{}", regcfg);

    let func = FuncInfo {
        name: func.name.clone(),
        cfg: regcfg.clone(),
        is_static: func.is_global,
        return_ty: func.return_ty.clone(),
    };

    let f = lowering::lower_to_pex(&func);
    dbg!(&f);

    cfg
}

fn lower_stmt(translator: &mut Tranlsator, stmt: &Statement) {
    match stmt {
        Statement::Define(def) => {
            let var = translator.ssa.create_var();
            translator.variables.insert(def.name.clone(), var);

            translator.var_types.insert(var, DynType::from(&def.ty));

            if let Some(expr) = &def.value {
                let value = lower_expr(translator, expr);
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
            let then_block = translator.create_block();

            for block in then_blocks {
                let condition = lower_expr(translator, &block.condition);
                translator.push_ins(Instruction::Brif(
                    condition,
                    then_block,
                    Vec::new(),
                    merge_block,
                    Vec::new(),
                ));
                translator.ssa.branch(&[then_block, merge_block]);
                translator.ssa.seal(then_block);

                translator.switch_to_block(then_block);
                for stmt in &block.statements {
                    lower_stmt(translator, stmt);
                }

                translator.push_ins(Instruction::Jmp(merge_block, Vec::new()));
                translator.ssa.branch(&[merge_block]);
            }

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
            let condition = load_place_expr(translator, condition);
            translator.push_ins(Instruction::Brif(
                condition,
                body_block,
                Vec::new(),
                merge_block,
                Vec::new(),
            ));
            translator.ssa.branch(&[body_block, merge_block]);

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
                Some(expr) => lower_expr(translator, expr),
                None => translator.ssa.create_value(),
            };
            translator.push_ins(Instruction::Return(value));
        }
    }
}

fn lower_expr(translator: &mut Tranlsator, expr: &Expr) -> Value {
    match expr {
        Expr::Literal(literal) => {
            let ty = match literal {
                Literal::NumberInt(_) => DynType::Int,
                Literal::NumberDecimal {
                    integer: _,
                    fraction: _,
                } => DynType::Float,
                Literal::String(_) => DynType::String,
            };

            let value = translator.create_value(ty);
            translator.push_ins(Instruction::Literal(literal.clone(), value));
            value
        }
        Expr::Ident { ident } => {
            // Identifier is a regular stack-local variable.
            if let Some(var) = translator.variables.get(ident) {
                let value = translator.ssa.use_var(*var);
                return value;
            }

            // Identifier is the name of a global variable, property or function
            // of the current class or a parent class.
            if let Some(global) = translator.ast.globals.iter().find(|v| v.name == *ident) {
                let value = translator.ssa.create_value();
                translator
                    .types
                    .insert(value, DynType::PlaceGlobal(global.name.clone()));
                return value;
            }

            if let Some(func) = translator.ast.functions.iter().find(|v| v.name == *ident) {
                let value = translator.ssa.create_value();
                translator
                    .types
                    .insert(value, DynType::Func(func.name.clone()));
                return value;
            }

            // Otherwise the identifier is a plain type/value name that needs to
            // be reconstructed into a value by a deref operation.
            let value = translator.ssa.create_value();
            translator
                .types
                .insert(value, DynType::Path(ident.to_owned()));
            value
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
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpEq(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpNe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpNe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLt { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpLt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpLe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGt { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpGt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGe { lhs, rhs } => {
            let lhs = load_place_expr(translator, lhs);
            let rhs = load_place_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpGe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Call { callee, args } => {
            let callee = lower_expr(translator, callee);
            let callee = translator
                .types
                .get(&callee)
                .unwrap()
                .unwrap_func()
                .to_owned();

            let args: Vec<_> = args
                .iter()
                .map(|arg| load_place_expr(translator, arg))
                .collect();
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Call(Call {
                callee: callee.clone(),
                args,
                returns: dst,
            }));
            dst
        }
        Expr::Deref { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let (Some(DynType::Path(lhs)), Some(DynType::Path(rhs))) =
                (translator.types.get(&lhs), translator.types.get(&rhs))
            else {
                unreachable!();
            };

            todo!();
        }
        Expr::Index { array, index } => {
            let lhs = lower_expr(translator, array);
            let rhs = lower_expr(translator, index);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Index(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Assign { src, dst } => {
            let src = load_place_expr(translator, src);
            store_place(translator, src, dst);
            translator.create_value(DynType::None)
        }
        Expr::AddAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Add, dst, src);
            store_place(translator, res, dst);
            translator.create_value(DynType::None)
        }
        Expr::SubAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Sub, dst, src);
            store_place(translator, res, dst);
            translator.create_value(DynType::None)
        }
        Expr::MulAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Mul, dst, src);
            store_place(translator, res, dst);
            translator.create_value(DynType::None)
        }
        Expr::DivAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Div, dst, src);
            store_place(translator, res, dst);
            translator.create_value(DynType::None)
        }
        Expr::RemAssign { src, dst } => {
            let res = lower_arithmetic_binop(translator, ArithmeticBinop::Rem, dst, src);
            store_place(translator, res, dst);
            translator.create_value(DynType::None)
        }
        x => todo!("{:?}", expr),
    }
}

fn convert_to(translator: &mut Tranlsator, src: Value, want: DynType) -> Value {
    let is = &translator.types[&src];
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
    translator.load_place(place)
}

/// Stores the given `value` into the given place expression `dst`.
fn store_place(translator: &mut Tranlsator, value: Value, dst: &Expr) {
    // The place is a local on-stack variable.
    if let Expr::Ident { ident } = dst {
        let var = translator.variables[ident];
        let want_ty = &translator.var_types[&var];
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

    dbg!(&translator);
    dbg!(lhs, rhs);

    let lhs_ty = &translator.types[&lhs];
    let rhs_ty = &translator.types[&rhs];

    // Only the add operator is allows for strings.
    if op == ArithmeticBinop::Add {
        if *lhs_ty == DynType::String || *rhs_ty == DynType::String {
            let lhs = convert_to(translator, lhs, DynType::String);
            let rhs = convert_to(translator, rhs, DynType::String);
            let dst = translator.create_value(DynType::String);
            translator.push_ins(Instruction::StringConcat(Binop { lhs, rhs, dst }));
            return dst;
        }
    }

    if *lhs_ty == DynType::Float || *rhs_ty == DynType::Float {
        let lhs = convert_to(translator, lhs, DynType::Float);
        let rhs = convert_to(translator, rhs, DynType::Float);
        let dst = translator.create_value(DynType::Float);

        let ins = match op {
            ArithmeticBinop::Add => Instruction::FAdd(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Sub => Instruction::FSub(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Mul => Instruction::FMul(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Div => Instruction::FMul(Binop { lhs, rhs, dst }),
            ArithmeticBinop::Rem => Instruction::FRem(Binop { lhs, rhs, dst }),
        };

        translator.push_ins(ins);
        return dst;
    }

    let dst = translator.create_value(DynType::Int);
    let ins = match op {
        ArithmeticBinop::Add => Instruction::IAdd(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Sub => Instruction::ISub(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Mul => Instruction::IMul(Binop { lhs, rhs, dst }),
        ArithmeticBinop::Div => Instruction::IMul(Binop { lhs, rhs, dst }),
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
}

#[derive(Clone, Debug)]
struct Param<T> {
    ty: Type,
    value: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DynType {
    None,
    Bool,
    Int,
    Float,
    String,
    /// A partial segment to a value.
    Path(String),
    Func(String),
    PlaceGlobal(String),
}

impl From<&Type> for DynType {
    fn from(value: &Type) -> Self {
        match value {
            Type::Ty(ty) if ty.eq_ignore_ascii_case("int") => Self::Int,
            Type::Ty(ty) if ty.eq_ignore_ascii_case("float") => Self::Float,
            Type::Ty(ty) if ty.eq_ignore_ascii_case("string") => Self::String,
            _ => todo!(),
        }
    }
}

impl DynType {
    fn unwrap_func(&self) -> &str {
        match self {
            Self::Func(f) => f,
            _ => unreachable!(),
        }
    }
}
