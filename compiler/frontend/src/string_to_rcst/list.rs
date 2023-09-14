use super::{
    expression::{expression, ExpressionParsingOptions},
    literal::{closing_parenthesis, comma, opening_parenthesis},
    whitespace::whitespaces_and_newlines,
};
use crate::{
    cst::{CstError, CstKind, IsMultiline},
    rcst::Rcst,
};
use tracing::instrument;

#[instrument(level = "trace")]
pub fn list(input: &str, indentation: usize) -> Option<(&str, Rcst)> {
    let (input, mut opening_parenthesis) = opening_parenthesis(input)?;

    // Empty list `(,)` - TODO: Somehow optimize this
    'handleEmptyList: {
        // Whitespace before comma.
        let (input, leading_whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        let opening_parenthesis = opening_parenthesis
            .clone()
            .wrap_in_whitespace(leading_whitespace);

        // Comma.
        let Some((input, comma)) = comma(input) else {
            break 'handleEmptyList;
        };

        // Whitespace after comma.
        let (input, trailing_whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        let comma = comma.wrap_in_whitespace(trailing_whitespace);

        // Closing parenthesis.
        let Some((input, closing_parenthesis)) = closing_parenthesis(input) else {
            break 'handleEmptyList;
        };

        return Some((
            input,
            CstKind::List {
                opening_parenthesis: Box::new(opening_parenthesis),
                items: vec![comma],
                closing_parenthesis: Box::new(closing_parenthesis),
            }
            .into(),
        ));
    }

    let (input, mut items) = 'handleItems: {
        // Parse first item and check if this is a parenthesized expression
        let (input, whitespace) = whitespaces_and_newlines(input, indentation + 1, true);
        let item_indentation = if whitespace.is_multiline() {
            indentation + 1
        } else {
            indentation
        };
        opening_parenthesis = opening_parenthesis.wrap_in_whitespace(whitespace);

        let (input, first_expression) = expression(
            input,
            item_indentation,
            ExpressionParsingOptions {
                allow_assignment: false,
                allow_call: true,
                allow_bar: true,
                allow_function: true,
            },
        )
        .map_or((input, None), |(input, expression)| {
            (input, Some(expression))
        });

        let (new_input, whitespace) = whitespaces_and_newlines(input, item_indentation + 1, true);

        // It is a parenthesized expression if there is no comma
        let Some((mut input, first_comma)) = comma(new_input) else {
            let (input, whitespace) = whitespaces_and_newlines(input, indentation, true);
            let (input, closing_parenthesis) = closing_parenthesis(input).unwrap_or((
                input,
                CstKind::Error {
                    unparsable_input: String::new(),
                    error: CstError::ParenthesisNotClosed,
                }
                .into(),
            ));

            return Some((
                input,
                CstKind::Parenthesized {
                    opening_parenthesis: Box::new(opening_parenthesis),
                    inner: Box::new(
                        first_expression
                            .unwrap_or_else(|| {
                                CstKind::Error {
                                    unparsable_input: String::new(),
                                    error: CstError::OpeningParenthesisMissesExpression,
                                }
                                .into()
                            })
                            .wrap_in_whitespace(whitespace),
                    ),
                    closing_parenthesis: Box::new(closing_parenthesis),
                }
                .into(),
            ));
        };

        let mut items: Vec<Rcst> = vec![CstKind::ListItem {
            value: Box::new(
                first_expression
                    .unwrap_or_else(|| {
                        CstKind::Error {
                            unparsable_input: String::new(),
                            error: CstError::ListItemMissesValue,
                        }
                        .into()
                    })
                    .wrap_in_whitespace(whitespace),
            ),
            comma: Some(Box::new(first_comma)),
        }
        .into()];

        // Parse rest
        loop {
            let new_input = input;

            // Whitespace before value.
            let (new_input, whitespace) =
                whitespaces_and_newlines(new_input, indentation + 1, true);
            let item_indentation = if whitespace.is_multiline() {
                indentation + 1
            } else {
                indentation
            };
            let last = items.pop().unwrap();
            items.push(last.wrap_in_whitespace(whitespace));
            input = new_input;

            // Value.
            let (new_input, value, has_value) = match expression(
                new_input,
                item_indentation,
                ExpressionParsingOptions {
                    allow_assignment: false,
                    allow_call: true,
                    allow_bar: true,
                    allow_function: true,
                },
            ) {
                Some((new_input, value)) => (new_input, value, true),
                None => (
                    new_input,
                    CstKind::Error {
                        unparsable_input: String::new(),
                        error: CstError::ListItemMissesValue,
                    }
                    .into(),
                    false,
                ),
            };

            // Whitespace between value and comma.
            let (new_input, whitespace) =
                whitespaces_and_newlines(new_input, item_indentation + 1, true);
            let value = value.wrap_in_whitespace(whitespace);

            // Comma.
            let (new_input, comma) = match comma(new_input) {
                Some((new_input, comma)) => (new_input, Some(comma)),
                None => (new_input, None),
            };

            if !has_value && comma.is_none() {
                break 'handleItems (input, items);
            }

            input = new_input;
            items.push(
                CstKind::ListItem {
                    value: Box::new(value),
                    comma: comma.map(Box::new),
                }
                .into(),
            );
        }
    };

    let (new_input, whitespace) = whitespaces_and_newlines(input, indentation, true);

    let (input, closing_parenthesis) = match closing_parenthesis(new_input) {
        Some((input, closing_parenthesis)) => {
            if items.is_empty() {
                opening_parenthesis = opening_parenthesis.wrap_in_whitespace(whitespace);
            } else {
                let last = items.pop().unwrap();
                items.push(last.wrap_in_whitespace(whitespace));
            }
            (input, closing_parenthesis)
        }
        None => (
            input,
            CstKind::Error {
                unparsable_input: String::new(),
                error: CstError::ListNotClosed,
            }
            .into(),
        ),
    };

    Some((
        input,
        CstKind::List {
            opening_parenthesis: Box::new(opening_parenthesis),
            items,
            closing_parenthesis: Box::new(closing_parenthesis),
        }
        .into(),
    ))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::string_to_rcst::utils::{build_identifier, build_simple_int, build_simple_text};

    #[test]
    fn test_parenthesized() {
        assert_eq!(
            list("(foo)", 0),
            Some((
                "",
                CstKind::Parenthesized {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    inner: Box::new(build_identifier("foo")),
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
        assert_eq!(list("foo", 0), None);
        assert_eq!(
            list("(foo", 0),
            Some((
                "",
                CstKind::Parenthesized {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    inner: Box::new(build_identifier("foo")),
                    closing_parenthesis: Box::new(
                        CstKind::Error {
                            unparsable_input: String::new(),
                            error: CstError::ParenthesisNotClosed
                        }
                        .into()
                    ),
                }
                .into(),
            )),
        );
        assert_eq!(
            list("()", 0),
            Some((
                "",
                CstKind::Parenthesized {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    inner: Box::new(
                        CstKind::Error {
                            unparsable_input: String::new(),
                            error: CstError::OpeningParenthesisMissesExpression,
                        }
                        .into()
                    ),
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            ))
        );
    }

    #[test]
    fn test_list() {
        assert_eq!(list("hello", 0), None);
        assert_eq!(
            list("(,)", 0),
            Some((
                "",
                CstKind::List {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    items: vec![CstKind::Comma.into()],
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
        assert_eq!(
            list("(foo,)", 0),
            Some((
                "",
                CstKind::List {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    items: vec![CstKind::ListItem {
                        value: Box::new(build_identifier("foo")),
                        comma: Some(Box::new(CstKind::Comma.into())),
                    }
                    .into()],
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
        assert_eq!(
            list("(foo, )", 0),
            Some((
                "",
                CstKind::List {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    items: vec![CstKind::ListItem {
                        value: Box::new(build_identifier("foo")),
                        comma: Some(Box::new(CstKind::Comma.into())),
                    }
                    .with_trailing_space()],
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
        assert_eq!(
            list("(foo,bar)", 0),
            Some((
                "",
                CstKind::List {
                    opening_parenthesis: Box::new(CstKind::OpeningParenthesis.into()),
                    items: vec![
                        CstKind::ListItem {
                            value: Box::new(build_identifier("foo")),
                            comma: Some(Box::new(CstKind::Comma.into())),
                        }
                        .into(),
                        CstKind::ListItem {
                            value: Box::new(build_identifier("bar")),
                            comma: None,
                        }
                        .into(),
                    ],
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
        // (
        //   foo,
        //   4,
        //   "Hi",
        // )
        assert_eq!(
            list("(\n  foo,\n  4,\n  \"Hi\",\n)", 0),
            Some((
                "",
                CstKind::List {
                    opening_parenthesis: Box::new(
                        CstKind::OpeningParenthesis.with_trailing_whitespace(vec![
                            CstKind::Newline("\n".to_string()),
                            CstKind::Whitespace("  ".to_string()),
                        ]),
                    ),
                    items: vec![
                        CstKind::ListItem {
                            value: Box::new(build_identifier("foo")),
                            comma: Some(Box::new(CstKind::Comma.into())),
                        }
                        .with_trailing_whitespace(vec![
                            CstKind::Newline("\n".to_string()),
                            CstKind::Whitespace("  ".to_string())
                        ]),
                        CstKind::ListItem {
                            value: Box::new(build_simple_int(4)),
                            comma: Some(Box::new(CstKind::Comma.into())),
                        }
                        .with_trailing_whitespace(vec![
                            CstKind::Newline("\n".to_string()),
                            CstKind::Whitespace("  ".to_string())
                        ]),
                        CstKind::ListItem {
                            value: Box::new(build_simple_text("Hi")),
                            comma: Some(Box::new(CstKind::Comma.into()))
                        }
                        .with_trailing_whitespace(vec![CstKind::Newline("\n".to_string())]),
                    ],
                    closing_parenthesis: Box::new(CstKind::ClosingParenthesis.into()),
                }
                .into(),
            )),
        );
    }
}
