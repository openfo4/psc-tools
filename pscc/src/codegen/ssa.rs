//! SSA Builder
//!
//! The current algorithm is based on
//! *Braun, M., Buchwald, S., Hack, S., Leißa, R., Mallon, C., Zwinkau, A. (2013).
//! Simple and Efficient Construction of Static Single Assignment Form.
//! In: Jhala, R., De Bosschere, K. (eds) Compiler Construction. CC 2013.
//! Lecture Notes in Computer Science, vol 7791. Springer, Berlin, Heidelberg.
//! <https://doi.org/10.1007/978-3-642-37051-9_6>*

use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug)]
pub struct SsaBuilder {
    blocks: Vec<Block>,
    next_value: Value,
    next_var: Var,
    cur_block: BlockId,
    var_values: HashMap<Var, Vec<Value>>,
}

impl SsaBuilder {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            next_value: Value(0),
            next_var: Var(0),
            cur_block: BlockId(usize::MAX),
            var_values: HashMap::new(),
        }
    }

    pub fn switch_block(&mut self, block: BlockId) {
        self.cur_block = block;
    }

    pub fn create_value(&mut self) -> Value {
        let value = self.next_value;
        self.next_value.0 += 1;
        value
    }

    /// Record a use of the current variable. Returns an appropriate value.
    pub fn use_var(&mut self, var: Var) -> Value {
        let mut block = &mut self.blocks[self.cur_block.0];

        loop {
            if let Some(value) = block.values.get(&var) {
                return *value;
            }

            // If the block is not sealed yet, we don't know all the potential
            // predecessors yet. Since another predecessor may be added to this
            // block in the future we create a new value and mark the variable
            // as undefined.
            // When the block becomes sealed later this will be resolved.
            if !block.sealed {
                let value = self.next_value;
                self.next_value.0 += 1;
                block.undef_values.push((var, value));
                block.values.insert(var, value);
                self.var_values.entry(var).or_default().push(value);
                break value;
            }

            // We need no Phi node if we have exactly one predecessor.
            if block.pred.len() == 1 {
                let pred = block.pred[0];
                block = &mut self.blocks[pred.0];
                continue;
            }

            let value = self.next_value;
            self.next_value.0 += 1;

            block.params.push(value);
            block.values.insert(var, value);
            self.var_values.entry(var).or_default().push(value);

            let id = block.id;
            for pred in block.pred.clone() {
                self.insert_phi(id, pred, var);
            }

            return value;
        }
    }

    /// Record a new definition of a variable at the current location.
    pub fn def_var(&mut self, var: Var, value: Value) {
        let block = &mut self.blocks[self.cur_block.0];
        block.values.insert(var, value);
    }

    fn insert_phi(&mut self, src: BlockId, dst: BlockId, var: Var) {
        let mut queue: VecDeque<_> = vec![(src, dst, var)].into();

        while let Some((src, dst, var)) = queue.pop_front() {
            let mut cur_block = &mut self.blocks[dst.0];
            let value = loop {
                if let Some(value) = cur_block.values.get(&var) {
                    break *value;
                }

                // If the block is not sealed yet, we don't know all the potential
                // predecessors yet. Since another predecessor may be added to this
                // block in the future we create a new value and mark the variable
                // as undefined.
                // When the block becomes sealed later this will be resolved.
                if !cur_block.sealed {
                    let value = self.next_value;
                    self.next_value.0 += 1;
                    cur_block.undef_values.push((var, value));
                    cur_block.values.insert(var, value);
                    self.var_values.entry(var).or_default().push(value);

                    break value;
                }

                // We need no Phi node if we have exactly one predecessor.
                if cur_block.pred.len() == 1 {
                    let pred = cur_block.pred[0];
                    cur_block = &mut self.blocks[pred.0];
                    continue;
                }

                let value = self.next_value;
                self.next_value.0 += 1;

                cur_block.params.push(value);
                cur_block.values.insert(var, value);
                self.var_values.entry(var).or_default().push(value);

                for block in &cur_block.pred {
                    queue.push_back((cur_block.id, *block, var));
                }

                break value;
            };

            let block = &mut self.blocks[dst.0];

            let branch = block
                .terminal
                .iter_mut()
                .find(|succ| succ.block == src)
                .unwrap();
            branch.params.push(value);
        }
    }

    /// Mark a block as sealed, meaning that all predecessors are known.
    pub fn seal(&mut self, block_id: BlockId) {
        let block = &mut self.blocks[block_id.0];

        let undef = core::mem::take(&mut block.undef_values);

        let id = block.id;
        let preds = block.pred.clone();
        for (var, _) in &undef {
            for pred in &preds {
                self.insert_phi(id, *pred, *var);
            }
        }

        let block = &mut self.blocks[block_id.0];

        for (_, value) in undef {
            block.params.push(value);
        }

        block.sealed = true;
    }

    pub fn create_var(&mut self) -> Var {
        let var = self.next_var;
        self.next_var.0 += 1;
        var
    }

    pub fn create_block(&mut self) -> BlockId {
        let block = BlockId(self.blocks.len());
        self.blocks.push(Block {
            id: block,
            values: HashMap::new(),
            params: Vec::new(),
            succ: Vec::new(),
            pred: Vec::new(),
            undef_values: Vec::new(),
            sealed: false,
            terminal: Vec::new(),
            filled: false,
        });
        block
    }

    /// Transfer control flow to any of the following blocks.
    ///
    /// This seals the current block.
    pub fn branch(&mut self, targets: &[BlockId]) {
        let block = &mut self.blocks[self.cur_block.0];

        debug_assert!(!block.filled);
        block.filled = true;

        let branches = targets
            .iter()
            .map(|block| Branch {
                block: *block,
                params: Vec::new(),
            })
            .collect();
        block.terminal = branches;

        block.succ.extend(targets);
        for target in targets {
            let target = &mut self.blocks[target.0];
            debug_assert!(!target.sealed, "cannot branch to a sealed block");

            target.pred.push(self.cur_block);
        }
    }

    pub fn block_params(&self, block: BlockId) -> &[Value] {
        let block = &self.blocks[block.0];
        &block.params
    }

    pub fn branch_args(&self, from: BlockId, to: BlockId) -> &[Value] {
        let block = &self.blocks[from.0];
        &block
            .terminal
            .iter()
            .find(|b| b.block == to)
            .unwrap()
            .params
    }

    pub fn var_values(&self, var: Var) -> &[Value] {
        self.var_values
            .get(&var)
            .map(|v| v.as_slice())
            .unwrap_or_default()
    }
}

impl Default for SsaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Var(usize);

/// A SSA value.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Value(usize);

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.0)
    }
}

#[derive(Clone, Debug)]
struct Block {
    id: BlockId,
    values: HashMap<Var, Value>,
    params: Vec<Value>,
    succ: Vec<BlockId>,
    pred: Vec<BlockId>,
    undef_values: Vec<(Var, Value)>,
    /// A block is sealed when we know all its predecessors.
    sealed: bool,
    filled: bool,
    terminal: Vec<Branch>,
}

#[derive(Clone, Debug)]
struct Branch {
    /// The block to transfer control to.
    block: BlockId,
    /// The local parameters to pass to the block.
    params: Vec<Value>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(usize);

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "b{}", self.0)
    }
}

#[cfg(test)]
mod tests {

    use crate::codegen::ssa::SsaBuilder;

    #[test]
    fn ssa_local() {
        let mut builder = SsaBuilder::new();

        let block = builder.create_block();
        builder.switch_block(block);

        let var = builder.create_var();
        let v0 = builder.create_value();
        builder.def_var(var, v0);

        let value = builder.use_var(var);
        assert_eq!(v0, value);

        let v1 = builder.create_value();
        builder.def_var(var, v1);

        let value = builder.use_var(var);
        assert_eq!(v1, value);
    }

    #[test]
    fn ssa_branch_sealed() {
        // b0:
        //   def x
        //   def y
        //   def z
        //   branch b1 b2
        // b1:
        //   use x
        // b2:
        //   def y
        //   use y
        let mut builder = SsaBuilder::new();

        let b0 = builder.create_block();
        let b1 = builder.create_block();
        let b2 = builder.create_block();

        let x = builder.create_var();
        let y = builder.create_var();
        let z = builder.create_var();

        builder.seal(b0);
        builder.switch_block(b0);

        let x0 = builder.create_value();
        let y0 = builder.create_value();
        let z0 = builder.create_value();
        builder.def_var(x, x0);
        builder.def_var(y, y0);
        builder.def_var(z, z0);
        assert_eq!(builder.use_var(x), x0);
        assert_eq!(builder.use_var(y), y0);
        assert_eq!(builder.use_var(z), z0);

        builder.branch(&[b1, b2]);
        builder.seal(b1);
        builder.seal(b2);

        builder.switch_block(b1);

        let x1 = builder.use_var(x);
        assert_eq!(x1, x0);

        builder.branch(&[]);

        builder.switch_block(b2);
        let y1 = builder.create_value();
        builder.def_var(y, y1);
        assert_eq!(builder.use_var(y), y1);
    }

    #[test]
    fn ssa_branch_unsealed() {
        // b0:
        //   def x
        //   def y
        //   def z
        //   branch b1 b2
        // b1:
        //   use x
        // b2:
        //   def y
        //   use y
        let mut builder = SsaBuilder::new();

        let b0 = builder.create_block();
        let b1 = builder.create_block();
        let b2 = builder.create_block();

        let x = builder.create_var();
        let y = builder.create_var();
        let z = builder.create_var();

        builder.switch_block(b0);

        let x0 = builder.create_value();
        let y0 = builder.create_value();
        let z0 = builder.create_value();
        builder.def_var(x, x0);
        builder.def_var(y, y0);
        builder.def_var(z, z0);
        assert_eq!(builder.use_var(x), x0);
        assert_eq!(builder.use_var(y), y0);
        assert_eq!(builder.use_var(z), z0);

        builder.branch(&[b1, b2]);

        builder.switch_block(b1);
        let x1 = builder.use_var(x);
        assert_eq!(builder.blocks[b1.0].undef_values[0], (x, x1));
        builder.branch(&[]);

        builder.switch_block(b2);
        let y1 = builder.create_value();
        builder.def_var(y, y1);
        assert_eq!(builder.use_var(y), y1);
        builder.branch(&[]);

        builder.seal(b0);
        builder.seal(b1);
        builder.seal(b2);

        assert_eq!(builder.blocks[b1.0].params[0], x1);
        assert_eq!(builder.blocks[b0.0].terminal[0].params[0], x0);
    }

    #[test]
    fn ssa_branch_and_merge_sealed() {
        // b0:
        //   def x
        //   def y
        //   branch b1 b2
        // b1:
        //   use x
        //   branch b3
        // b2:
        //   def x
        //   branch b3
        // b3:
        //   use x
        //   use y
        let mut builder = SsaBuilder::new();

        let b0 = builder.create_block();
        let b1 = builder.create_block();
        let b2 = builder.create_block();
        let b3 = builder.create_block();

        let x = builder.create_var();
        let y = builder.create_var();

        builder.seal(b0);
        builder.switch_block(b0);

        let x0 = builder.create_value();
        let y0 = builder.create_value();
        builder.def_var(x, x0);
        builder.def_var(y, y0);
        assert_eq!(builder.use_var(x), x0);
        assert_eq!(builder.use_var(y), y0);

        builder.branch(&[b1, b2]);
        builder.seal(b1);
        builder.seal(b2);

        builder.switch_block(b1);

        let x1 = builder.use_var(x);
        assert_eq!(x1, x0);

        builder.branch(&[b3]);

        builder.switch_block(b2);

        let x2 = builder.create_value();
        builder.def_var(x, x2);

        builder.branch(&[b3]);

        builder.seal(b3);

        builder.switch_block(b3);

        let x3 = builder.use_var(x);
        assert_eq!(builder.blocks[b3.0].params[0], x3);
        assert_eq!(builder.blocks[b1.0].terminal[0].params[0], x0);
        assert_eq!(builder.blocks[b2.0].terminal[0].params[0], x2);

        let y2 = builder.use_var(y);
        assert_eq!(builder.blocks[b3.0].params[1], y2);
        assert_eq!(builder.blocks[b1.0].terminal[0].params[1], y0);
        assert_eq!(builder.blocks[b2.0].terminal[0].params[1], y0);
    }

    #[test]
    fn ssa_branch_and_merge_unsealed() {
        // b0:
        //   def x
        //   def y
        //   branch b1 b2
        // b1:
        //   use x
        //   branch b3
        // b2:
        //   def x
        //   branch b3
        // b3:
        //   use x
        //   use y
        let mut builder = SsaBuilder::new();

        let b0 = builder.create_block();
        let b1 = builder.create_block();
        let b2 = builder.create_block();
        let b3 = builder.create_block();

        let x = builder.create_var();
        let y = builder.create_var();

        builder.seal(b0);
        builder.switch_block(b0);

        let x0 = builder.create_value();
        let y0 = builder.create_value();
        builder.def_var(x, x0);
        builder.def_var(y, y0);
        assert_eq!(builder.use_var(x), x0);
        assert_eq!(builder.use_var(y), y0);
        builder.branch(&[b1, b2]);

        builder.switch_block(b1);
        let x1 = builder.use_var(x);
        builder.branch(&[b3]);

        builder.switch_block(b2);
        let x2 = builder.create_value();
        builder.def_var(x, x2);
        builder.branch(&[b3]);

        builder.switch_block(b3);
        let x3 = builder.use_var(x);

        builder.seal(b0);
        builder.seal(b1);
        builder.seal(b2);
        builder.seal(b3);

        assert_eq!(builder.blocks[b1.0].params, [x1]);
        assert_eq!(builder.blocks[b0.0].terminal[0].params, [x0]);

        assert_eq!(builder.blocks[b2.0].params, []);

        assert_eq!(builder.blocks[b3.0].params, [x3]);
        assert_eq!(builder.blocks[b1.0].terminal[0].params, [x1]);
        assert_eq!(builder.blocks[b2.0].terminal[0].params, [x2]);
    }

    #[test]
    fn ssa_loop_sealed() {
        // b0:
        //   def x
        //   def y
        // b1:
        //   use x
        //   use y
        //   branch b2 b3
        // b2:
        //   use x
        //   use y
        //   def x
        //   branch b1
        // b3:
        //   use x
        //   use y
        let mut builder = SsaBuilder::new();

        let b0 = builder.create_block();
        let b1 = builder.create_block();
        let b2 = builder.create_block();
        let b3 = builder.create_block();

        let x = builder.create_var();
        let y = builder.create_var();

        builder.seal(b0);
        builder.switch_block(b0);
        let x0 = builder.create_value();
        let y0 = builder.create_value();
        builder.def_var(x, x0);
        builder.def_var(y, y0);
        assert_eq!(builder.use_var(x), x0);
        assert_eq!(builder.use_var(y), y0);
        builder.branch(&[b1]);

        builder.switch_block(b1);
        let x1 = builder.use_var(x);
        let y1 = builder.use_var(y);
        assert_eq!(builder.blocks[b1.0].undef_values[0], (x, x1));
        assert_eq!(builder.blocks[b1.0].undef_values[1], (y, y1));
        builder.branch(&[b2, b3]);

        builder.seal(b2);
        builder.switch_block(b2);
        let x2 = builder.use_var(x);
        let y2 = builder.use_var(y);
        let x3 = builder.create_value();
        builder.def_var(x, x3);
        assert_eq!(x2, x1);
        assert_eq!(y2, y1);
        builder.branch(&[b1]);

        builder.seal(b1);
        builder.seal(b3);
        builder.switch_block(b3);
        let x4 = builder.use_var(x);
        let y3 = builder.use_var(y);
        assert_eq!(x4, x1);
        assert_eq!(y3, y1);

        assert_eq!(builder.blocks[b1.0].params[0], x1);
        assert_eq!(builder.blocks[b1.0].params[1], y1);
        assert_eq!(builder.blocks[b0.0].terminal[0].params[0], x0);
        assert_eq!(builder.blocks[b0.0].terminal[0].params[1], y0);
    }
}
