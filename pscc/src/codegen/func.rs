use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{self, Display, Formatter};

use crate::codegen::Instruction;
use crate::codegen::ssa::{BlockId, Value};

#[derive(Clone, Debug)]
pub struct ControlFlowGraph<T> {
    pub entry: BlockId,
    pub blocks: HashMap<BlockId, Block<T>>,
}

impl<T> ControlFlowGraph<T> {
    pub fn visit_bfs<F>(&self, mut visitor: F)
    where
        F: FnMut(BlockId, &Block<T>),
    {
        let mut queue: VecDeque<_> = vec![self.entry].into();
        while let Some(id) = queue.pop_front() {
            let block = &self.blocks[&id];
            visitor(id, block);

            for ins in &block.instructions {
                match ins {
                    Instruction::Brif(_, then_block, _, else_block, _) => {
                        queue.push_back(*then_block);
                        queue.push_back(*else_block);
                    }
                    Instruction::Jmp(block, _) => {
                        queue.push_back(*block);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn succ(&self, block: BlockId) -> impl Iterator<Item = BlockId> {
        match self.blocks[&block].instructions.last() {
            Some(Instruction::Jmp(block, _)) => vec![*block].into_iter(),
            Some(Instruction::Brif(_, then_block, _, else_block, _)) => {
                vec![*then_block, *else_block].into_iter()
            }
            Some(Instruction::Return(_)) => vec![].into_iter(),
            _ => unreachable!(),
        }
    }
}

impl<T> Display for ControlFlowGraph<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut queue: VecDeque<_> = vec![self.entry].into();
        let mut visited = HashSet::new();

        while let Some(id) = queue.pop_front() {
            if !visited.insert(id) {
                continue;
            }

            let block = &self.blocks[&id];

            if block.params.is_empty() {
                writeln!(f, "{}:", id)?;
            } else {
                let params = block
                    .params
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(f, "{}({}):", id, params)?;
            }

            for instruction in &block.instructions {
                write!(f, "  ")?;

                match instruction {
                    Instruction::AddParams(params) => {
                        let params = params
                            .iter()
                            .map(|p| p.value.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");

                        writeln!(f, "params {}", params)?;
                    }
                    Instruction::Literal(lit, dst) => writeln!(f, "{} = const {:?}", dst, lit)?,
                    Instruction::Add(op) => writeln!(f, "{} = add {} {}", op.dst, op.lhs, op.rhs)?,
                    Instruction::Sub(op) => writeln!(f, "{} = sub {} {}", op.dst, op.lhs, op.rhs)?,
                    Instruction::Mul(op) => writeln!(f, "{} = mul {} {}", op.dst, op.lhs, op.rhs)?,
                    Instruction::Div(op) => writeln!(f, "{} = div {} {}", op.dst, op.lhs, op.rhs)?,
                    Instruction::Rem(op) => writeln!(f, "{} = rem {} {}", op.dst, op.lhs, op.rhs)?,
                    Instruction::CmpEq(op) => {
                        writeln!(f, "{} = cmpeq {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::CmpNe(op) => {
                        writeln!(f, "{} = cmpne {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::CmpLt(op) => {
                        writeln!(f, "{} = cmplt {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::CmpLe(op) => {
                        writeln!(f, "{} = cmple {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::CmpGt(op) => {
                        writeln!(f, "{} = cmpgt {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::CmpGe(op) => {
                        writeln!(f, "{} = cmpge {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::Deref(op) => {
                        writeln!(f, "{} = deref {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::Index(op) => {
                        writeln!(f, "{} = index {} {}", op.dst, op.lhs, op.rhs)?
                    }
                    Instruction::Brif(cond, then_block, then_args, else_block, else_args) => {
                        let then_args = then_args
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        let else_args = else_args
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");

                        writeln!(
                            f,
                            "brif {} {}({}) {}({})",
                            cond, then_block, then_args, else_block, else_args
                        )?;
                        queue.push_back(*then_block);
                        queue.push_back(*else_block);
                    }
                    Instruction::Jmp(block, args) => {
                        let args = args
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        writeln!(f, "jmp {}({})", block, args)?;
                        queue.push_back(*block);
                    }
                    Instruction::Call(op) => {
                        let args = op
                            .args
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(" ");

                        writeln!(f, "{} = call {} {}", op.returns, op.callee, args)?;
                    }
                    Instruction::Store(op) => {
                        writeln!(f, "{} = {}", op.dst, op.src)?;
                    }
                    Instruction::Return(value) => writeln!(f, "ret {}", value)?,
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Block<T> {
    pub params: Vec<T>,
    pub instructions: Vec<Instruction<T>>,
}

#[derive(Clone, Debug)]
pub struct DataFlowGraph {
    defs: HashMap<Value, (BlockId, usize)>,
    uses: HashMap<Value, Vec<(BlockId, usize)>>,
}

impl DataFlowGraph {
    pub fn new(cfg: &ControlFlowGraph<Value>) -> Self {
        let mut defs = HashMap::new();
        let mut uses = HashMap::<_, Vec<_>>::new();

        dbg!(&cfg);

        cfg.visit_bfs(|id, block| {
            for value in &block.params {
                defs.insert(*value, (id, usize::MAX));
            }

            for (index, ins) in block.instructions.iter().enumerate() {
                let loc = (id, index);

                match ins {
                    Instruction::AddParams(params) => {
                        for param in params {
                            defs.insert(param.value, loc);
                        }
                    }
                    Instruction::Literal(_, dst) => {
                        defs.insert(*dst, loc);
                    }
                    Instruction::Add(op)
                    | Instruction::Sub(op)
                    | Instruction::Mul(op)
                    | Instruction::Div(op)
                    | Instruction::Rem(op)
                    | Instruction::CmpEq(op)
                    | Instruction::CmpNe(op)
                    | Instruction::CmpLt(op)
                    | Instruction::CmpLe(op)
                    | Instruction::CmpGt(op)
                    | Instruction::CmpGe(op)
                    | Instruction::Deref(op)
                    | Instruction::Index(op) => {
                        uses.entry(op.lhs).or_default().push(loc);
                        uses.entry(op.rhs).or_default().push(loc);
                        defs.insert(op.dst, loc);
                    }
                    Instruction::Store(op) => {
                        uses.entry(op.src).or_default().push(loc);
                        defs.insert(op.dst, loc);
                    }
                    Instruction::Call(op) => {
                        uses.entry(op.callee).or_default().push(loc);
                        for arg in &op.args {
                            uses.entry(*arg).or_default().push(loc);
                        }
                        defs.insert(op.returns, loc);
                    }
                    Instruction::Return(val) => {
                        uses.entry(*val).or_default().push(loc);
                    }
                    Instruction::Jmp(_, args) => {
                        for arg in args {
                            uses.entry(*arg).or_default().push(loc);
                        }
                    }
                    Instruction::Brif(condition, _, then_args, _, else_args) => {
                        uses.entry(*condition).or_default().push(loc);
                        for arg in then_args.iter().chain(else_args) {
                            uses.entry(*arg).or_default().push(loc);
                        }
                    }
                }
            }
        });

        Self { defs, uses }
    }

    pub fn def(&self, value: Value) -> (BlockId, usize) {
        self.defs[&value]
    }

    pub fn uses(&self, value: Value) -> &[(BlockId, usize)] {
        &self.uses[&value]
    }
}

#[derive(Clone, Debug)]
pub enum DefLoc {
    Param(BlockId),
    Instruction(BlockId, usize),
}
