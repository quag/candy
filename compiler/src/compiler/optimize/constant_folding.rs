use crate::{
    builtin_functions::BuiltinFunction,
    compiler::mir::{Body, Expression, Id, Mir, VisibleExpressions},
};
use tracing::debug;

impl Mir {
    pub fn fold_constants(&mut self) {
        self.body.visit(&mut |id, expression, visible, _| {
            match expression {
                Expression::Call {
                    function,
                    arguments,
                    responsible,
                    ..
                } => {
                    if let Expression::Builtin(builtin) = visible.get(*function) &&
                        let Some(result) = Self::run_builtin(*builtin, arguments, visible)
                    {
                        let evaluated_call = match result {
                            Ok(return_value) => return_value,
                            Err(panic_reason) => {
                                let mut body = Body::new();
                                let reason = body.push_with_new_id(&mut self.id_generator, Expression::Text(panic_reason));
                                body.push_with_new_id(&mut self.id_generator, Expression::Panic { reason, responsible: *responsible });
                                Expression::Multiple(body)
                            },
                        };
                        debug!("Builtin {id} inlined to {evaluated_call:?}.");
                        *expression = evaluated_call;
                    }
                }
                Expression::Needs {
                    condition,
                    reason,
                    responsible,
                } => {
                    if let Expression::Symbol(symbol) = visible.get(*condition) {
                        let result = match symbol.as_str() {
                            "True" => Expression::Symbol("Nothing".to_string()),
                            "False" => Expression::Panic {
                                reason: *reason,
                                responsible: *responsible,
                            },
                            // TODO (before merging PR): Also save the call site
                            // in the needs expression and make that responsible
                            // for a panic with a non-symbol given.
                            _ => return,
                        };
                        *expression = result;
                    }
                }
                Expression::Error { child, errors } => {
                    // TODO (before merging PR): Remove and replace with a
                    // panic.
                }
                _ => {}
            }
        });
    }

    /// This function tries to run a builtin, requiring a minimal amount of
    /// static knowledge. For example, it can find out that the result of
    /// `builtinEquals $3 $3` is `True`, even if the value of `$3` is not known
    /// at compile-time.
    ///
    /// Returns `None` if the call couldn't be evaluated statically. Returns
    /// `Some(Ok(expression))` if the call successfully completed with a return
    /// value. Returns `Some(Err(reason))` if the call panics.
    fn run_builtin(
        builtin: BuiltinFunction,
        arguments: &[Id],
        visible: &VisibleExpressions,
    ) -> Option<Result<Expression, String>> {
        // warn!("Constant folding candidate: builtin{builtin:?}");
        // warn!(
        //     "Arguments: {}",
        //     arguments.iter().map(|arg| format!("{arg}")).join(", ")
        // );
        // warn!(
        //     "Expressions:\n{}",
        //     expressions
        //         .iter()
        //         .map(|(id, expr)| format!("{id}: {expr}"))
        //         .join("\n")
        // );

        Some(Ok(match builtin {
            BuiltinFunction::Equals => {
                if arguments.len() != 2 {
                    return Some(Err("wrong number of arguments".to_string()));
                }

                let a = arguments[0];
                let b = arguments[1];

                let are_equal = a == b || a.semantically_equals(b, visible)?;
                Expression::Symbol(if are_equal { "True" } else { "False" }.to_string())
            }
            // BuiltinFunction::FunctionRun => return,
            // BuiltinFunction::GetArgumentCount => todo!(),
            // BuiltinFunction::IfElse => todo!(),
            // BuiltinFunction::IntAdd => todo!(),
            // BuiltinFunction::IntBitLength => todo!(),
            // BuiltinFunction::IntBitwiseAnd => todo!(),
            // BuiltinFunction::IntBitwiseOr => todo!(),
            // BuiltinFunction::IntBitwiseXor => todo!(),
            // BuiltinFunction::IntCompareTo => todo!(),
            // BuiltinFunction::IntDivideTruncating => todo!(),
            // BuiltinFunction::IntModulo => todo!(),
            // BuiltinFunction::IntMultiply => todo!(),
            // BuiltinFunction::IntParse => todo!(),
            // BuiltinFunction::IntRemainder => todo!(),
            // BuiltinFunction::IntShiftLeft => todo!(),
            // BuiltinFunction::IntShiftRight => todo!(),
            // BuiltinFunction::IntSubtract => todo!(),
            // BuiltinFunction::Parallel => todo!(),
            // BuiltinFunction::Print => todo!(),
            BuiltinFunction::StructGet => {
                if arguments.len() != 2 {
                    return Some(Err("wrong number of arguments".to_string()));
                }

                let struct_id = arguments[0];
                let key_id = arguments[1];

                // TODO: Also catch this being called on a non-struct and
                // statically panic in that case.
                let Expression::Struct(fields) = visible.get(struct_id) else {
                    return None;
                };

                // TODO: Relax this requirement. Even if not all keys are
                // constant, we may still conclude the result of the builtin:
                // If only one key is statically determined to be equal to the
                // other and one is not, then we can still resolve that.
                if fields
                    .keys()
                    .all(|key| visible.get(*key).is_constant(visible))
                    && visible.get(key_id).is_constant(visible)
                {
                    let value = fields
                        .iter()
                        .find(|(k, _)| k.semantically_equals(key_id, visible).unwrap_or(false))
                        .map(|(_, value)| value.clone());
                    if let Some(value) = value {
                        Expression::Reference(value.clone())
                    } else {
                        return Some(Err(format!(
                            "Struct access will panic because key {key_id} isn't in there."
                        )));
                    }
                } else {
                    return None;
                }
            }
            // BuiltinFunction::StructGetKeys => todo!(),
            // BuiltinFunction::StructHasKey => todo!(),
            // BuiltinFunction::TextCharacters => todo!(),
            // BuiltinFunction::TextConcatenate => todo!(),
            // BuiltinFunction::TextContains => todo!(),
            // BuiltinFunction::TextEndsWith => todo!(),
            // BuiltinFunction::TextGetRange => todo!(),
            // BuiltinFunction::TextIsEmpty => todo!(),
            // BuiltinFunction::TextLength => todo!(),
            // BuiltinFunction::TextStartsWith => todo!(),
            // BuiltinFunction::TextTrimEnd => todo!(),
            // BuiltinFunction::TextTrimStart => todo!(),
            // BuiltinFunction::Try => todo!(),
            // BuiltinFunction::TypeOf => todo!(),
            _ => return None,
        }))
    }
}
