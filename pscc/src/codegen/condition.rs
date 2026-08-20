use crate::ast::Expr;
use crate::codegen::ssa::BlockId;
use crate::codegen::{Instruction, Tranlsator, load_place_expr};

pub fn lower_condition_expr(
    translator: &mut Tranlsator,
    condition: &Expr,
    true_block: BlockId,
    false_block: BlockId,
) {
    match condition {
        Expr::LogicalAnd { lhs, rhs } => {
            let lhs_true_block = translator.create_block();
            lower_condition_expr(translator, lhs, lhs_true_block, false_block);

            translator.ssa.seal(lhs_true_block);
            translator.switch_to_block(lhs_true_block);

            lower_condition_expr(translator, rhs, true_block, false_block);
        }
        Expr::LogicalOr { lhs, rhs } => {
            let lhs_false_block = translator.create_block();
            lower_condition_expr(translator, lhs, true_block, lhs_false_block);

            translator.ssa.seal(lhs_false_block);
            translator.switch_to_block(lhs_false_block);

            lower_condition_expr(translator, rhs, true_block, false_block);
        }
        expr => {
            let cond = load_place_expr(translator, expr);
            translator.push_ins(Instruction::Brif(
                cond,
                true_block,
                Vec::new(),
                false_block,
                Vec::new(),
            ));
            translator.ssa.branch(&[true_block, false_block]);
        }
    }
}
