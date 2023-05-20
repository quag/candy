use candy_frontend::{
    ast_to_hir::AstToHir,
    hir::{self, Expression, HirDb, Id},
    module::{Module, ModuleDb},
    position::PositionConversionDb,
};
use candy_fuzzer::{Fuzzer, Status};
use candy_vm::{context::RunLimitedNumberOfInstructions, heap::Function, lir::Lir};
use itertools::Itertools;
use rand::{prelude::SliceRandom, thread_rng};
use std::sync::Arc;
use tracing::{debug, error};

use crate::{
    features_candy::hints::{utils::IdToEndOfLine, HintKind},
    utils::JoinWithCommasAndAnd,
};

use super::Hint;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct FuzzerManager {
    fuzzers: FxHashMap<Module, FxHashMap<Id, Fuzzer>>,
}

impl FuzzerManager {
    pub fn update_module(
        &mut self,
        module: Module,
        lir: Arc<Lir>,
        fuzzable_functions: &FxHashMap<Id, Function>,
    ) {
        let fuzzers = fuzzable_functions
            .iter()
            .map(|(id, function)| (id.clone(), Fuzzer::new(lir.clone(), *function, id.clone())))
            .collect();
        self.fuzzers.insert(module, fuzzers);
    }

    pub fn remove_module(&mut self, module: Module) {
        self.fuzzers.remove(&module).unwrap();
    }

    pub fn run(&mut self) -> Option<Module> {
        let mut running_fuzzers = self
            .fuzzers
            .values_mut()
            .flat_map(|fuzzers| fuzzers.values_mut())
            .filter(|fuzzer| matches!(fuzzer.status(), Status::StillFuzzing { .. }))
            .collect_vec();

        let fuzzer = running_fuzzers.choose_mut(&mut thread_rng())?;
        fuzzer.run(&mut RunLimitedNumberOfInstructions::new(1000));

        match &fuzzer.status() {
            Status::StillFuzzing { .. } => None,
            Status::FoundPanic { .. } => Some(fuzzer.function_id.module.clone()),
            Status::TotalCoverageButNoPanic => None,
        }
    }

    pub fn get_hints<DB>(&self, db: &DB, module: &Module) -> Vec<Vec<Hint>>
    where
        DB: AstToHir + HirDb + ModuleDb + PositionConversionDb,
    {
        let mut hints = vec![];

        debug!(
            "There {}.",
            if self.fuzzers.len() == 1 {
                "is 1 fuzzer".to_string()
            } else {
                format!("are {} fuzzers", self.fuzzers.len())
            }
        );

        for fuzzer in self.fuzzers[module].values() {
            let Status::FoundPanic {
                input,
                panic,
                ..
            } = fuzzer.status() else { continue; };

            let id = fuzzer.function_id.clone();
            let first_hint = {
                let parameter_names = match db.find_expression(id.clone()) {
                    Some(Expression::Function(hir::Function { parameters, .. })) => parameters
                        .into_iter()
                        .map(|parameter| parameter.keys.last().unwrap().to_string())
                        .collect_vec(),
                    Some(_) => panic!("Looks like we fuzzed a non-function. That's weird."),
                    None => {
                        error!("Using fuzzing, we found an error in a generated function.");
                        continue;
                    }
                };
                Hint {
                    kind: HintKind::Fuzz,
                    text: format!(
                        "If this is called with {},",
                        parameter_names
                            .iter()
                            .zip(input.arguments.iter())
                            .map(|(name, argument)| format!("`{name} = {argument:?}`"))
                            .collect_vec()
                            .join_with_commas_and_and(),
                    ),
                    position: db.id_to_end_of_line(id.clone()).unwrap(),
                }
            };

            let second_hint = {
                if &panic.responsible.module != module {
                    // The function panics internally for an input, but it's the
                    // fault of an inner function that's in another module.
                    // TODO: The fuzz case should instead be highlighted in the
                    // used function directly. We don't do that right now
                    // because we assume the fuzzer will find the panic when
                    // fuzzing the faulty function, but we should save the
                    // panicking case (or something like that) in the future.
                    continue;
                }
                if db.hir_to_cst_id(id.clone()).is_none() {
                    panic!(
                        "It looks like the generated code {} is at fault for a panic.",
                        panic.responsible,
                    );
                }

                // TODO: In the future, re-run only the failing case with
                // tracing enabled and also show the arguments to the failing
                // function in the hint.
                Hint {
                    kind: HintKind::Fuzz,
                    text: format!("then {} panics: {}", panic.responsible, panic.reason),
                    position: db.id_to_end_of_line(panic.responsible.clone()).unwrap(),
                }
            };

            hints.push(vec![first_hint, second_hint]);
        }

        hints
    }
}
