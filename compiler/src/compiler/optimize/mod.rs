// mod common_subtree_elimination;
mod complexity;
mod constant_folding;
mod constant_lifting;
mod follow_references;
mod inlining;
mod module_folding;
mod multiple_flattening;
mod tree_shaking;
mod utils;

use super::mir::Mir;
use crate::{database::Database, module::Module};
use tracing::debug;

impl Mir {
    pub fn optimize(&mut self, db: &Database) {
        debug!("MIR: {self:?}");
        debug!("Complexity: {}", self.complexity());
        self.optimize_obvious(db, &[]);
        debug!("Done optimizing.");
        debug!("MIR: {self:?}");
        debug!("Complexity: {}", self.complexity());
    }

    /// Performs optimizations without negative effects.
    pub fn optimize_obvious(&mut self, db: &Database, import_chain: &[Module]) {
        self.optimize_obvious_self_contained();
        self.fold_modules(db, import_chain);
        self.optimize_obvious_self_contained();
    }

    /// Performs optimizations without negative effects that work without
    /// looking at other modules.
    pub fn optimize_obvious_self_contained(&mut self) {
        loop {
            let before = self.clone();

            // debug!("Following references");
            self.checked_optimization(|mir| mir.follow_references());
            // debug!("Tree shake");
            self.checked_optimization(|mir| mir.tree_shake());
            // debug!("Fold constants");
            self.checked_optimization(|mir| mir.fold_constants());
            // debug!("Inline functions containing use");
            self.checked_optimization(|mir| mir.inline_functions_containing_use());
            // debug!("Lift constants");
            self.checked_optimization(|mir| mir.lift_constants());
            // debug!("Eliminate common subtrees");
            // self.checked_optimization(|mir| mir.eliminate_common_subtrees());
            // debug!("Flatten multiple");
            self.checked_optimization(|mir| mir.flatten_multiples());

            if *self == before {
                return;
            }
        }
    }

    fn checked_optimization(&mut self, optimization: fn(&mut Mir) -> ()) {
        optimization(self);
        self.validate();
    }
}
