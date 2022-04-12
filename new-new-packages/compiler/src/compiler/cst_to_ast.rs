use std::ops::Range;
use std::sync::Arc;

use im::HashMap;
use itertools::Itertools;

use super::ast::{
    self, Ast, AstError, AstKind, AstString, CollectErrors, Identifier, Int, Lambda, Symbol, Text,
};
use super::cst::{self, Cst, CstDb, CstKind};
use super::error::{CompilerError, CompilerErrorPayload};
use super::rcst_to_cst::RcstToCst;
use crate::compiler::ast::Struct;
use crate::compiler::cst::UnwrapWhitespaceAndComment;
use crate::input::Input;

#[salsa::query_group(CstToAstStorage)]
pub trait CstToAst: CstDb + RcstToCst {
    fn ast_to_cst_id(&self, id: ast::Id) -> Option<cst::Id>;
    fn ast_id_to_span(&self, id: ast::Id) -> Option<Range<usize>>;

    fn cst_to_ast_id(&self, input: Input, id: cst::Id) -> Option<ast::Id>;

    fn ast(&self, input: Input) -> Option<(Arc<Vec<Ast>>, HashMap<ast::Id, cst::Id>)>;
    fn ast_raw(&self, input: Input) -> Option<(Arc<Vec<Ast>>, HashMap<ast::Id, cst::Id>)>;
}

fn ast_to_cst_id(db: &dyn CstToAst, id: ast::Id) -> Option<cst::Id> {
    let (_, ast_to_cst_id_mapping) = db.ast(id.input.clone()).unwrap();
    ast_to_cst_id_mapping.get(&id).cloned()
}
fn ast_id_to_span(db: &dyn CstToAst, id: ast::Id) -> Option<Range<usize>> {
    let cst_id = db.ast_to_cst_id(id.clone())?;
    Some(db.find_cst(id.input, cst_id).span)
}

fn cst_to_ast_id(db: &dyn CstToAst, input: Input, id: cst::Id) -> Option<ast::Id> {
    let (_, ast_to_cst_id_mapping) = db.ast(input).unwrap();
    ast_to_cst_id_mapping
        .iter()
        .find_map(|(key, &value)| if value == id { Some(key) } else { None })
        .cloned()
}

fn ast(db: &dyn CstToAst, input: Input) -> Option<(Arc<Vec<Ast>>, HashMap<ast::Id, cst::Id>)> {
    db.ast_raw(input).map(|(ast, id_mapping)| (ast, id_mapping))
}
fn ast_raw(db: &dyn CstToAst, input: Input) -> Option<(Arc<Vec<Ast>>, HashMap<ast::Id, cst::Id>)> {
    let cst = db.cst(input.clone())?;
    let cst = cst.unwrap_whitespace_and_comment();
    let mut context = LoweringContext::new(input);
    let asts = (&mut context).lower_csts(&cst);
    Some((Arc::new(asts), context.id_mapping))
}

struct LoweringContext {
    input: Input,
    next_id: usize,
    id_mapping: HashMap<ast::Id, cst::Id>,
}
impl LoweringContext {
    fn new(input: Input) -> LoweringContext {
        LoweringContext {
            input,
            next_id: 0,
            id_mapping: HashMap::new(),
        }
    }
    fn lower_csts(&mut self, csts: &[Cst]) -> Vec<Ast> {
        csts.iter().map(|it| self.lower_cst(it)).collect()
    }
    fn lower_cst(&mut self, cst: &Cst) -> Ast {
        match &cst.kind {
            CstKind::EqualsSign
            | CstKind::Comma
            | CstKind::Colon
            | CstKind::OpeningParenthesis
            | CstKind::ClosingParenthesis
            | CstKind::OpeningBracket
            | CstKind::ClosingBracket
            | CstKind::OpeningCurlyBrace
            | CstKind::ClosingCurlyBrace
            | CstKind::Arrow
            | CstKind::DoubleQuote
            | CstKind::Octothorpe => self.create_ast(
                cst.id,
                AstKind::Error {
                    child: None,
                    errors: vec![CompilerError {
                        span: cst.span.clone(),
                        payload: CompilerErrorPayload::Ast(AstError::UnexpectedPunctuation),
                    }],
                },
            ),
            CstKind::Whitespace(_)
            | CstKind::Newline(_)
            | CstKind::Comment { .. }
            | CstKind::TrailingWhitespace { .. } => {
                panic!("Whitespace should have been removed before lowering to AST.")
            }
            CstKind::Identifier(identifier) => {
                let string = self.create_string_without_id_mapping(identifier.to_string());
                self.create_ast(cst.id, AstKind::Identifier(Identifier(string)))
            }
            CstKind::Symbol(symbol) => {
                let string = self.create_string_without_id_mapping(symbol.to_string());
                self.create_ast(cst.id, AstKind::Symbol(Symbol(string)))
            }
            CstKind::Int(value) => self.create_ast(cst.id, AstKind::Int(Int(*value))),
            CstKind::Text {
                opening_quote,
                parts,
                closing_quote,
            } => {
                assert!(
                    matches!(opening_quote.kind, CstKind::DoubleQuote),
                    "Text needs to start with opening double quote, but started with {}.",
                    opening_quote
                );

                let text = parts
                    .into_iter()
                    .filter_map(|it| match it {
                        Cst {
                            kind: CstKind::TextPart(text),
                            ..
                        } => Some(text),
                        _ => panic!("Text contains non-TextPart. Whitespaces should have been removed already."),
                    })
                    .join("");
                let string = self.create_string_without_id_mapping(text);
                let mut text = self.create_ast(cst.id, AstKind::Text(Text(string)));

                if !matches!(closing_quote.kind, CstKind::DoubleQuote) {
                    text = self.create_ast(
                        closing_quote.id,
                        AstKind::Error {
                            child: None,
                            errors: vec![CompilerError {
                                span: closing_quote.span.clone(),
                                payload: CompilerErrorPayload::Ast(
                                    AstError::TextWithoutClosingQuote,
                                ),
                            }],
                        },
                    );
                }

                text
            }
            CstKind::TextPart(_) => panic!("TextPart should only occur in Text."),
            CstKind::Parenthesized {
                opening_parenthesis,
                inner,
                closing_parenthesis,
            } => {
                let mut ast = self.lower_cst(inner);

                assert!(
                    matches!(opening_parenthesis.kind, CstKind::OpeningParenthesis),
                    "Parenthesized needs to start with opening parenthesis, but started with {}.",
                    opening_parenthesis
                );
                if !matches!(closing_parenthesis.kind, CstKind::ClosingParenthesis) {
                    ast = self.create_ast(
                        closing_parenthesis.id,
                        AstKind::Error {
                            child: None,
                            errors: vec![CompilerError {
                                span: closing_parenthesis.span.clone(),
                                payload: CompilerErrorPayload::Ast(
                                    AstError::ParenthesizedWithoutClosingParenthesis,
                                ),
                            }],
                        },
                    );
                }

                ast
            }
            CstKind::Call { name, arguments } => {
                let name_string = if let CstKind::Identifier(identifier) = &name.kind {
                    Some(self.create_string(cst.id.to_owned(), identifier.to_owned()))
                } else {
                    None
                };
                let arguments = self.lower_csts(arguments);

                if let Some(name) = name_string {
                    self.create_ast(cst.id, AstKind::Call(ast::Call { name, arguments }))
                } else {
                    let mut errors = vec![];
                    errors.push(CompilerError {
                        span: name.span.clone(),
                        payload: CompilerErrorPayload::Ast(AstError::CallOfANonIdentifier),
                    });
                    arguments.collect_errors(&mut errors);
                    self.create_ast(
                        cst.id,
                        AstKind::Error {
                            child: None,
                            errors,
                        },
                    )
                }
            }
            CstKind::Struct {
                opening_bracket,
                fields,
                closing_bracket,
            } => {
                let mut errors = vec![];

                assert!(
                    !matches!(opening_bracket.kind, CstKind::OpeningBracket),
                    "Struct should always have an opening bracket, but instead had {}.",
                    opening_bracket
                );

                let fields = fields
                    .into_iter()
                    .filter_map(|field| {
                        if let CstKind::StructField {
                            key,
                            colon,
                            value,
                            comma,
                        } = &field.kind
                        {
                            let mut key = self.lower_cst(&key.clone());
                            let mut value = self.lower_cst(&value.clone());

                            if !matches!(colon.kind, CstKind::Colon) {
                                key = self.create_ast(
                                    colon.id,
                                    AstKind::Error {
                                        child: None,
                                        errors: vec![CompilerError {
                                            span: colon.span.clone(),
                                            payload: CompilerErrorPayload::Ast(
                                                AstError::ColonMissingAfterStructKey,
                                            ),
                                        }],
                                    },
                                )
                            }
                            if let Some(comma) = comma {
                                if !matches!(comma.kind, CstKind::Comma) {
                                    value = self.create_ast(
                                        comma.id,
                                        AstKind::Error {
                                            child: None,
                                            errors: vec![CompilerError {
                                                span: comma.span.clone(),
                                                payload: CompilerErrorPayload::Ast(
                                                    AstError::NonCommaAfterStructValue,
                                                ),
                                            }],
                                        },
                                    )
                                }
                            }

                            Some((key, value))
                        } else {
                            errors.push(CompilerError {
                                span: cst.span.clone(),
                                payload: CompilerErrorPayload::Ast(
                                    AstError::StructWithNonStructField,
                                ),
                            });
                            None
                        }
                    })
                    .collect();

                if !matches!(closing_bracket.kind, CstKind::ClosingBracket) {
                    errors.push(CompilerError {
                        span: closing_bracket.span.clone(),
                        payload: CompilerErrorPayload::Ast(AstError::StructWithoutClosingBrace),
                    });
                }

                let ast = self.create_ast(cst.id, AstKind::Struct(Struct { fields }));
                if errors.is_empty() {
                    ast
                } else {
                    self.create_ast(
                        cst.id,
                        AstKind::Error {
                            child: Some(Box::new(ast)),
                            errors,
                        },
                    )
                }
            }
            CstKind::StructField { .. } => panic!("StructField should only appear in Struct."),
            CstKind::Lambda {
                opening_curly_brace,
                parameters_and_arrow,
                body,
                closing_curly_brace,
            } => {
                assert!(
                    matches!(opening_curly_brace.kind, CstKind::OpeningCurlyBrace),
                    "Expected an opening curly brace at the beginning of a lambda, but found {}.",
                    opening_curly_brace,
                );

                let (parameters, mut errors) =
                    if let Some((parameters, arrow)) = parameters_and_arrow {
                        assert!(
                            matches!(arrow.kind, CstKind::Arrow),
                            "Expected an arrow after the parameters in a lambda, but found `{}`.",
                            arrow
                        );
                        self.lower_parameters(parameters)
                    } else {
                        (vec![], vec![])
                    };

                let body = self.lower_csts(body);

                if !matches!(closing_curly_brace.kind, CstKind::ClosingCurlyBrace) {
                    errors.push(CompilerError {
                        span: closing_curly_brace.span.clone(),
                        payload: CompilerErrorPayload::Ast(
                            AstError::LambdaWithoutClosingCurlyBrace,
                        ),
                    });
                }

                let mut ast = self.create_ast(cst.id, AstKind::Lambda(Lambda { parameters, body }));
                if !errors.is_empty() {
                    ast = self.create_ast(
                        cst.id,
                        AstKind::Error {
                            child: None,
                            errors,
                        },
                    );
                }
                ast
            }
            CstKind::Assignment {
                name,
                parameters,
                equals_sign,
                body,
            } => {
                let name = self.lower_identifier(name);
                let (parameters, errors) = self.lower_parameters(parameters);

                assert!(
                    matches!(equals_sign.kind, CstKind::EqualsSign),
                    "Expected an equals sign for the assignment, but found {} instead.",
                    equals_sign,
                );

                let mut body = self.lower_csts(body);

                if !parameters.is_empty() {
                    body =
                        vec![self.create_ast(cst.id, AstKind::Lambda(Lambda { parameters, body }))];
                }

                let mut ast =
                    self.create_ast(cst.id, AstKind::Assignment(ast::Assignment { name, body }));
                if !errors.is_empty() {
                    ast = self.create_ast(
                        cst.id,
                        AstKind::Error {
                            child: None,
                            errors,
                        },
                    );
                }
                ast
            }
            CstKind::Error { error, .. } => self.create_ast(
                cst.id,
                AstKind::Error {
                    child: None,
                    errors: vec![CompilerError {
                        span: cst.span.clone(),
                        payload: CompilerErrorPayload::Rcst(error.clone()),
                    }],
                },
            ),
        }
    }

    fn lower_parameters(&mut self, csts: &[Cst]) -> (Vec<AstString>, Vec<CompilerError>) {
        let mut errors = vec![];
        let parameters = csts
            .into_iter()
            .enumerate()
            .map(|(index, it)| match self.lower_parameter(it) {
                Ok(parameter) => parameter,
                Err(error) => {
                    errors.push(error);
                    self.create_string(it.id, format!("<invalid#{}", index))
                }
            })
            .collect();
        (parameters, errors)
    }
    fn lower_parameter(&mut self, cst: &Cst) -> Result<AstString, CompilerError> {
        if let CstKind::Identifier(identifier) = &cst.kind {
            Ok(self.create_string(cst.id.to_owned(), identifier.clone()))
        } else {
            Err(CompilerError {
                span: cst.span.clone(),
                payload: CompilerErrorPayload::Ast(AstError::ExpectedParameter),
            })
        }
    }
    fn lower_identifier(&mut self, cst: &Cst) -> AstString {
        match cst {
            Cst {
                id,
                kind: CstKind::Identifier(identifier),
                ..
            } => self.create_string(id.to_owned(), identifier.clone()),
            _ => {
                panic!("Expected an identifier, but found `{}`.", cst);
            }
        }
    }

    fn create_ast(&mut self, cst_id: cst::Id, kind: AstKind) -> Ast {
        Ast {
            id: self.create_next_id(cst_id),
            kind,
        }
    }
    fn create_string(&mut self, cst_id: cst::Id, value: String) -> AstString {
        AstString {
            id: self.create_next_id(cst_id),
            value,
        }
    }
    fn create_string_without_id_mapping(&mut self, value: String) -> AstString {
        AstString {
            id: self.create_next_id_without_mapping(),
            value,
        }
    }
    fn create_next_id(&mut self, cst_id: cst::Id) -> ast::Id {
        let id = self.create_next_id_without_mapping();
        assert!(matches!(self.id_mapping.insert(id.clone(), cst_id), None));
        id
    }
    fn create_next_id_without_mapping(&mut self) -> ast::Id {
        let id = ast::Id::new(self.input.clone(), self.next_id);
        self.next_id += 1;
        id
    }
}
