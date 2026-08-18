use std::collections::{HashMap, HashSet, VecDeque};
use std::hint::black_box;

use crate::codegen::func::{Block, DataFlowGraph};
use crate::codegen::reg::Register;
use crate::codegen::ssa::BlockId;
use crate::codegen::{ControlFlowGraph, FuncInfo, Instruction, ssa};
use crate::lexer::Literal;

use pex::{ConditionalBranch, Instruction as PexInstruction, Unop, Value, VariableDefinition};

pub fn lower_to_pex(func: &FuncInfo) -> pex::Function {
    let mut params = Vec::new();

    let mut branches = Vec::new();

    let mut blocks = HashMap::new();

    let schedule = schedule_blocks(&func.cfg);
    let mut schedule_succ = HashMap::new();
    for [src, dst] in schedule.array_windows::<2>() {
        schedule_succ.insert(*src, *dst);
    }

    dbg!(&schedule);

    func.cfg.visit_bfs(|id, block| {
        let mut instructions = Vec::new();

        for ins in &block.instructions {
            let out = match ins {
                Instruction::AddParams(p) => {
                    for p in p {
                        params.push(pex::VariableDefinition {
                            name: p.value.to_string(),
                            ty: p.ty.to_string(),
                        });
                    }
                    continue;
                }
                // We always directly embed literals as immediate values in the
                // instructions where they are used.
                // The literal values themselves are never used.
                Instruction::Literal(lit, dst) => pex::Instruction::Store(Unop {
                    src: match lit {
                        Literal::NumberInt(value) => Value::Int(*value),
                        _ => todo!(),
                    },
                    dst: Value::Ident(dst.to_string()),
                }),
                Instruction::Store(op) => pex::Instruction::Store(pex::Unop {
                    src: Value::Ident(op.src.to_string()),
                    dst: Value::Ident(op.dst.to_string()),
                }),
                Instruction::Add(op) => pex::Instruction::IAdd(pex::Binop {
                    lhs: Value::Ident(op.lhs.to_string()),
                    rhs: Value::Ident(op.rhs.to_string()),
                    dst: Value::Ident(op.dst.to_string()),
                }),
                Instruction::CmpEq(op) => PexInstruction::CmpEq(pex::Binop {
                    lhs: Value::Ident(op.lhs.to_string()),
                    rhs: Value::Ident(op.rhs.to_string()),
                    dst: Value::Ident(op.dst.to_string()),
                }),
                Instruction::Return(value) => {
                    PexInstruction::Return(Value::Ident(value.to_string()))
                }
                Instruction::Brif(condition, then_block, then_args, else_block, else_args) => {
                    debug_assert!(then_args.is_empty());
                    debug_assert!(else_args.is_empty());

                    let condition = Value::Ident(condition.to_string());

                    let succ = schedule_succ.get(&id).unwrap();
                    if *succ == *then_block {
                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *else_block,
                        });

                        pex::Instruction::BranchIfFalse(ConditionalBranch {
                            condition,
                            offset: 0,
                        })
                    } else if *succ == *else_block {
                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *then_block,
                        });

                        pex::Instruction::BranchIfTrue(ConditionalBranch {
                            condition,
                            offset: 0,
                        })
                    } else {
                        todo!();
                    }
                }
                Instruction::Jmp(block, args) => {
                    debug_assert!(args.is_empty());

                    let succ = schedule_succ.get(&id).unwrap();
                    if *succ == *block {
                        continue;
                    }

                    branches.push(BranchReloc {
                        block: id,
                        instruction: instructions.len(),
                        target: *block,
                    });

                    pex::Instruction::Branch(pex::Branch { offset: 0 })
                }
                _ => {
                    continue;
                }
            };

            instructions.push(out);
        }

        blocks.insert(id, instructions);
    });

    let mut instructions = Vec::new();
    let mut block_offsets = HashMap::new();
    for id in schedule {
        block_offsets.insert(id, instructions.len());
        let block = blocks.remove(&id).unwrap();
        instructions.extend(block);
    }

    for branch in branches {
        let source = block_offsets[&branch.block] + branch.instruction;
        let target = block_offsets[&branch.target];
        let offset = target as i32 - source as i32;

        match &mut instructions[source] {
            pex::Instruction::Branch(op) => op.offset = offset,
            pex::Instruction::BranchIfTrue(op) | pex::Instruction::BranchIfFalse(op) => {
                op.offset = offset
            }
            _ => unreachable!(),
        }
    }

    // for id in schedule_blocks(&func.cfg) {
    //     let block = blocks.remove(&id).unwrap();
    //     instructions.extend(block);
    // }

    let return_ty = func
        .return_ty
        .as_ref()
        .map(|ty| ty.to_string())
        .unwrap_or_else(|| "none".to_string());

    pex::Function {
        name: func.name.clone(),
        is_static: func.is_static,
        is_native: false,
        locals: Vec::new(),
        params,
        return_ty,
        instructions,
    }
}

#[derive(Copy, Clone, Debug)]
struct BranchReloc {
    block: BlockId,
    /// The output instruction containing the branch.
    instruction: usize,
    /// The target of the branch instruction.
    target: BlockId,
}

fn schedule_blocks<T>(cfg: &ControlFlowGraph<T>) -> Vec<BlockId> {
    let mut blocks = Vec::new();
    let mut scheduled = HashSet::new();

    let mut queue: VecDeque<_> = vec![cfg.entry].into();
    while let Some(id) = queue.pop_front() {
        if !scheduled.insert(id) {
            continue;
        }

        let block = &cfg.blocks[&id];
        blocks.push(id);

        match block.instructions.last() {
            Some(Instruction::Jmp(block, _)) => {
                queue.push_back(*block);
            }
            Some(Instruction::Brif(_, then_block, _, else_block, _)) => {
                // Option 1: If any block is a back-edge to a block we already
                // scheduled we must always emit a jump to reach it.
                // Always schedule it first.
                if !scheduled.contains(then_block) {
                    queue.push_front(*else_block);
                    queue.push_front(*then_block);
                    continue;
                }

                if !scheduled.contains(else_block) {
                    queue.push_front(*then_block);
                    queue.push_front(*else_block);
                    continue;
                }

                // Option 2: If any block contains a subgraph with an edge back
                // to our current block we schedule it next.
                // This typically happens for while loops and we want to expand
                // the complete while loop body before continuing.
                if has_edge_to_self(cfg, &scheduled, *then_block) {
                    queue.push_front(*else_block);
                    queue.push_front(*then_block);
                    continue;
                }

                if has_edge_to_self(cfg, &scheduled, *else_block) {
                    queue.push_front(*then_block);
                    queue.push_front(*else_block);
                    continue;
                }

                // Option 3: The remaining subgraph is now acyclic.
                // If any block always passes through the other block, we schedule
                // that block first.
                // This typically happens for if branches without an else branch.
                if passes_through(cfg, *then_block, *else_block, &scheduled) {
                    queue.push_front(*else_block);
                    queue.push_front(*then_block);
                    continue;
                }

                // If `else_block` passes through `then_block` this is the correct
                // schedule.
                // Otherwise we have a unrelated blocks where both orderings result
                // in the same number of jumps.
                // This typically happens for an if branch with an else branch.
                queue.push_front(*then_block);
                queue.push_front(*else_block);
            }
            _ => {}
        }
    }

    blocks
}

fn has_edge_to_self<T>(
    cfg: &ControlFlowGraph<T>,
    exclude: &HashSet<BlockId>,
    block: BlockId,
) -> bool {
    let mut queue = vec![block];
    let mut visited = HashSet::new();

    while let Some(id) = queue.pop() {
        if !visited.insert(id) {
            continue;
        }

        if exclude.contains(&id) {
            continue;
        }

        for succ in cfg.succ(id) {
            if succ == block {
                return true;
            }

            queue.push(succ);
        }
    }

    false
}

fn passes_through<T>(
    cfg: &ControlFlowGraph<T>,
    block: BlockId,
    target: BlockId,
    exclude: &HashSet<BlockId>,
) -> bool {
    let mut queue = vec![block];
    let mut visited = HashSet::new();

    while let Some(id) = queue.pop() {
        if !visited.insert(id) {
            continue;
        }

        if exclude.contains(&id) {
            continue;
        }

        for succ in cfg.succ(id) {
            if succ == target {
                return true;
            }

            queue.push(succ);
        }
    }

    false
}
