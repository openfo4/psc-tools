mod func;
mod lowering;
mod reg;
mod ssa;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{self, Display, Formatter};

use crate::ast::{Expr, Function, Statement, Type};
use crate::codegen::func::{Block, ControlFlowGraph};
use crate::codegen::reg::Register;
use crate::codegen::ssa::{BlockId, SsaBuilder, Value, Var};
use crate::lexer::Literal;

#[derive(Clone, Debug)]
pub enum Instruction<T> {
    AddParams(Vec<Param<T>>),
    Literal(Literal<'static>, T),
    Add(Binop<T>),
    Sub(Binop<T>),
    Mul(Binop<T>),
    Div(Binop<T>),
    Rem(Binop<T>),
    CmpEq(Binop<T>),
    CmpNe(Binop<T>),
    CmpLt(Binop<T>),
    CmpLe(Binop<T>),
    CmpGt(Binop<T>),
    CmpGe(Binop<T>),
    Call(Call),
    Jmp(BlockId, Vec<T>),
    Brif(T, BlockId, Vec<T>, BlockId, Vec<T>),
    Deref(Binop<T>),
    Index(Binop<T>),
    Store(Unop<T>),
    Return(T),
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
struct Call {
    callee: Value,
    args: Vec<Value>,
    returns: Value,
}

#[derive(Clone, Debug, Default)]
struct Tranlsator {
    ssa: SsaBuilder,
    variables: HashMap<String, Var>,
    blocks: HashMap<BlockId, Vec<Instruction<Value>>>,
    cur_block: Option<BlockId>,
}

impl Tranlsator {
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
}

pub fn lower_function(func: &Function) -> ControlFlowGraph<Value> {
    let mut translator = Tranlsator::default();

    let entry = translator.create_block();
    translator.ssa.seal(entry);
    translator.ssa.switch_block(entry);
    translator.cur_block = Some(entry);

    let mut params = Vec::new();
    for param in &func.params {
        let var = translator.get_or_create_var(&param.name);
        let value = translator.ssa.create_value();
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

    let cfg = ControlFlowGraph { entry, blocks };

    let regcfg = reg::ssa_to_reg(&cfg);

    println!("{}", cfg);
    println!("{}", regcfg);

    let func = FuncInfo {
        name: func.name.clone(),
        cfg: regcfg.clone(),
        is_static: func.is_global,
        return_ty: func.return_ty.clone(),
    };

    dbg!("f");

    let f = lowering::lower_to_pex(&func);
    dbg!(&f);

    cfg
}

fn lower_stmt(translator: &mut Tranlsator, stmt: &Statement) {
    match stmt {
        Statement::Define(def) => {}
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
        } => {}
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
            let value = translator.ssa.create_value();
            translator.push_ins(Instruction::Literal(literal.clone(), value));
            value
        }
        Expr::Ident { ident } => {
            let var = translator.get_or_create_var(ident);
            let value = translator.ssa.use_var(var);
            value
        }
        Expr::Add { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Add(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Sub { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Sub(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Mul { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Mul(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Div { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Div(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Rem { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Rem(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpEq { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpEq(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpNe { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpNe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLt { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpLt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpLe { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpLe(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGt { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpGt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::CmpGe { lhs, rhs } => {
            let lhs = lower_expr(translator, lhs);
            let rhs = lower_expr(translator, rhs);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::CmpGt(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Call { callee, args } => {
            let callee = lower_expr(translator, callee);
            let args: Vec<_> = args.iter().map(|arg| lower_expr(translator, arg)).collect();
            let dst = translator.ssa.create_value();
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
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Deref(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Index { array, index } => {
            let lhs = lower_expr(translator, array);
            let rhs = lower_expr(translator, index);
            let dst = translator.ssa.create_value();
            translator.push_ins(Instruction::Index(Binop { lhs, rhs, dst }));
            dst
        }
        Expr::Assign { src, dst } => {
            let src = lower_expr(translator, src);
            store_maybe_place(translator, src, dst);
            translator.ssa.create_value()
        }
        Expr::AddAssign { src, dst } => {
            let lhs = lower_expr(translator, dst);
            let rhs = lower_expr(translator, src);
            let res = translator.ssa.create_value();
            translator.push_ins(Instruction::Add(Binop { lhs, rhs, dst: res }));
            store_maybe_place(translator, res, dst);
            translator.ssa.create_value()
        }
        Expr::SubAssign { src, dst } => {
            let lhs = lower_expr(translator, dst);
            let rhs = lower_expr(translator, src);
            let res = translator.ssa.create_value();
            translator.push_ins(Instruction::Sub(Binop { lhs, rhs, dst: res }));
            store_maybe_place(translator, res, dst);
            translator.ssa.create_value()
        }
        Expr::MulAssign { src, dst } => {
            let lhs = lower_expr(translator, dst);
            let rhs = lower_expr(translator, src);
            let res = translator.ssa.create_value();
            translator.push_ins(Instruction::Mul(Binop { lhs, rhs, dst: res }));
            store_maybe_place(translator, res, dst);
            translator.ssa.create_value()
        }
        Expr::DivAssign { src, dst } => {
            let lhs = lower_expr(translator, dst);
            let rhs = lower_expr(translator, src);
            let res = translator.ssa.create_value();
            translator.push_ins(Instruction::Div(Binop { lhs, rhs, dst: res }));
            store_maybe_place(translator, res, dst);
            translator.ssa.create_value()
        }
        Expr::RemAssign { src, dst } => {
            let lhs = lower_expr(translator, dst);
            let rhs = lower_expr(translator, src);
            let res = translator.ssa.create_value();
            translator.push_ins(Instruction::Rem(Binop { lhs, rhs, dst: res }));
            store_maybe_place(translator, res, dst);
            translator.ssa.create_value()
        }
        x => todo!("{:?}", expr),
    }
}

fn store_maybe_place(translator: &mut Tranlsator, value: Value, dst: &Expr) {
    if let Expr::Ident { ident } = dst {
        let var = translator.get_or_create_var(ident);
        translator.ssa.def_var(var, value);
    } else {
        let dst = lower_expr(translator, dst);
        translator.push_ins(Instruction::Store(Unop { src: value, dst }));
    }
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
