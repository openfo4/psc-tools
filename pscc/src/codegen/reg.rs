use std::collections::HashMap;

use crate::ast::Type;
use crate::codegen::func::{Block, ControlFlowGraph, DataFlowGraph};
use crate::codegen::ssa::Value;
use crate::codegen::{Binop, Call, Callee, Cast, Instruction, Param, PropertyRef, Unop};

pub fn ssa_to_reg(
    cfg: &ControlFlowGraph<Value>,
    value_types: HashMap<Value, Type>,
) -> (ControlFlowGraph<Register>, HashMap<Register, Type>) {
    let dfg = DataFlowGraph::new(cfg);

    let mut regs = RegisterPool::new(value_types);

    let mut blocks = HashMap::new();

    cfg.visit_bfs(|id, block| {
        let mut instructions = Vec::new();

        for (index, ins) in block.instructions.iter().enumerate() {
            // We can retire the source register for the current instruction
            // before we schedule it.
            // This allows the instruction to reuse a source register as a
            // destination register.
            for_each_input_value(ins, |value| {
                let loc = (id, index);
                if dfg.uses(value).last() == Some(&loc) {
                    regs.try_retire(value);
                }
            });

            let mut convert_binop = |op: &Binop<Value>| Binop {
                lhs: regs.get(op.lhs),
                rhs: regs.get(op.rhs),
                dst: regs.alloc(op.dst),
            };

            let out = match ins {
                Instruction::AddParams(params) => {
                    let params = params
                        .iter()
                        .map(|param| {
                            let value = regs.alloc(param.value);

                            // If there are no instructions actually using the parameter
                            // we can immediately retire it again.
                            // Dead parameters are not removed by the optimizer because
                            // it would change the function signature.
                            if dfg.uses(param.value).is_empty() {
                                regs.try_retire(param.value);
                            }

                            Param {
                                ty: param.ty.clone(),
                                value,
                            }
                        })
                        .collect();
                    Instruction::AddParams(params)
                }
                Instruction::Literal(lit, dst) => {
                    Instruction::Literal(lit.clone(), regs.alloc(*dst))
                }
                Instruction::IAdd(op) => Instruction::IAdd(convert_binop(op)),
                Instruction::ISub(op) => Instruction::ISub(convert_binop(op)),
                Instruction::IMul(op) => Instruction::IMul(convert_binop(op)),
                Instruction::IDiv(op) => Instruction::IDiv(convert_binop(op)),
                Instruction::IRem(op) => Instruction::IRem(convert_binop(op)),
                Instruction::FAdd(op) => Instruction::FAdd(convert_binop(op)),
                Instruction::FSub(op) => Instruction::FSub(convert_binop(op)),
                Instruction::FMul(op) => Instruction::FMul(convert_binop(op)),
                Instruction::FDiv(op) => Instruction::FDiv(convert_binop(op)),
                Instruction::FRem(op) => Instruction::FRem(convert_binop(op)),
                Instruction::CmpEq(op) => Instruction::CmpEq(convert_binop(op)),
                Instruction::CmpNe(op) => Instruction::CmpNe(convert_binop(op)),
                Instruction::CmpLt(op) => Instruction::CmpLt(convert_binop(op)),
                Instruction::CmpLe(op) => Instruction::CmpLe(convert_binop(op)),
                Instruction::CmpGt(op) => Instruction::CmpGt(convert_binop(op)),
                Instruction::CmpGe(op) => Instruction::CmpGe(convert_binop(op)),
                Instruction::StringConcat(op) => Instruction::StringConcat(convert_binop(op)),
                Instruction::Call(op) => {
                    let args = op.args.iter().map(|arg| regs.get(*arg)).collect();

                    Instruction::Call(Call {
                        callee: match op.callee.clone() {
                            Callee::Local { object, func } => Callee::Local {
                                object: regs.get(object),
                                func,
                            },
                            Callee::Static { class, func } => Callee::Static { class, func },
                            Callee::LocalSelf { func } => Callee::LocalSelf { func },
                        },
                        args,
                        returns: regs.alloc(op.returns),
                    })
                }
                Instruction::Return(value) => Instruction::Return(regs.get(*value)),
                Instruction::Brif(
                    condition,
                    then_block_id,
                    then_args,
                    else_block_id,
                    else_args,
                ) => {
                    // The condition MUST be read before copying the block arguments,
                    // since the condition register may be retired already.
                    let condition = regs.get(*condition);

                    // We assume that the input CFG has a minimal and pruned,
                    // therefore if a block follows that has arguments we can
                    // assume that the edge is always critical (Otherwise no
                    // arguments would be passed).
                    let then_block = &cfg.blocks[then_block_id];
                    for (src, dst) in then_args.iter().zip(&then_block.params) {
                        let src = regs.get(*src);
                        if let Err(dst) = regs.try_alloc_preferred_or_get(*dst, src) {
                            instructions.push(Instruction::Store(Unop { src, dst }));
                        }
                    }

                    let else_block = &cfg.blocks[else_block_id];
                    for (src, dst) in else_args.iter().zip(&else_block.params) {
                        let src = regs.get(*src);
                        if let Err(dst) = regs.try_alloc_preferred_or_get(*dst, src) {
                            instructions.push(Instruction::Store(Unop { src, dst }));
                        }
                    }

                    Instruction::Brif(
                        condition,
                        *then_block_id,
                        Vec::new(),
                        *else_block_id,
                        Vec::new(),
                    )
                }
                Instruction::Jmp(block_id, args) => {
                    let block = &cfg.blocks[block_id];
                    for (src, dst) in args.iter().zip(&block.params) {
                        let src = regs.get(*src);
                        if let Err(dst) = regs.try_alloc_preferred_or_get(*dst, src) {
                            instructions.push(Instruction::Store(Unop { src, dst }));
                        }
                    }

                    Instruction::Jmp(*block_id, Vec::new())
                }
                Instruction::LoadGlobal(name, dst) => {
                    let reg = regs.alloc(*dst);
                    Instruction::LoadGlobal(name.clone(), reg)
                }
                Instruction::Cast(op) => Instruction::Cast(Cast {
                    src: regs.get(op.src),
                    dst: regs.alloc(op.dst),
                    src_ty: op.src_ty.clone(),
                    dst_ty: op.dst_ty.clone(),
                }),
                Instruction::StoreGlobal(global, src) => {
                    Instruction::StoreGlobal(global.clone(), regs.get(*src))
                }
                Instruction::GetProperty(prop, dst) => match prop {
                    PropertyRef::Local(prop) => {
                        Instruction::GetProperty(PropertyRef::Local(prop.clone()), regs.alloc(*dst))
                    }
                    PropertyRef::Other(object, prop) => Instruction::GetProperty(
                        PropertyRef::Other(regs.get(*object), prop.clone()),
                        regs.alloc(*dst),
                    ),
                },
                Instruction::SetProperty(prop, src) => match prop {
                    PropertyRef::Local(prop) => {
                        Instruction::SetProperty(PropertyRef::Local(prop.clone()), regs.get(*src))
                    }
                    PropertyRef::Other(object, prop) => Instruction::SetProperty(
                        PropertyRef::Other(regs.get(*object), prop.clone()),
                        regs.get(*src),
                    ),
                },
                x => todo!("{:?}", x),
            };

            instructions.push(out);
        }

        blocks.insert(
            id,
            Block {
                params: Vec::new(),
                instructions,
            },
        );
    });

    let mut register_types = HashMap::new();
    for (ty, regs) in regs.retired {
        for reg in regs {
            register_types.insert(reg, ty.clone());
        }
    }
    for (val, reg) in regs.registers {
        let ty = regs.value_types[&val].clone();
        register_types.insert(reg, ty);
    }

    (
        ControlFlowGraph {
            blocks,
            entry: cfg.entry,
        },
        register_types,
    )
}

#[derive(Clone, Debug, Default)]
struct RegisterPool {
    next: usize,
    registers: HashMap<Value, Register>,
    retired: HashMap<Type, Vec<Register>>,
    value_types: HashMap<Value, Type>,
}

impl RegisterPool {
    fn new(value_types: HashMap<Value, Type>) -> Self {
        Self {
            next: 0,
            registers: HashMap::new(),
            retired: HashMap::new(),
            value_types,
        }
    }

    fn alloc(&mut self, value: Value) -> Register {
        debug_assert!(!self.registers.contains_key(&value));

        let ty = &self.value_types[&value];
        if let Some(regs) = self.retired.get_mut(ty)
            && let Some(reg) = regs.pop()
        {
            self.registers.insert(value, reg);
            return reg;
        }

        let id = Register(self.next);
        self.next += 1;
        self.registers.insert(value, id);
        id
    }

    fn try_retire(&mut self, value: Value) {
        if let Some(reg) = self.registers.get(&value) {
            let ty = &self.value_types[&value];
            self.retired.entry(ty.clone()).or_default().push(*reg);
        }
    }

    fn get(&self, value: Value) -> Register {
        self.registers[&value]
    }

    fn try_alloc_preferred_or_get(
        &mut self,
        value: Value,
        preferred: Register,
    ) -> Result<(), Register> {
        match self.registers.get(&value) {
            Some(reg) => Err(*reg),
            None => self.try_alloc_preferred(value, preferred),
        }
    }

    /// Allocates a new [`Register`] for the given [`Value`].
    ///
    /// If possible the new register will be `preferred`, otherwise `Err(..)` with a new register
    /// is returned.
    fn try_alloc_preferred(&mut self, value: Value, preferred: Register) -> Result<(), Register> {
        let ty = &self.value_types[&value];

        let mut preferred_usable = false;
        if let Some(regs) = self.retired.get_mut(ty) {
            regs.retain(|reg| {
                if *reg == preferred {
                    preferred_usable = true;
                    false
                } else {
                    true
                }
            });
        }

        if preferred_usable {
            self.registers.insert(value, preferred);
            Ok(())
        } else {
            let reg = self.alloc(value);
            Err(reg)
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Register(usize);

impl std::fmt::Display for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

fn for_each_input_value<F>(ins: &Instruction<Value>, mut f: F)
where
    F: FnMut(Value),
{
    let mut binop = |op: &Binop<Value>| {
        f(op.lhs);
        f(op.rhs);
    };

    match ins {
        Instruction::AddParams(_) => {}
        Instruction::Literal(_, _) => {}
        Instruction::IAdd(op)
        | Instruction::ISub(op)
        | Instruction::IMul(op)
        | Instruction::IDiv(op)
        | Instruction::IRem(op)
        | Instruction::FAdd(op)
        | Instruction::FSub(op)
        | Instruction::FMul(op)
        | Instruction::FDiv(op)
        | Instruction::FRem(op)
        | Instruction::CmpEq(op)
        | Instruction::CmpNe(op)
        | Instruction::CmpLt(op)
        | Instruction::CmpLe(op)
        | Instruction::CmpGt(op)
        | Instruction::CmpGe(op)
        | Instruction::StringConcat(op)
        | Instruction::Index(op) => binop(op),
        Instruction::Brif(condition, _, then_args, _, else_args) => {
            f(*condition);
            for arg in then_args.iter().chain(else_args) {
                f(*arg);
            }
        }
        Instruction::Jmp(_, args) => {
            for arg in args {
                f(*arg);
            }
        }
        Instruction::Return(value) => f(*value),
        Instruction::Call(op) => {
            for arg in &op.args {
                f(*arg);
            }
        }
        Instruction::LoadGlobal(_, _) => {}
        Instruction::Cast(op) => {
            f(op.src);
        }
        Instruction::StoreGlobal(_, src) => {
            f(*src);
        }
        Instruction::GetProperty(prop, _) => match prop {
            PropertyRef::Local(_) => {}
            PropertyRef::Other(value, _) => f(*value),
        },
        Instruction::SetProperty(prop, src) => {
            f(*src);
            match prop {
                PropertyRef::Local(_) => {}
                PropertyRef::Other(value, _) => f(*value),
            }
        }
        x => todo!("{:?}", x),
    }
}
