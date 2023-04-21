use crate::lir::{Instruction, Lir, StackOffset};
use candy_frontend::{
    cst::CstDb,
    error::{CompilerError, CompilerErrorPayload},
    hir,
    id::CountableId,
    mir::{Body, Expression, Id, Mir},
    mir_optimize::OptimizeMir,
    module::Module,
    rich_ir::ToRichIr,
    tracing::TracingConfig,
};
use itertools::Itertools;
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[salsa::query_group(MirToLirStorage)]
pub trait MirToLir: CstDb + OptimizeMir {
    fn lir(
        &self,
        module: Module,
        tracing: TracingConfig,
    ) -> (Arc<Lir>, Arc<FxHashSet<CompilerError>>);
}

fn lir(
    db: &dyn MirToLir,
    module: Module,
    tracing: TracingConfig,
) -> (Arc<Lir>, Arc<FxHashSet<CompilerError>>) {
    let (mir, errors) = db
        .optimized_mir(module.clone(), tracing)
        .unwrap_or_else(|error| {
            let payload = CompilerErrorPayload::Module(error);
            let mir = Mir::build(|body| {
                let reason = body.push_text(payload.to_string());
                let responsible = body.push_hir_id(hir::Id::user());
                body.push_panic(reason, responsible);
            });
            let errors = vec![CompilerError::for_whole_module(module, payload)]
                .into_iter()
                .collect();
            (Arc::new(mir), Arc::new(errors))
        });
    let instructions = compile_lambda(&FxHashSet::default(), &[], Id::from_usize(0), &mir.body);
    (Arc::new(Lir { instructions }), errors)
}

fn compile_lambda(
    captured: &FxHashSet<Id>,
    parameters: &[Id],
    responsible_parameter: Id,
    body: &Body,
) -> Vec<Instruction> {
    let mut context = LoweringContext::default();
    for captured in captured {
        context.stack.push(*captured);
    }
    for parameter in parameters {
        context.stack.push(*parameter);
    }
    context.stack.push(responsible_parameter);

    for (id, expression) in body.iter() {
        context.compile_expression(id, expression);
    }

    if matches!(
        context.instructions.last().unwrap(),
        Instruction::Call { .. },
    ) {
        let Instruction::Call { num_args } = context.instructions.pop().unwrap() else { unreachable!() };
        context.instructions.push(Instruction::TailCall {
            num_locals_to_pop: context.stack.len() - 1,
            num_args,
        });
    } else {
        let dummy_id = Id::from_usize(0);
        context.emit(
            dummy_id,
            Instruction::PopMultipleBelowTop(context.stack.len() - 1),
        );
        context.emit(dummy_id, Instruction::Return);
    }

    context.instructions
}

#[derive(Default)]
struct LoweringContext {
    stack: Vec<Id>,
    instructions: Vec<Instruction>,
}
impl LoweringContext {
    fn compile_expression(&mut self, id: Id, expression: &Expression) {
        match expression {
            Expression::Int(int) => self.emit(id, Instruction::CreateInt(int.clone())),
            Expression::Text(text) => self.emit(id, Instruction::CreateText(text.clone())),
            Expression::Reference(reference) => {
                self.emit_push_from_stack(*reference);
                self.stack.replace_top_id(id);
            }
            Expression::Symbol(symbol) => self.emit(id, Instruction::CreateSymbol(symbol.clone())),
            Expression::Builtin(builtin) => {
                self.emit(id, Instruction::CreateBuiltin(*builtin));
            }
            Expression::List(items) => {
                for item in items {
                    self.emit_push_from_stack(*item);
                }
                self.emit(
                    id,
                    Instruction::CreateList {
                        num_items: items.len(),
                    },
                );
            }
            Expression::Struct(fields) => {
                for (key, value) in fields {
                    self.emit_push_from_stack(*key);
                    self.emit_push_from_stack(*value);
                }
                self.emit(
                    id,
                    Instruction::CreateStruct {
                        num_fields: fields.len(),
                    },
                );
            }
            Expression::HirId(hir_id) => {
                self.emit(id, Instruction::CreateHirId(hir_id.clone()));
            }
            Expression::Lambda {
                original_hirs: _,
                parameters,
                responsible_parameter,
                body,
            } => {
                let captured = expression.captured_ids();
                let instructions =
                    compile_lambda(&captured, parameters, *responsible_parameter, body);

                self.emit(
                    id,
                    Instruction::CreateClosure {
                        captured: captured
                            .iter()
                            .map(|id| self.stack.find_id(*id))
                            .collect_vec(),
                        num_args: parameters.len(),
                        body: instructions,
                    },
                );
            }
            Expression::Parameter => {
                panic!("The MIR should not contain any parameter expressions.")
            }
            Expression::Call {
                function,
                arguments,
                responsible,
            } => {
                self.emit_push_from_stack(*function);
                for argument in arguments {
                    self.emit_push_from_stack(*argument);
                }
                self.emit_push_from_stack(*responsible);
                self.emit(
                    id,
                    Instruction::Call {
                        num_args: arguments.len(),
                    },
                );
            }
            Expression::UseModule { .. } => {
                panic!("MIR still contains use. This should have been optimized out.");
            }
            Expression::Panic {
                reason,
                responsible,
            } => {
                self.emit_push_from_stack(*reason);
                self.emit_push_from_stack(*responsible);
                self.emit(id, Instruction::Panic);
            }
            Expression::Multiple(_) => {
                panic!("The MIR shouldn't contain multiple expressions anymore.");
            }
            Expression::ModuleStarts { module } => {
                self.emit(
                    id,
                    Instruction::ModuleStarts {
                        module: module.clone(),
                    },
                );
            }
            Expression::ModuleEnds => self.emit(id, Instruction::ModuleEnds),
            Expression::TraceCallStarts {
                hir_call,
                function,
                arguments,
                responsible,
            } => {
                self.emit_push_from_stack(*hir_call);
                self.emit_push_from_stack(*function);
                for argument in arguments {
                    self.emit_push_from_stack(*argument);
                }
                self.emit_push_from_stack(*responsible);
                self.emit(
                    id,
                    Instruction::TraceCallStarts {
                        num_args: arguments.len(),
                    },
                );
            }
            Expression::TraceCallEnds { return_value } => {
                self.emit_push_from_stack(*return_value);
                self.emit(id, Instruction::TraceCallEnds);
            }
            Expression::TraceExpressionEvaluated {
                hir_expression,
                value,
            } => {
                self.emit_push_from_stack(*hir_expression);
                self.emit_push_from_stack(*value);
                self.emit(id, Instruction::TraceExpressionEvaluated);
            }
            Expression::TraceFoundFuzzableClosure {
                hir_definition,
                closure,
            } => {
                self.emit_push_from_stack(*hir_definition);
                self.emit_push_from_stack(*closure);
                self.emit(id, Instruction::TraceFoundFuzzableClosure);
            }
        }
    }

    fn emit_push_from_stack(&mut self, id: Id) {
        let offset = self.stack.find_id(id);
        self.emit(id, Instruction::PushFromStack(offset));
    }
    fn emit(&mut self, id: Id, instruction: Instruction) {
        instruction.apply_to_stack(&mut self.stack, id);
        self.instructions.push(instruction);
    }
}

trait StackExt {
    fn pop_multiple(&mut self, n: usize);
    fn find_id(&self, id: Id) -> StackOffset;
    fn replace_top_id(&mut self, id: Id);
}
impl StackExt for Vec<Id> {
    fn pop_multiple(&mut self, n: usize) {
        for _ in 0..n {
            self.pop();
        }
    }
    fn find_id(&self, id: Id) -> StackOffset {
        self.iter()
            .rev()
            .position(|it| *it == id)
            .unwrap_or_else(|| {
                panic!(
                    "Id {} not found in stack: {}",
                    id.to_rich_ir(),
                    self.iter().map(|it| it.to_rich_ir()).join(" "),
                )
            })
    }
    fn replace_top_id(&mut self, id: Id) {
        self.pop().unwrap();
        self.push(id);
    }
}
