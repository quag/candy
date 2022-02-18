use std::ops::Range;
use std::sync::Arc;

use super::ast::{self, Assignment, Ast, AstKind, Identifier, Int, Struct, Symbol, Text};
use super::cst::{self, Cst, CstDb};
use super::cst_to_ast::CstToAst;
use super::error::CompilerError;
use super::hir::{self, Body, Expression, Lambda};
use crate::builtin_functions;
use crate::input::Input;
use im::HashMap;

#[salsa::query_group(AstToHirStorage)]
pub trait AstToHir: CstDb + CstToAst {
    fn hir_to_ast_id(&self, input: Input, id: hir::Id) -> Option<ast::Id>;
    fn hir_to_cst_id(&self, input: Input, id: hir::Id) -> Option<cst::Id>;
    fn hir_id_to_span(&self, input: Input, id: hir::Id) -> Option<Range<usize>>;
    fn hir_id_to_display_span(&self, input: Input, id: hir::Id) -> Option<Range<usize>>;

    fn hir(&self, input: Input) -> Option<(Arc<Body>, HashMap<hir::Id, ast::Id>)>;
    fn hir_raw(
        &self,
        input: Input,
    ) -> Option<(Arc<Body>, HashMap<hir::Id, ast::Id>, Vec<CompilerError>)>;
}

fn hir_to_ast_id(db: &dyn AstToHir, input: Input, id: hir::Id) -> Option<ast::Id> {
    let (_, hir_to_ast_id_mapping) = db.hir(input).unwrap();
    hir_to_ast_id_mapping.get(&id).cloned()
}
fn hir_to_cst_id(db: &dyn AstToHir, input: Input, id: hir::Id) -> Option<cst::Id> {
    let id = db.hir_to_ast_id(input.clone(), id)?;
    db.ast_to_cst_id(input, id)
}
fn hir_id_to_span(db: &dyn AstToHir, input: Input, id: hir::Id) -> Option<Range<usize>> {
    let id = db.hir_to_ast_id(input.clone(), id)?;
    db.ast_id_to_span(input, id)
}
fn hir_id_to_display_span(db: &dyn AstToHir, input: Input, id: hir::Id) -> Option<Range<usize>> {
    let id = db.hir_to_cst_id(input.clone(), id)?;
    Some(db.find_cst(input, id)?.display_span())
}

fn hir(db: &dyn AstToHir, input: Input) -> Option<(Arc<Body>, HashMap<hir::Id, ast::Id>)> {
    db.hir_raw(input)
        .map(|(hir, id_mapping, _)| (hir, id_mapping))
}
fn hir_raw(
    db: &dyn AstToHir,
    input: Input,
) -> Option<(Arc<Body>, HashMap<hir::Id, ast::Id>, Vec<CompilerError>)> {
    let (ast, _) = db.ast(input.clone())?;

    let cst = db.cst(input.clone()).unwrap();
    let (_, ast_to_cst_id_mapping) = db.ast(input.clone()).unwrap();

    let mut context = Context {
        db,
        input: input.clone(),
        cst,
        ast_to_cst_id_mapping,
        id_mapping: HashMap::new(),
        errors: vec![],
    };

    let mut compiler = Compiler::new(&mut context);
    compiler.compile(&ast);
    Some((
        Arc::new(compiler.body),
        compiler.context.id_mapping,
        compiler.context.errors,
    ))
}

struct Context<'a> {
    db: &'a dyn AstToHir,
    input: Input,
    cst: Arc<Vec<Cst>>,
    ast_to_cst_id_mapping: HashMap<ast::Id, cst::Id>,
    id_mapping: HashMap<hir::Id, ast::Id>,
    errors: Vec<CompilerError>,
}
struct Compiler<'a> {
    context: &'a mut Context<'a>,
    body: Body,
    parent_ids: Vec<usize>,
    next_id: usize,
    identifiers: HashMap<String, hir::Id>,
}
impl<'a> Compiler<'a> {
    fn new(context: &'a mut Context<'a>) -> Self {
        let builtin_identifiers = builtin_functions::VALUES
            .iter()
            .enumerate()
            .map(|(index, builtin_function)| {
                let string = format!("builtin{:?}", builtin_function);
                (string, hir::Id(vec![index]))
            })
            .collect::<HashMap<_, _>>();

        Compiler {
            context,
            parent_ids: vec![],
            next_id: builtin_identifiers.len(),
            body: Body::new(),
            identifiers: builtin_identifiers,
        }
    }

    fn compile(&mut self, asts: &[Ast]) {
        if asts.is_empty() {
            self.body.out = Some(self.push_without_ast_mapping(Expression::nothing()));
        } else {
            for ast in asts.into_iter() {
                self.body.out = Some(self.compile_single(ast));
            }
        }
    }
    fn compile_single(&mut self, ast: &Ast) -> hir::Id {
        match &ast.kind {
            AstKind::Int(Int(int)) => self.push(ast.id, Expression::Int(int.to_owned()), None),
            AstKind::Text(Text(string)) => {
                self.push(ast.id, Expression::Text(string.value.to_owned()), None)
            }
            AstKind::Identifier(Identifier(symbol)) => {
                let reference = match self.identifiers.get(&symbol.value) {
                    Some(reference) => reference.to_owned(),
                    None => {
                        self.context.errors.push(CompilerError {
                            message: format!("Unknown reference: {}", symbol.value),
                            span: self
                                .context
                                .db
                                .ast_id_to_span(self.context.input.clone(), symbol.id)
                                .unwrap(),
                        });
                        return self.push(symbol.id, Expression::Error, None);
                    }
                };
                self.push(ast.id, Expression::Reference(reference.to_owned()), None)
            }
            AstKind::Symbol(Symbol(symbol)) => {
                self.push(ast.id, Expression::Symbol(symbol.value.to_owned()), None)
            }
            AstKind::Struct(Struct { entries }) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| (self.compile_single(key), self.compile_single(value)))
                    .collect();
                self.push(ast.id, Expression::Struct(entries), None)
            }
            AstKind::Lambda(ast::Lambda {
                parameters,
                body: body_asts,
            }) => {
                let mut body = Body::new();
                let lambda_id = add_ids(&self.parent_ids, self.next_id);
                let mut identifiers = self.identifiers.clone();

                for (parameter_index, parameter) in parameters.iter().enumerate() {
                    let id = hir::Id(add_ids(&lambda_id, parameter_index));
                    body.identifiers
                        .insert(id.to_owned(), parameter.value.to_owned());
                    identifiers.insert(parameter.value.to_owned(), id);
                }
                let mut inner = Compiler::<'a> {
                    context: self.context,
                    body,
                    parent_ids: lambda_id.to_owned(),
                    next_id: parameters.len(),
                    identifiers,
                };

                inner.compile(&body_asts);
                self.context = inner.context;
                self.push(
                    ast.id,
                    Expression::Lambda(Lambda {
                        first_id: hir::Id(add_ids(&lambda_id[..], 0)),
                        parameters: parameters.iter().map(|it| it.value.to_owned()).collect(),
                        body: inner.body,
                    }),
                    None,
                )
            }
            AstKind::Call(ast::Call { name, arguments }) => {
                let argument_ids = arguments
                    .iter()
                    .map(|argument| self.compile_single(argument))
                    .collect();
                let function = match self.identifiers.get(&name.value) {
                    Some(function) => function.to_owned(),
                    None => {
                        self.context.errors.push(CompilerError {
                            message: format!("Unknown function: {}", name.value),
                            span: self
                                .context
                                .db
                                .ast_id_to_span(self.context.input.clone(), name.id)
                                .unwrap(),
                        });
                        return self.push(name.id, Expression::Error, None);
                    }
                };
                self.push(
                    ast.id,
                    Expression::Call {
                        function,
                        arguments: argument_ids,
                    },
                    None,
                )
            }
            AstKind::Assignment(Assignment { name, body }) => {
                // let context = self.take_context();
                let mut inner = Compiler::<'a> {
                    context: self.context,
                    body: Body::new(),
                    parent_ids: add_ids(&self.parent_ids, self.next_id),
                    next_id: 0,
                    identifiers: self.identifiers.clone(),
                };
                inner.compile(&body);
                self.context = inner.context;
                self.push(
                    ast.id,
                    Expression::Body(inner.body),
                    Some(name.value.to_owned()),
                )
            }
            AstKind::Error => self.push(ast.id, Expression::Error, None),
        }
    }

    // fn take_context(&mut self) -> Context<'a> {
    //     std::mem::replace(
    //         &mut self.context,
    //         Context {
    //             db: self.context.db,
    //             input: self.context.input,
    //             cst: self.context.cst,
    //             ast_to_cst_id_mapping: self.context.ast_to_cst_id_mapping,
    //             id_mapping: self.context.id_mapping,
    //             errors: vec![],
    //         },
    //     )
    // }

    fn push(
        &mut self,
        ast_id: ast::Id,
        expression: Expression,
        identifier: Option<String>,
    ) -> hir::Id {
        let id = self.create_next_id(ast_id);
        self.body.push(id.clone(), expression, identifier.clone());
        self.context.id_mapping.insert(id.clone(), ast_id);
        if let Some(identifier) = identifier {
            self.identifiers.insert(identifier, id.clone());
        }
        id
    }
    fn push_without_ast_mapping(&mut self, expression: Expression) -> hir::Id {
        let id = self.create_next_id_without_ast_mapping();
        self.body.push(id.to_owned(), expression, None);
        id
    }

    fn create_next_id(&mut self, ast_id: ast::Id) -> hir::Id {
        let id = self.create_next_id_without_ast_mapping();
        assert!(matches!(
            self.context.id_mapping.insert(id.to_owned(), ast_id),
            None
        ));
        id
    }
    fn create_next_id_without_ast_mapping(&mut self) -> hir::Id {
        let id = hir::Id(add_ids(&self.parent_ids, self.next_id));
        self.next_id += 1;
        id
    }
}

fn add_ids(parents: &[usize], id: usize) -> Vec<usize> {
    parents.iter().map(|it| *it).chain(vec![id]).collect()
}
