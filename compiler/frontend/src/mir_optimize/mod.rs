//! Optimizations are a necessity for Candy code to run reasonably fast. For
//! example, without optimizations, if two modules import a third module using
//! `use "..foo"`, then the `foo` module is instantiated twice completely
//! separately. Because this module can in turn depend on other modules, this
//! approach would lead to exponential code blowup.
//!
//! When optimizing code in general, there are two main objectives:
//!
//! - Making the code fast.
//! - Making the code small.
//!
//! Some optimizations benefit both of these objectives. For example, removing
//! ignored computations from the program makes it smaller, but also means
//! there's less code to be executed. Other optimizations further one objective,
//! but harm the other. For example, inlining functions (basically copying their
//! code to where they're used), can make the code bigger, but also potentially
//! faster because there are less function calls to be performed.
//!
//! Depending on the use case, the tradeoff between both objectives changes. To
//! put you in the right mindset, here are just two use cases:
//!
//! - Programming for a microcontroller with 1 MB of ROM available for the
//!   program. In this case, you want your code to be as fast as possible while
//!   still fitting in 1 MB. Interestingly, the importance of code size is a
//!   step function: There's no benefit in only using 0.5 MB, but 1.1 MB makes
//!   the program completely unusable.
//!
//! - Programming for a WASM module to be downloaded. In this case, you might
//!   have some concrete measurements on how performance and download size
//!   affect user retention.
//!
//! It should be noted that we can't judge performance statically. Although some
//! optimizations such as inlining typically improve performance, there are rare
//! cases where they don't. For example, inlining a function that's used in
//! multiple places means the CPU's branch predictor can't benefit from the
//! knowledge gained by previous function executions. Inlining might also make
//! your program bigger, causing more cache misses. Thankfully, Candy is not yet
//! optimized enough for us to care about such details.
//!
//! This module contains several optimizations. All of them operate on the MIR.
//! Some are called "obvious". Those are optimizations that typically improve
//! both performance and code size. Whenever they can be applied, they should be
//! applied.

mod cleanup;
mod common_subtree_elimination;
mod complexity;
mod constant_folding;
mod constant_lifting;
mod inlining;
mod module_folding;
mod multiple_flattening;
mod reference_following;
mod tree_shaking;
mod utils;

use super::{hir, hir_to_mir::HirToMir, mir::Mir, tracing::TracingConfig};
use crate::{
    error::CompilerError, hir_to_mir::MirResult, mir::MirError, module::Module, rich_ir::ToRichIr,
};
use rustc_hash::{FxHashSet, FxHasher};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};
use tracing::debug;

use itertools::Itertools;

#[salsa::query_group(OptimizeMirStorage)]
pub trait OptimizeMir: HirToMir {
    #[salsa::cycle(recover_from_cycle)]
    fn optimized_mir(&self, module: Module, tracing: TracingConfig) -> MirResult;
}

fn optimized_mir(db: &dyn OptimizeMir, module: Module, tracing: TracingConfig) -> MirResult {
    debug!("{}: Compiling.", module.to_rich_ir());
    let (mir, errors) = db.mir(module.clone(), tracing.clone())?;
    let mut mir = (*mir).clone();
    let mut errors = (*errors).clone();

    let complexity_before = mir.complexity();
    mir.optimize_obvious(db, &tracing, &mut errors);
    let complexity_after = mir.complexity();

    debug!(
        "{}: Done. Optimized from {complexity_before} to {complexity_after}",
        module.to_rich_ir(),
    );
    Ok((Arc::new(mir), Arc::new(errors)))
}

impl Mir {
    /// Performs optimizations that (usually) improve both performance and code
    /// size.
    pub fn optimize_obvious(
        &mut self,
        db: &dyn OptimizeMir,
        tracing: &TracingConfig,
        errors: &mut FxHashSet<CompilerError>,
    ) {
        self.optimize_stuff_necessary_for_module_folding();
        self.checked_optimization(&mut |mir| mir.fold_modules(db, tracing, errors));
        self.replace_remaining_uses_with_panics(errors);
        self.heavily_optimize();
        self.cleanup();
    }

    pub fn optimize_stuff_necessary_for_module_folding(&mut self) {
        loop {
            let hashcode_before = self.do_hash();

            // TODO: If you have the (unusual) code structure of a very long
            // function containing a `use` that's used very often, this
            // optimization leads to a big blowup of code. We should possibly
            // think about what to do in that case.
            self.checked_optimization(&mut |mir| mir.inline_functions_containing_use());
            self.checked_optimization(&mut |mir| mir.flatten_multiples());
            self.checked_optimization(&mut |mir| mir.follow_references());

            if self.do_hash() == hashcode_before {
                return;
            }
        }
    }

    /// Performs optimizations that (usually) improve both performance and code
    /// size and that work without looking at other modules.
    pub fn heavily_optimize(&mut self) {
        loop {
            let hashcode_before = self.do_hash();

            self.checked_optimization(&mut |mir| mir.follow_references());
            self.checked_optimization(&mut |mir| mir.remove_redundant_return_references());
            self.checked_optimization(&mut |mir| mir.tree_shake());
            self.checked_optimization(&mut |mir| mir.fold_constants());
            self.checked_optimization(&mut |mir| mir.inline_functions_only_called_once());
            self.checked_optimization(&mut |mir| mir.inline_tiny_functions());
            self.checked_optimization(&mut |mir| mir.lift_constants());
            self.checked_optimization(&mut |mir| mir.eliminate_common_subtrees());
            self.checked_optimization(&mut |mir| mir.flatten_multiples());

            if self.do_hash() == hashcode_before {
                return;
            }
        }
    }
    fn do_hash(&self) -> u64 {
        let mut hasher = FxHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }

    fn checked_optimization(&mut self, optimization: &mut impl FnMut(&mut Mir)) {
        self.cleanup();
        optimization(self);
        if cfg!(debug_assertions) {
            self.validate();
        }
    }
}

fn recover_from_cycle(
    _db: &dyn OptimizeMir,
    cycle: &[String],
    module: &Module,
    _tracing: &TracingConfig,
) -> MirResult {
    let error = CompilerError::for_whole_module(
        module.clone(),
        MirError::ModuleHasCycle {
            cycle: cycle.iter().cloned().collect_vec(),
        },
    );

    let mir = Mir::build(|body| {
        let reason = body.push_text(error.payload.to_string());
        let responsible = body.push_hir_id(hir::Id::new(module.clone(), vec![]));
        body.push_panic(reason, responsible);
    });

    Ok((Arc::new(mir), Arc::new(vec![error].into_iter().collect())))
}
