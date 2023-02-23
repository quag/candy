use enumset::EnumSet;

use super::{ast::AstError, cst, hir::HirError, rcst::RcstError};
use crate::{
    module::Module,
    position::Offset,
    rich_ir::{RichIrBuilder, ToRichIr},
};
use std::{fmt::Display, hash::Hash, ops::Range};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct CompilerError {
    pub module: Module,
    pub span: Range<Offset>,
    pub payload: CompilerErrorPayload,
}
impl Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} span({} – {}): {}",
            self.module.to_rich_ir(),
            *self.span.start,
            *self.span.end,
            self.payload,
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum CompilerErrorPayload {
    InvalidUtf8,
    Rcst(RcstError),
    Ast(AstError),
    Hir(HirError),
}
impl Display for CompilerErrorPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            CompilerErrorPayload::InvalidUtf8 => "The module contains invalid UTF-8.".to_string(),
            CompilerErrorPayload::Rcst(error) => match error {
                RcstError::CurlyBraceNotClosed => "The curly brace is not closed.",
                RcstError::IdentifierContainsNonAlphanumericAscii => {
                    "This identifier contains non-alphanumeric ASCII characters."
                }
                RcstError::IntContainsNonDigits => {
                    "This integer contains characters that are not digits."
                }
                RcstError::ListItemMissesValue => "This list item is missing a value.",
                RcstError::ListNotClosed => "The list is not closed.",
                RcstError::MatchMissesCases => "This match misses cases to match against.",
                RcstError::MatchCaseMissesArrow => "This match case misses an arrow.",
                RcstError::MatchCaseMissesBody => "This match case misses a body to run.",
                RcstError::OpeningParenthesisWithoutExpression => {
                    "Here's an opening parenthesis without an expression after it."
                }
                RcstError::OrPatternMissesRight => "This or-pattern misses a right-hand side.",
                RcstError::ParenthesisNotClosed => "This parenthesis isn't closed.",
                RcstError::PipeMissesCall => "There should be a call after this pipe.",
                RcstError::StructFieldMissesColon => "This struct field misses a colon.",
                RcstError::StructFieldMissesKey => "This struct field misses a key.",
                RcstError::StructFieldMissesValue => "This struct field misses a value.",
                RcstError::StructNotClosed => "This struct is not closed.",
                RcstError::SymbolContainsNonAlphanumericAscii => {
                    "This symbol contains non-alphanumeric ASCII characters."
                }
                RcstError::TextNotClosed => "This text isn't closed.",
                RcstError::TextNotSufficientlyIndented => "This text isn't sufficiently indented.",
                RcstError::TextInterpolationNotClosed => "This text interpolation isn't closed.",
                RcstError::TextInterpolationWithoutExpression => {
                    "Here's a start of a text interpolation without an expression after it."
                }
                RcstError::TooMuchWhitespace => "There is too much whitespace here.",
                RcstError::UnexpectedCharacters => "This is an unexpected character.",
                RcstError::UnparsedRest => "The parser couldn't parse this rest.",
                RcstError::WeirdWhitespace => "This is weird whitespace.",
                RcstError::WeirdWhitespaceInIndentation => {
                    "This is weird whitespace. Make sure to use indent using two spaces."
                }
            }
            .to_string(),
            CompilerErrorPayload::Ast(error) => match error {
                AstError::CallInPattern => "Calls in patterns are not allowed.".to_string(),
                AstError::ExpectedNameOrPatternInAssignment => {
                    "An assignment should have a name or pattern on the left side.".to_string()
                }
                AstError::ExpectedParameter => "A parameter should come here.".to_string(),
                AstError::LambdaWithoutClosingCurlyBrace => {
                    "This lambda doesn't have a closing curly brace.".to_string()
                }
                AstError::ListItemWithoutComma => {
                    "This list item should be followed by a comma.".to_string()
                }
                AstError::ListWithNonListItem => "This is not a list item.".to_string(),
                AstError::ListWithoutClosingParenthesis => {
                    "This list doesn't have a closing parenthesis.".to_string()
                }
                AstError::OrPatternIsMissingIdentifiers {
                    identifier,
                    number_of_missing_captures,
                    ..
                } => {
                    format!(
                        "`{identifier}` is missing in {number_of_missing_captures} {} of this or-pattern.",
                        if number_of_missing_captures.get() == 1 { "sub-pattern" } else { "sub-patterns" },
                    )
                }
                AstError::ParenthesizedInPattern => {
                    "Parentheses are not allowed in patterns.".to_string()
                }
                AstError::ParenthesizedWithoutClosingParenthesis => {
                    "This expression is parenthesized, but the closing parenthesis is missing."
                        .to_string()
                }
                AstError::PatternContainsInvalidExpression => {
                    "This type of expression is not allowed in patterns.".to_string()
                }
                AstError::PatternLiteralPartContainsInvalidExpression => {
                    "This type of expression is not allowed in this part of a pattern.".to_string()
                }
                AstError::PipeInPattern => "Pipes are not allowed in patterns.".to_string(),
                AstError::StructKeyWithoutColon => {
                    "This struct key should be followed by a colon.".to_string()
                }
                AstError::StructShorthandWithNotIdentifier => {
                    "Shorthand syntax in structs only supports identifiers.".to_string()
                }
                AstError::StructValueWithoutComma => {
                    "This struct value should be followed by a comma.".to_string()
                }
                AstError::StructWithNonStructField => {
                    "Structs should only contain struct key.".to_string()
                }
                AstError::StructWithoutClosingBrace => {
                    "This struct doesn't have a closing bracket.".to_string()
                }
                AstError::TextWithoutClosingQuote => "This text never ends.".to_string(),
                AstError::TextInterpolationWithoutClosingCurlyBraces => {
                    "This text interpolation never ends.".to_string()
                }
                AstError::UnexpectedPunctuation => "This punctuation was unexpected.".to_string(),
            },
            CompilerErrorPayload::Hir(error) => match error {
                HirError::NeedsWithWrongNumberOfArguments { num_args } => {
                    format!("`needs` accepts one or two arguments, but was called with {num_args} arguments. Its parameters are the `condition` and an optional `message`.")
                }
                HirError::PublicAssignmentInNotTopLevel => {
                    "Public assignments (:=) can only be used in top-level code.".to_string()
                }
                HirError::PublicAssignmentWithSameName { name } => {
                    format!("There already exists a public assignment (:=) named `{name}`.")
                }
                HirError::UnknownReference { name } => format!("`{name}`is not in scope."),
            },
        };
        write!(f, "{message}")
    }
}
impl CompilerError {
    pub fn to_related_information(&self) -> Vec<(Module, cst::Id, String)> {
        match &self.payload {
            CompilerErrorPayload::Ast(AstError::OrPatternIsMissingIdentifiers {
                all_captures,
                ..
            }) => all_captures
                .iter()
                .map(|capture| {
                    (
                        self.module.clone(),
                        capture.to_owned(),
                        "The identifier is bound here.".to_string(),
                    )
                })
                .collect(),
            _ => vec![],
        }
    }
}

impl ToRichIr for CompilerError {
    fn build_rich_ir(&self, builder: &mut RichIrBuilder) {
        // TODO: include more rich information
        builder.push(self.to_string(), None, EnumSet::empty());
    }
}
