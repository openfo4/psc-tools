use std::collections::{HashMap, HashSet, VecDeque};

use crate::StringBuilder;
use crate::codegen::reg::Register;
use crate::codegen::ssa::BlockId;
use crate::codegen::{Binop, Callee, ControlFlowGraph, FuncInfo, Instruction, PropertyRef, Unop};
use crate::lexer::Literal;

use pex::instruction::{
    ArrayGet, ArraySet, CallMethod, CallStatic, ConditionalJump, Jump, PropertyGet, PropertySet,
};
use pex::{Value, VariableDefinition, instruction as pexins};

pub fn lower_to_pex(func: &FuncInfo, strings: &mut StringBuilder) -> pex::Function {
    let mut params = Vec::new();
    let mut locals = Vec::new();

    let mut branches = Vec::new();

    let mut blocks = HashMap::new();

    let schedule = schedule_blocks(&func.cfg);
    let mut schedule_succ = HashMap::new();
    for [src, dst] in schedule.array_windows::<2>() {
        schedule_succ.insert(*src, *dst);
    }

    let mut reg_names = HashMap::new();
    for (reg, _) in &func.regtys {
        reg_names.insert(*reg, strings.insert(&reg.to_string()));
    }

    func.cfg.visit_bfs(|id, block| {
        let mut instructions = Vec::new();

        let convert_binop = |op: &Binop<Register>| pex::instruction::Binop {
            lhs: Value::Ident(reg_names[&op.lhs]),
            rhs: Value::Ident(reg_names[&op.rhs]),
            dst: Value::Ident(reg_names[&op.dst]),
        };

        let convert_unop = |op: &Unop<Register>| pex::instruction::Unop {
            src: Value::Ident(reg_names[&op.src]),
            dst: Value::Ident(reg_names[&op.dst]),
        };

        for ins in &block.instructions {
            let out = match ins {
                Instruction::AddParams(p) => {
                    let mut local_regs: HashSet<_> = func.regtys.keys().copied().collect();

                    for p in p {
                        local_regs.remove(&p.value);

                        params.push(pex::VariableDefinition {
                            name: reg_names[&p.value],
                            ty: strings.insert(&p.ty.to_string()),
                        });
                    }

                    for reg in local_regs {
                        let ty = &func.regtys[&reg];
                        locals.push(VariableDefinition {
                            name: reg_names[&reg],
                            ty: strings.insert(&ty.to_string()),
                        })
                    }

                    continue;
                }
                // We always directly embed literals as immediate values in the
                // instructions where they are used.
                // The literal values themselves are never used.
                Instruction::Literal(lit, dst) => {
                    pexins::Instruction::Store(pex::instruction::Unop {
                        src: match lit {
                            Literal::None => Value::Null,
                            Literal::Bool(value) => Value::Bool(*value),
                            Literal::NumberInt(value) => Value::Int(*value),
                            _ => todo!(),
                        },
                        dst: Value::Ident(reg_names[dst]),
                    })
                }
                Instruction::Store(op) => pexins::Instruction::Store(pex::instruction::Unop {
                    src: Value::Ident(reg_names[&op.src]),
                    dst: Value::Ident(reg_names[&op.dst]),
                }),
                Instruction::IAdd(op) => pexins::Instruction::IAdd(convert_binop(op)),
                Instruction::ISub(op) => pexins::Instruction::ISub(convert_binop(op)),
                Instruction::IMul(op) => pexins::Instruction::IMul(convert_binop(op)),
                Instruction::IDiv(op) => pexins::Instruction::IDiv(convert_binop(op)),
                Instruction::IRem(op) => pexins::Instruction::IMod(convert_binop(op)),
                Instruction::INeg(op) => pexins::Instruction::INeg(convert_unop(op)),
                Instruction::FAdd(op) => pexins::Instruction::FAdd(convert_binop(op)),
                Instruction::FSub(op) => pexins::Instruction::FSub(convert_binop(op)),
                Instruction::FMul(op) => pexins::Instruction::FMul(convert_binop(op)),
                Instruction::FDiv(op) => pexins::Instruction::FDiv(convert_binop(op)),
                Instruction::FRem(op) => todo!(),
                Instruction::FNeg(op) => pexins::Instruction::FNeg(convert_unop(op)),
                Instruction::StringConcat(op) => {
                    pexins::Instruction::StringConcat(convert_binop(op))
                }
                Instruction::BNot(op) => pexins::Instruction::Not(convert_unop(op)),
                Instruction::CmpEq(op) => pexins::Instruction::CmpEq(convert_binop(op)),
                Instruction::CmpNe(op) => todo!(),
                Instruction::CmpLt(op) => pexins::Instruction::CmpLt(convert_binop(op)),
                Instruction::CmpLe(op) => pexins::Instruction::CmpLe(convert_binop(op)),
                Instruction::CmpGt(op) => pexins::Instruction::CmpGt(convert_binop(op)),
                Instruction::CmpGe(op) => pexins::Instruction::CmpGe(convert_binop(op)),
                Instruction::Return(value) => {
                    pexins::Instruction::Return(Value::Ident(reg_names[value]))
                }
                Instruction::Brif(condition, then_block, then_args, else_block, else_args) => {
                    debug_assert!(then_args.is_empty());
                    debug_assert!(else_args.is_empty());

                    let condition = Value::Ident(reg_names[condition]);

                    let succ = schedule_succ.get(&id).unwrap();
                    if *succ == *then_block {
                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *else_block,
                        });

                        pexins::Instruction::JumpIfFalse(ConditionalJump {
                            condition,
                            offset: 0,
                        })
                    } else if *succ == *else_block {
                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *then_block,
                        });

                        pexins::Instruction::JumpIfTrue(ConditionalJump {
                            condition,
                            offset: 0,
                        })
                    } else {
                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *then_block,
                        });
                        instructions.push(pexins::Instruction::JumpIfTrue(ConditionalJump {
                            condition,
                            offset: 0,
                        }));

                        branches.push(BranchReloc {
                            block: id,
                            instruction: instructions.len(),
                            target: *else_block,
                        });

                        pexins::Instruction::Jump(Jump { offset: 0 })
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

                    pexins::Instruction::Jump(Jump { offset: 0 })
                }
                Instruction::LoadGlobal(global, dst) => {
                    pexins::Instruction::Store(pex::instruction::Unop {
                        src: Value::Ident(strings.insert(global)),
                        dst: Value::Ident(reg_names[dst]),
                    })
                }
                Instruction::StoreGlobal(global, src) => {
                    pexins::Instruction::Store(pex::instruction::Unop {
                        src: Value::Ident(reg_names[src]),
                        dst: Value::Ident(strings.insert(global)),
                    })
                }
                Instruction::Call(op) => {
                    let args = op
                        .args
                        .iter()
                        .map(|arg| Value::Ident(reg_names[arg]))
                        .collect();
                    let return_assignment = Value::Ident(reg_names[&op.returns]);

                    match &op.callee {
                        Callee::Local { object, func } => {
                            pexins::Instruction::CallMethod(CallMethod {
                                function_name: strings.insert(func),
                                object: Value::Ident(reg_names[object]),
                                return_assignment,
                                args,
                            })
                        }
                        Callee::Static { class, func } => {
                            pexins::Instruction::CallStatic(CallStatic {
                                class: strings.insert(class),
                                function: strings.insert(func),
                                args,
                                return_assignment,
                            })
                        }
                        Callee::LocalSelf { func } => pexins::Instruction::CallMethod(CallMethod {
                            function_name: strings.insert(func),
                            object: Value::Ident(strings.insert("self")),
                            return_assignment,
                            args,
                        }),
                    }
                }
                Instruction::GetProperty(prop, dst) => match prop {
                    PropertyRef::Local(name) => pexins::Instruction::PropertyGet(PropertyGet {
                        property: strings.insert(name),
                        value: Value::Ident(strings.insert("self")),
                        return_assignment: Value::Ident(reg_names[dst]),
                    }),
                    PropertyRef::Other(object, name) => {
                        pexins::Instruction::PropertyGet(PropertyGet {
                            property: strings.insert(name),
                            value: Value::Ident(strings.insert(&object.to_string())),
                            return_assignment: Value::Ident(reg_names[dst]),
                        })
                    }
                },
                Instruction::SetProperty(prop, src) => match prop {
                    PropertyRef::Local(name) => pexins::Instruction::PropertySet(PropertySet {
                        property: strings.insert(name),
                        object: Value::Ident(strings.insert("self")),
                        value: Value::Ident(reg_names[src]),
                    }),
                    PropertyRef::Other(object, name) => {
                        pexins::Instruction::PropertySet(PropertySet {
                            property: strings.insert(name),
                            object: Value::Ident(strings.insert(&object.to_string())),
                            value: Value::Ident(reg_names[src]),
                        })
                    }
                },
                Instruction::ArrayLen(op) => pexins::Instruction::ArrayLength(convert_unop(op)),
                Instruction::ArrayLoad(op) => pexins::Instruction::ArrayGet(ArrayGet {
                    array: Value::Ident(reg_names[&op.array]),
                    index: Value::Ident(reg_names[&op.index]),
                    return_assignment: Value::Ident(reg_names[&op.dst]),
                }),
                Instruction::ArrayStore(op) => pexins::Instruction::ArraySet(ArraySet {
                    array: Value::Ident(reg_names[&op.array]),
                    index: Value::Ident(reg_names[&op.index]),
                    value: Value::Ident(reg_names[&op.value]),
                }),
                Instruction::ArrayAlloc(op) => pexins::Instruction::ArrayCreate(convert_unop(op)),
                Instruction::Cast(op) => pexins::Instruction::Cast(pex::instruction::Unop {
                    src: Value::Ident(reg_names[&op.src]),
                    dst: Value::Ident(reg_names[&op.dst]),
                }),
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
            pexins::Instruction::Jump(op) => op.offset = offset,
            pexins::Instruction::JumpIfTrue(op) | pexins::Instruction::JumpIfFalse(op) => {
                op.offset = offset
            }
            _ => unreachable!(),
        }
    }

    let return_ty = func
        .return_ty
        .as_ref()
        .map(|ty| ty.to_string())
        .unwrap_or_else(|| "none".to_string());

    pex::Function {
        is_static: func.is_static,
        is_native: false,
        locals,
        params,
        return_ty: strings.insert(&return_ty),
        instructions,
        docstring: strings.insert(""),
        user_flags: 0,
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
