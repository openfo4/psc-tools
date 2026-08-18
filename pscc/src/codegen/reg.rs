use std::collections::HashMap;

use crate::codegen::func::{Block, ControlFlowGraph, DataFlowGraph};
use crate::codegen::ssa::Value;
use crate::codegen::{Binop, Instruction, Param, Unop};

pub fn ssa_to_reg(cfg: &ControlFlowGraph<Value>) -> ControlFlowGraph<Register> {
    let dfg = DataFlowGraph::new(cfg);

    let mut regs = RegisterPool::default();

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

            let out = match ins {
                Instruction::AddParams(params) => {
                    let params = params
                        .iter()
                        .map(|param| Param {
                            ty: param.ty.clone(),
                            value: regs.alloc(param.value),
                        })
                        .collect();
                    Instruction::AddParams(params)
                }
                Instruction::Literal(lit, dst) => {
                    Instruction::Literal(lit.clone(), regs.alloc(*dst))
                }
                Instruction::Add(op) => Instruction::Add(Binop {
                    lhs: regs.get(op.lhs),
                    rhs: regs.get(op.rhs),
                    dst: regs.alloc(op.dst),
                }),
                Instruction::CmpEq(op) => Instruction::CmpEq(Binop {
                    lhs: regs.get(op.lhs),
                    rhs: regs.get(op.rhs),
                    dst: regs.alloc(op.dst),
                }),
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

    ControlFlowGraph {
        blocks,
        entry: cfg.entry,
    }
}

#[derive(Clone, Debug, Default)]
struct RegisterPool {
    next: usize,
    registers: HashMap<Value, Register>,
    retired: Vec<Register>,
}

impl RegisterPool {
    fn alloc(&mut self, value: Value) -> Register {
        if let Some(reg) = self.retired.pop() {
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
            self.retired.push(*reg);
        }
    }

    fn get(&self, value: Value) -> Register {
        self.registers[&value]
    }

    fn get_or_alloc(&mut self, value: Value) -> Register {
        match self.registers.get(&value) {
            Some(reg) => *reg,
            None => self.alloc(value),
        }
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
        let mut preferred_usable = false;
        self.retired.retain(|reg| {
            if *reg == preferred {
                preferred_usable = true;
                false
            } else {
                true
            }
        });

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
        Instruction::Add(op) => binop(op),
        Instruction::CmpEq(op) => binop(op),
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
        x => todo!("{:?}", x),
    }
}
