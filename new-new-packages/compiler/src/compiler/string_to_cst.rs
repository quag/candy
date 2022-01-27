use super::cst::*;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while_m_n},
    character::complete::{alphanumeric0, anychar, line_ending, not_line_ending, space1},
    combinator::{map, opt, recognize, success, verify},
    multi::{count, many0},
    sequence::{delimited, tuple},
    IResult, Offset, Parser,
};
use nom_supreme::{error::ErrorTree, final_parser::final_parser, ParserExt};
use proptest::prelude::*;

type ParserResult<'a, T> = IResult<&'a str, T, ErrorTree<&'a str>>;

pub trait StringToCst {
    fn parse_cst(&self) -> Vec<Cst>;
}
impl StringToCst for str {
    fn parse_cst(&self) -> Vec<Cst> {
        // TODO: handle trailing whitespace and comments properly
        let source = format!("\n{}", self);
        let parser = map(
            tuple((|input| expressions0(&source, input, 0), many0(line_ending))),
            |(csts, _)| csts,
        );
        let result: Result<_, ErrorTree<&str>> = final_parser(parser)(&source);
        match result {
            Ok(mut csts) => {
                // TODO: remove the leading newline we inserted above
                fix_offsets_csts(&mut 0, &mut csts);
                csts
            }
            Err(err) => vec![create_cst(CstKind::Error {
                offset: 0,
                unparsable_input: self.to_owned(),
                message: format!("An error occurred while parsing: {:?}", err),
            })],
        }
    }
}

/// Because we don't parse the input directly, but prepend a newline to it, we
/// need to adjust the offsets of the CSTs to account for that.
fn fix_offsets_csts(next_id: &mut usize, csts: &mut Vec<Cst>) {
    for cst in csts {
        fix_offsets_cst(next_id, cst)
    }
}
fn fix_offsets_cst(next_id: &mut usize, cst: &mut Cst) {
    cst.id = Id(next_id.to_owned());
    *next_id += 1;
    match &mut cst.kind {
        CstKind::EqualsSign { offset } => *offset -= 1,
        CstKind::OpeningParenthesis { offset } => *offset -= 1,
        CstKind::ClosingParenthesis { offset } => *offset -= 1,
        CstKind::OpeningCurlyBrace { offset } => *offset -= 1,
        CstKind::ClosingCurlyBrace { offset } => *offset -= 1,
        CstKind::Arrow { offset } => *offset -= 1,
        CstKind::Int { offset, .. } => *offset -= 1,
        CstKind::Text { offset, .. } => *offset -= 1,
        CstKind::Identifier { offset, .. } => *offset -= 1,
        CstKind::Symbol { offset, .. } => *offset -= 1,
        CstKind::LeadingWhitespace { child, .. } => fix_offsets_cst(next_id, &mut *child),
        CstKind::LeadingComment { child, .. } => fix_offsets_cst(next_id, &mut *child),
        CstKind::TrailingWhitespace { child, .. } => fix_offsets_cst(next_id, &mut *child),
        CstKind::TrailingComment { child, .. } => fix_offsets_cst(next_id, &mut *child),
        CstKind::Parenthesized {
            opening_parenthesis,
            inner,
            closing_parenthesis,
        } => {
            fix_offsets_cst(next_id, &mut *opening_parenthesis);
            fix_offsets_cst(next_id, &mut *inner);
            fix_offsets_cst(next_id, &mut *closing_parenthesis);
        }
        CstKind::Lambda {
            opening_curly_brace,
            parameters_and_arrow,
            body,
            closing_curly_brace,
        } => {
            fix_offsets_cst(next_id, &mut *opening_curly_brace);
            match parameters_and_arrow {
                Some((arguments, arrow)) => {
                    fix_offsets_csts(next_id, arguments);
                    fix_offsets_cst(next_id, &mut *arrow);
                }
                None => {}
            };
            fix_offsets_csts(next_id, body);
            fix_offsets_cst(next_id, &mut *closing_curly_brace);
        }
        CstKind::Call { name, arguments } => {
            fix_offsets_cst(next_id, &mut *name);
            fix_offsets_csts(next_id, arguments);
        }
        CstKind::Assignment {
            name,
            parameters,
            equals_sign,
            body,
        } => {
            fix_offsets_cst(next_id, &mut *name);
            fix_offsets_csts(next_id, parameters);
            fix_offsets_cst(next_id, &mut *equals_sign);
            fix_offsets_csts(next_id, body);
        }
        CstKind::Error { offset, .. } => *offset -= 1,
    };
}

fn expressions1<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
) -> ParserResult<'a, Vec<Cst>> {
    verify(
        |input| expressions0(source, input, indentation),
        |csts: &Vec<Cst>| !csts.is_empty(),
    )
    .context("expressions1")
    .parse(input)
}
fn expressions0<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
) -> ParserResult<'a, Vec<Cst>> {
    many0(|input| {
        leading_whitespace_and_comment_and_empty_lines(
            source,
            input,
            indentation,
            1,
            |source, input, indentation| {
                trailing_whitespace_and_comment(input, |input| {
                    leading_indentation(input, indentation, |input| {
                        expression(source, input, indentation)
                    })
                })
            },
        )
    })
    .context("expressions0")
    .parse(input)
}

fn expression<'a>(source: &'a str, input: &'a str, indentation: usize) -> ParserResult<'a, Cst> {
    alt((
        |input| int(source, input),
        |input| text(source, input),
        |input| symbol(source, input),
        |input| parenthesized(source, input, indentation),
        |input| lambda(source, input, indentation),
        |input| assignment(source, input, indentation),
        |input| call(source, input, indentation),
        // TODO: catch-all
    ))
    .context("expression")
    .parse(input)
}

// Simple characters.

fn equals_sign<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "equals_sign", "=", |offset| {
        CstKind::EqualsSign { offset }
    })
}

fn opening_parenthesis<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "opening_parenthesis", "(", |offset| {
        CstKind::OpeningParenthesis { offset }
    })
}

fn closing_parenthesis<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "closing_parenthesis", ")", |offset| {
        CstKind::ClosingParenthesis { offset }
    })
}

fn opening_curly_brace<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "opening_curly_brace", "{", |offset| {
        CstKind::OpeningCurlyBrace { offset }
    })
}

fn closing_curly_brace<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "closing_curly_brace", "}", |offset| {
        CstKind::ClosingCurlyBrace { offset }
    })
}
fn arrow<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    parse_symbol(source, input, "arrow", "->", |offset| CstKind::Arrow {
        offset,
    })
}

fn parse_symbol<'a, F>(
    source: &'a str,
    input: &'a str,
    name: &'static str,
    symbol: &'static str,
    mut mapper: F,
) -> ParserResult<'a, Cst>
where
    F: FnMut(usize) -> CstKind,
{
    map(
        |input| with_offset(source, input, tag(symbol)),
        |(offset, _)| create_cst((&mut mapper(offset)).clone()),
    )
    .context(name)
    .parse(input)
}

// Self-contained atoms of the language.

fn int<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    map(
        |input| {
            with_offset(
                source,
                input,
                take_while_m_n(1, 64, |c: char| c.is_digit(10)),
            )
        },
        |(offset, input)| {
            let value = u64::from_str_radix(input, 10).expect("Couldn't parse int.");
            create_cst(CstKind::Int {
                offset,
                value,
                source: input.to_owned(),
            })
        },
    )
    .context("int")
    .parse(input)
}

fn text<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    map(
        |input| {
            with_offset(
                source,
                input,
                delimited(tag("\""), take_while(|it| it != '\"'), tag("\"")),
            )
        },
        |(offset, string)| {
            create_cst(CstKind::Text {
                offset,
                value: string.to_owned(),
            })
        },
    )
    .context("text")
    .parse(input)
}

fn identifier<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    map(
        |input| {
            with_offset(
                source,
                input,
                recognize(tuple((
                    verify(anychar, |it| it.is_lowercase()),
                    alphanumeric0,
                ))),
            )
        },
        |(offset, value)| {
            create_cst(CstKind::Identifier {
                offset,
                value: value.to_owned(),
            })
        },
    )
    .context("identifier")
    .parse(input)
}

fn symbol<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    map(
        |input| {
            with_offset(
                source,
                input,
                recognize(tuple((
                    verify(anychar, |it| it.is_uppercase()),
                    alphanumeric0,
                ))),
            )
        },
        |(offset, value)| {
            create_cst(CstKind::Symbol {
                offset,
                value: value.to_owned(),
            })
        },
    )
    .context("symbol")
    .parse(input)
}

// Decorators.

fn leading_indentation<'a, F>(
    input: &'a str,
    indentation: usize,
    mut parser: F,
) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        if indentation == 0 {
            return parser(input);
        }

        let (input, indent) = recognize(count(tag("  "), indentation))(input)?;
        let (input, child) = &mut parser(input)?;
        Ok((
            *input,
            create_cst(CstKind::LeadingWhitespace {
                value: indent.to_owned(),
                child: Box::new(child.clone()),
            }),
        ))
    })
    .context("leading_indentation")
    .parse(input)
}

fn leading_whitespace_and_comment_and_empty_lines<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
    min_newlines: usize,
    parser: fn(&'a str, &'a str, usize) -> ParserResult<'a, Cst>,
) -> ParserResult<'a, Cst> {
    let parser: Box<dyn FnMut(&'a str) -> ParserResult<Cst>> = if min_newlines > 0 {
        Box::new(move |input| {
            with_leading_newlines(source, input, indentation, min_newlines, parser)
        })
    } else {
        Box::new(|input| {
            match with_leading_newlines(source, input, indentation, min_newlines, parser) {
                Ok(it) => Ok(it),
                Err(_) => parser(source, input, indentation),
            }
        })
    };
    parser
        .context("leading_whitespace_and_comment_and_empty_lines")
        .parse(input)
}
fn with_leading_newlines<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
    min_newlines: usize,
    parser: fn(&'a str, &'a str, usize) -> ParserResult<'a, Cst>,
) -> ParserResult<'a, Cst> {
    leading_whitespace(input, |input| {
        leading_comment(
            input,
            map(
                tuple((line_ending, |input| {
                    leading_whitespace_and_comment_and_empty_lines(
                        source,
                        input,
                        indentation,
                        if min_newlines > 0 {
                            min_newlines - 1
                        } else {
                            0
                        },
                        parser,
                    )
                })),
                |(line_break, child)| {
                    create_cst(CstKind::LeadingWhitespace {
                        value: line_break.to_owned(),
                        child: Box::new(child),
                    })
                },
            ),
        )
    })
}
fn leading_whitespace<'a, F>(input: &'a str, mut parser: F) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        let space_result: IResult<_, _, ErrorTree<&'a str>> = space1(input);
        let (input, result) = match space_result {
            Ok((input, space)) => {
                let (input, child) = parser(input)?;
                (
                    input,
                    create_cst(CstKind::LeadingWhitespace {
                        value: space.to_owned(),
                        child: Box::new(child),
                    }),
                )
            }
            Err(_) => parser(input)?,
        };
        Ok((input, result))
    })
    .context("leading_whitespace")
    .parse(input)
}
fn leading_comment<'a, F>(input: &'a str, mut parser: F) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        let comment_result: IResult<_, _> = tuple((tag("#"), not_line_ending))(input);
        let (input, comment) = match comment_result {
            Ok((input, (_, comment))) => (input, comment),
            Err(_) => return parser(input),
        };

        let (input, child) = parser(input)?;
        let result = create_cst(CstKind::LeadingComment {
            value: comment.to_owned(),
            child: Box::new(child),
        });
        Ok((input, result))
    })
    .context("leading_comment")
    .parse(input)
}

fn trailing_whitespace_and_comment_and_empty_lines<'a, F>(
    input: &'a str,
    mut parser: F,
) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        let (mut input, mut child) = trailing_whitespace_and_comment(input, &mut parser)?;

        loop {
            let result = trailing_whitespace_and_comment(input, |input| {
                let (input, line_break) = line_ending(input)?;

                Ok((
                    input,
                    create_cst(CstKind::TrailingWhitespace {
                        child: Box::new(child.clone()),
                        value: line_break.to_owned(),
                    }),
                ))
            });
            match result {
                Ok((remaining_input, inner_child)) => {
                    input = remaining_input;
                    child = inner_child;
                }
                Err(_) => break,
            }
        }
        Ok((input, child))
    })
    .context("trailing_whitespace_and_comment_and_empty_lines")
    .parse(input)
}
fn trailing_whitespace_and_comment<'a, F>(input: &'a str, mut parser: F) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| trailing_comment(input, |input| trailing_whitespace(input, &mut parser)))
        .context("trailing_whitespace_and_comment")
        .parse(input)
}
fn trailing_whitespace<'a, F>(input: &'a str, mut parser: F) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        let (input, child) = parser(input)?;

        let space_result: IResult<_, _, ErrorTree<&'a str>> = space1(input);
        let (input, result) = match space_result {
            Ok((remaining, space)) => (
                remaining,
                create_cst(CstKind::TrailingWhitespace {
                    child: Box::new(child),
                    value: space.to_owned(),
                }),
            ),
            Err(_) => (input, child),
        };
        Ok((input, result))
    })
    .context("trailing_whitespace")
    .parse(input)
}
fn trailing_comment<'a, F>(input: &'a str, mut parser: F) -> ParserResult<'a, Cst>
where
    F: FnMut(&'a str) -> ParserResult<'a, Cst>,
{
    (|input| {
        let (input, child) = parser(input)?;

        let comment_result: IResult<_, _, ErrorTree<&'a str>> =
            tuple((tag("#"), not_line_ending))(input);
        let (input, result) = match comment_result {
            Ok((remaining, (_, comment))) => (
                remaining,
                create_cst(CstKind::TrailingComment {
                    child: Box::new(child),
                    value: comment.to_owned(),
                }),
            ),
            Err(_) => (input, child),
        };
        Ok((input, result))
    })
    .context("trailing_comment")
    .parse(input)
}

// Compound expressions.

fn parenthesized<'a>(source: &'a str, input: &'a str, indentation: usize) -> ParserResult<'a, Cst> {
    map(
        tuple((
            |input| opening_parenthesis(source, input),
            |input| expression(source, input, indentation),
            |input| closing_parenthesis(source, input),
        )),
        |(opening_parenthesis, inner, closing_parenthesis)| {
            create_cst(CstKind::Parenthesized {
                opening_parenthesis: Box::new(opening_parenthesis),
                inner: Box::new(inner),
                closing_parenthesis: Box::new(closing_parenthesis),
            })
        },
    )
    .context("parenthesized")
    .parse(input)
}

fn lambda<'a>(source: &'a str, input: &'a str, indentation: usize) -> ParserResult<'a, Cst> {
    map(
        tuple((
            |input| {
                trailing_whitespace_and_comment_and_empty_lines(input, |input| {
                    opening_curly_brace(source, input)
                })
            },
            opt(tuple((
                |input| parameters(source, input, indentation),
                map(
                    |input| trailing_whitespace_and_comment(input, |input| arrow(source, input)),
                    |it| Box::new(it),
                ),
            ))),
            alt((
                |input| expressions1(source, input, indentation + 1),
                map(
                    |input| {
                        trailing_whitespace_and_comment_and_empty_lines(input, |input| {
                            expression(source, input, indentation)
                        })
                    },
                    |cst| vec![cst],
                ),
                success(vec![]),
            )),
            |input| {
                leading_whitespace_and_comment_and_empty_lines(
                    source,
                    input,
                    indentation,
                    0,
                    |source, input, _indentation| {
                        trailing_whitespace_and_comment(input, |input| {
                            closing_curly_brace(source, input)
                        })
                    },
                )
            },
        )),
        |(opening_curly_brace, parameters_and_arrow, body, closing_curly_brace)| {
            create_cst(CstKind::Lambda {
                opening_curly_brace: Box::new(opening_curly_brace),
                parameters_and_arrow,
                body,
                closing_curly_brace: Box::new(closing_curly_brace),
            })
        },
    )
    .context("lambda")
    .parse(input)
}
fn parameters<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
) -> ParserResult<'a, Vec<Cst>> {
    many0(|input| {
        trailing_whitespace_and_comment_and_empty_lines(
            input,
            alt((
                |input| int(source, input),
                |input| text(source, input),
                |input| symbol(source, input),
                |input| parenthesized(source, input, indentation),
                // TODO: only allow single-line lambdas
                |input| lambda(source, input, indentation),
                |input| call_without_arguments(source, input),
                // TODO: catch-all
            )),
        )
    })
    .context("arguments")
    .parse(input)
}

fn call<'a>(source: &'a str, input: &'a str, indentation: usize) -> ParserResult<'a, Cst> {
    map(
        tuple((
            |input| trailing_whitespace_and_comment(input, |input| identifier(source, input)),
            alt((
                |input| expressions1(source, input, indentation + 1),
                |input| arguments(source, input, indentation),
            )),
        )),
        |(name, arguments)| {
            create_cst(CstKind::Call {
                name: Box::new(name),
                arguments,
            })
        },
    )
    .context("call")
    .parse(input)
}
fn call_without_arguments<'a>(source: &'a str, input: &'a str) -> ParserResult<'a, Cst> {
    map(
        |input| trailing_whitespace_and_comment(input, |input| identifier(source, input)),
        |name| {
            create_cst(CstKind::Call {
                name: Box::new(name),
                arguments: vec![],
            })
        },
    )
    .context("call_without_arguments")
    .parse(input)
}
fn arguments<'a>(
    source: &'a str,
    input: &'a str,
    indentation: usize,
) -> ParserResult<'a, Vec<Cst>> {
    many0(|input| {
        trailing_whitespace_and_comment(
            input,
            alt((
                |input| int(source, input),
                |input| text(source, input),
                |input| symbol(source, input),
                |input| parenthesized(source, input, indentation),
                // TODO: only allow single-line lambdas
                |input| lambda(source, input, indentation),
                |input| call_without_arguments(source, input),
                // TODO: catch-all
            )),
        )
    })
    .context("arguments")
    .parse(input)
}
fn assignment<'a>(source: &'a str, input: &'a str, indentation: usize) -> ParserResult<'a, Cst> {
    (|input| {
        let (input, left) =
            trailing_whitespace_and_comment(input, |input| call(source, input, indentation))?;
        let (name, parameters) = match left {
            Cst {
                kind: CstKind::Call { name, arguments },
                ..
            } => (name, arguments),
            _ => panic!("`call` did not return a `CstKind::Call`."),
        };
        let (input, equals_sign) =
            trailing_whitespace_and_comment(input, |input| equals_sign(source, input))?;

        let (input, body) = alt((
            |input| expressions1(source, input, indentation + 1),
            map(
                |input| expression(source, input, indentation),
                |cst| vec![cst],
            ),
            success(vec![]),
        ))(input)?;
        Ok((
            input,
            create_cst(CstKind::Assignment {
                name,
                parameters,
                equals_sign: Box::new(equals_sign),
                body,
            }),
        ))
    })
    .context("assignment")
    .parse(input)
}

pub fn with_offset<'a, O, F>(
    source: &'a str,
    input: &'a str,
    mut parser: F,
) -> ParserResult<'a, (usize, O)>
where
    F: FnMut(&'a str) -> ParserResult<O>,
{
    (move |input: &'a str| match parser.parse(input) {
        Ok((remaining, result)) => {
            let offset = source.offset(&input);
            Ok((remaining, (offset, result)))
        }
        Err(e) => Err(e),
    })
    .context("with_offset")
    .parse(input)
}

fn create_cst(kind: CstKind) -> Cst {
    Cst { id: Id(0), kind }
}

proptest! {
    #[test]
    fn test_int(value in 0u64..) {
        let string = value.to_string();
        prop_assert_eq!(int(&string, &string).unwrap(), ("", create_cst(CstKind::Int{offset: 0, value: value, source: string.clone()})));
    }
    #[test]
    fn test_text(value in "[\\w\\d\\s]*") {
        let stringified_text = format!("\"{}\"", value);
        prop_assert_eq!(text(&stringified_text, &stringified_text).unwrap(), ("", create_cst(CstKind::Text{offset: 0, value: value.clone()})));
    }
    #[test]
    fn test_symbol(value in "[A-Z][A-Za-z0-9]*") {
        prop_assert_eq!(symbol(&value, &value).unwrap(), ("", create_cst(CstKind::Symbol{ offset: 0, value: value.clone()})));
    }
    #[test]
    fn test_identifier(value in "[a-z][A-Za-z0-9]*") {
        prop_assert_eq!(identifier(&value, &value).unwrap(), ("", create_cst(CstKind::Identifier{ offset: 0, value: value.clone()})));
    }
}

#[test]
fn test_indented() {
    fn parse(source: &str, indentation: usize) -> (&str, Cst) {
        leading_indentation(source, indentation, |input| int(source, input)).unwrap()
    }
    assert_eq!(
        parse("123", 0),
        (
            "",
            create_cst(CstKind::LeadingWhitespace {
                value: "".to_owned(),
                child: Box::new(create_cst(CstKind::Int {
                    offset: 0,
                    value: 123,
                    source: "123".to_owned()
                })),
            })
        )
    );
    // assert_eq!(
    //     parse("  123", 0),
    //     (
    //         "  ",
    //         CstKind::LeadingWhitespace {
    //             value: "".to_owned(),
    //             child: Box::new(CstKind::Int { value: 123 })
    //         }
    //     )
    // );
    assert_eq!(
        parse("  123", 1),
        (
            "",
            create_cst(CstKind::LeadingWhitespace {
                value: "  ".to_owned(),
                child: Box::new(create_cst(CstKind::Int {
                    offset: 2,
                    value: 123,
                    source: "123".to_owned()
                }))
            })
        )
    );
    // assert_eq!(
    //     parse("    123", 1),
    //     (
    //         "  ",
    //         CstKind::LeadingWhitespace {
    //             value: "".to_owned(),
    //             child: Box::new(CstKind::Int { value: 123 })
    //         }
    //     )
    // );
}
#[test]
fn test_expressions0() {
    fn parse(source: &str) -> (&str, Vec<Cst>) {
        expressions0(source, source, 0).unwrap()
    }
    assert_eq!(parse(""), ("", vec![]));
    assert_eq!(parse("\n"), ("\n", vec![]));
    assert_eq!(parse("\n#abc\n"), ("\n#abc\n", vec![]));
    assert_eq!(
        parse("\n123"),
        (
            "",
            vec![create_cst(CstKind::LeadingWhitespace {
                value: "\n".to_owned(),
                child: Box::new(create_cst(CstKind::Int {
                    offset: 1,
                    value: 123,
                    source: "123".to_owned()
                }))
            })]
        )
    );
    assert_eq!(
        parse("\nprint"),
        (
            "",
            vec![create_cst(CstKind::LeadingWhitespace {
                value: "\n".to_owned(),
                child: Box::new(create_cst(CstKind::Call {
                    name: Box::new(create_cst(CstKind::Identifier {
                        offset: 1,
                        value: "print".to_owned()
                    })),
                    arguments: vec![]
                }))
            })]
        )
    );
    assert_eq!(
        parse("\nfoo = bar\n"),
        (
            "\n",
            vec![create_cst(CstKind::LeadingWhitespace {
                value: "\n".to_owned(),
                child: Box::new(create_cst(CstKind::Assignment {
                    name: Box::new(create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::Identifier {
                            offset: 1,
                            value: "foo".to_owned()
                        }))
                    })),
                    parameters: vec![],
                    equals_sign: Box::new(create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::EqualsSign { offset: 5 }))
                    })),
                    body: vec![create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 7,
                            value: "bar".to_owned()
                        })),
                        arguments: vec![]
                    })]
                }))
            })]
        )
    );
    assert_eq!(
        parse("\nfoo\nbar"),
        (
            "",
            vec![
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 1,
                            value: "foo".to_owned()
                        })),
                        arguments: vec![],
                    }))
                }),
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 5,
                            value: "bar".to_owned()
                        })),
                        arguments: vec![],
                    }))
                }),
            ]
        )
    );
    assert_eq!(
        parse("\nadd 1 2"),
        (
            "",
            vec![create_cst(CstKind::LeadingWhitespace {
                value: "\n".to_owned(),
                child: Box::new(create_cst(CstKind::Call {
                    name: Box::new(create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::Identifier {
                            offset: 1,
                            value: "add".to_owned()
                        })),
                    })),
                    arguments: vec![
                        create_cst(CstKind::TrailingWhitespace {
                            value: " ".to_owned(),
                            child: Box::new(create_cst(CstKind::Int {
                                offset: 5,
                                value: 1,
                                source: "1".to_owned()
                            }))
                        }),
                        create_cst(CstKind::Int {
                            offset: 7,
                            value: 2,
                            source: "2".to_owned()
                        })
                    ],
                }))
            })]
        )
    );
    assert_eq!(
        parse("\nfoo = bar\nadd\n  1\n  2"),
        (
            "",
            vec![
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Assignment {
                        name: Box::new(create_cst(CstKind::TrailingWhitespace {
                            value: " ".to_owned(),
                            child: Box::new(create_cst(CstKind::Identifier {
                                offset: 1,
                                value: "foo".to_owned()
                            })),
                        })),
                        parameters: vec![],
                        equals_sign: Box::new(create_cst(CstKind::TrailingWhitespace {
                            value: " ".to_owned(),
                            child: Box::new(create_cst(CstKind::EqualsSign { offset: 5 }))
                        })),
                        body: vec![create_cst(CstKind::Call {
                            name: Box::new(create_cst(CstKind::Identifier {
                                offset: 7,
                                value: "bar".to_owned()
                            })),
                            arguments: vec![]
                        })]
                    }))
                }),
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 11,
                            value: "add".to_owned()
                        })),
                        arguments: vec![
                            create_cst(CstKind::LeadingWhitespace {
                                value: "\n".to_owned(),
                                child: Box::new(create_cst(CstKind::LeadingWhitespace {
                                    value: "  ".to_owned(),
                                    child: Box::new(create_cst(CstKind::Int {
                                        offset: 17,
                                        value: 1,
                                        source: "1".to_owned()
                                    }))
                                }))
                            }),
                            create_cst(CstKind::LeadingWhitespace {
                                value: "\n".to_owned(),
                                child: Box::new(create_cst(CstKind::LeadingWhitespace {
                                    value: "  ".to_owned(),
                                    child: Box::new(create_cst(CstKind::Int {
                                        offset: 21,
                                        value: 2,
                                        source: "2".to_owned()
                                    }))
                                }))
                            }),
                        ],
                    }))
                })
            ]
        )
    );
    assert_eq!(
        parse("\nadd\n  2\nmyIterable"),
        (
            "",
            vec![
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 1,
                            value: "add".to_owned()
                        })),
                        arguments: vec![create_cst(CstKind::LeadingWhitespace {
                            value: "\n".to_owned(),
                            child: Box::new(create_cst(CstKind::LeadingWhitespace {
                                value: "  ".to_owned(),
                                child: Box::new(create_cst(CstKind::Int {
                                    offset: 7,
                                    value: 2,
                                    source: "2".to_owned()
                                }))
                            }))
                        })],
                    }))
                }),
                create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::Identifier {
                            offset: 9,
                            value: "myIterable".to_owned()
                        })),
                        arguments: vec![],
                    }))
                })
            ]
        )
    );
}
#[test]
fn test_call() {
    fn parse(source: &str) -> (&str, Cst) {
        call(source, source, 0).unwrap()
    }
    assert_eq!(
        parse("print"),
        (
            "",
            create_cst(CstKind::Call {
                name: Box::new(create_cst(CstKind::Identifier {
                    offset: 0,
                    value: "print".to_owned()
                })),
                arguments: vec![]
            })
        )
    );
    assert_eq!(
        parse("print 123 \"foo\" Bar"),
        (
            "",
            create_cst(CstKind::Call {
                name: Box::new(create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::Identifier {
                        offset: 0,
                        value: "print".to_owned()
                    }))
                })),
                arguments: vec![
                    create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::Int {
                            offset: 6,
                            value: 123,
                            source: "123".to_owned()
                        }))
                    }),
                    create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::Text {
                            offset: 10,
                            value: "foo".to_owned()
                        }))
                    }),
                    create_cst(CstKind::Symbol {
                        offset: 16,
                        value: "Bar".to_owned()
                    })
                ]
            })
        )
    );
    assert_eq!(
        parse("add\n  7\nmyIterable"),
        (
            "\nmyIterable",
            create_cst(CstKind::Call {
                name: Box::new(create_cst(CstKind::Identifier {
                    offset: 0,
                    value: "add".to_owned()
                })),
                arguments: vec![create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::LeadingWhitespace {
                        value: "  ".to_owned(),
                        child: Box::new(create_cst(CstKind::Int {
                            offset: 6,
                            value: 7,
                            source: "7".to_owned()
                        }))
                    }))
                })]
            })
        )
    );
}
#[test]
fn test_lambda() {
    fn parse(source: &str) -> (&str, Cst) {
        lambda(source, source, 0).unwrap()
    }

    assert_eq!(
        parse("{ 123 }"),
        (
            "",
            create_cst(CstKind::Lambda {
                opening_curly_brace: Box::new(create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::OpeningCurlyBrace { offset: 0 }))
                })),
                parameters_and_arrow: None,
                body: vec![create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::Int {
                        offset: 2,
                        value: 123,
                        source: "123".to_owned()
                    }))
                })],
                closing_curly_brace: Box::new(create_cst(CstKind::ClosingCurlyBrace { offset: 6 }))
            }),
        )
    );
    assert_eq!(
        parse("{ n -> 5 }"),
        (
            "",
            create_cst(CstKind::Lambda {
                opening_curly_brace: Box::new(create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::OpeningCurlyBrace { offset: 0 }))
                })),
                parameters_and_arrow: Some((
                    vec![create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::TrailingWhitespace {
                            value: " ".to_owned(),
                            child: Box::new(create_cst(CstKind::Identifier {
                                offset: 2,
                                value: "n".to_owned()
                            }))
                        })),
                        arguments: vec![]
                    })],
                    Box::new(create_cst(CstKind::TrailingWhitespace {
                        value: " ".to_owned(),
                        child: Box::new(create_cst(CstKind::Arrow { offset: 4 }))
                    }))
                )),
                body: vec![create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::Int {
                        offset: 7,
                        value: 5,
                        source: "5".to_owned()
                    }))
                })],
                closing_curly_brace: Box::new(create_cst(CstKind::ClosingCurlyBrace { offset: 9 }))
            }),
        )
    );
    assert_eq!(
        parse("{ a ->\n  123\n}"),
        (
            "",
            create_cst(CstKind::Lambda {
                opening_curly_brace: Box::new(create_cst(CstKind::TrailingWhitespace {
                    value: " ".to_owned(),
                    child: Box::new(create_cst(CstKind::OpeningCurlyBrace { offset: 0 })),
                })),
                parameters_and_arrow: Some((
                    vec![create_cst(CstKind::Call {
                        name: Box::new(create_cst(CstKind::TrailingWhitespace {
                            value: " ".to_owned(),
                            child: Box::new(create_cst(CstKind::Identifier {
                                offset: 2,
                                value: "a".to_owned()
                            }))
                        })),
                        arguments: vec![]
                    })],
                    Box::new(create_cst(CstKind::Arrow { offset: 4 }))
                )),
                body: vec![create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::LeadingWhitespace {
                        value: "  ".to_owned(),
                        child: Box::new(create_cst(CstKind::Int {
                            offset: 9,
                            value: 123,
                            source: "123".to_owned()
                        }))
                    }))
                })],
                closing_curly_brace: Box::new(create_cst(CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(create_cst(CstKind::ClosingCurlyBrace { offset: 13 }))
                }))
            }),
        )
    );
}
#[test]
fn test_leading_stuff() {
    let source = "123";
    assert_eq!(
        leading_whitespace(source, |input| int(source, input)).unwrap(),
        (
            "",
            create_cst(CstKind::Int {
                offset: 0,
                value: 123,
                source: "123".to_owned()
            })
        )
    );

    let source = " 123";
    assert_eq!(
        leading_whitespace(source, |input| int(source, input)).unwrap(),
        (
            "",
            create_cst(CstKind::LeadingWhitespace {
                value: " ".to_owned(),
                child: Box::new(create_cst(CstKind::Int {
                    offset: 1,
                    value: 123,
                    source: "123".to_owned()
                }))
            }),
        )
    );

    fn parse(source: &str) -> (&str, Cst) {
        leading_whitespace_and_comment_and_empty_lines(
            source,
            source,
            0,
            1,
            |source, input, _indentation| int(source, input),
        )
        .unwrap()
    }

    assert_eq!(
        parse("\n123"),
        (
            "",
            create_cst(CstKind::LeadingWhitespace {
                value: "\n".to_owned(),
                child: Box::new(create_cst(CstKind::Int {
                    offset: 1,
                    value: 123,
                    source: "123".to_owned()
                }),)
            }),
        )
    );
}
// #[test]
// fn test_trailing_whitespace_and_comment() {
//     assert_eq!(trailing_whitespace_and_comment("").unwrap(), ("", ()));
//     assert_eq!(trailing_whitespace_and_comment(" \t").unwrap(), ("", ()));
//     assert_eq!(
//         trailing_whitespace_and_comment(" print").unwrap(),
//         ("print", ())
//     );
//     assert_eq!(trailing_whitespace_and_comment("# abc").unwrap(), ("", ()));
//     assert_eq!(
//         trailing_whitespace_and_comment(" \t# abc").unwrap(),
//         ("", ())
//     );
// }
// #[test]
// fn test_comment() {
//     assert_eq!(comment("#").unwrap(), ("", ()));
//     assert_eq!(comment("# abc").unwrap(), ("", ()));
// }

// trait ParserCandyExt<'a, O> {
//     fn map<R, F, G>(&mut self, f: F) -> Box<dyn FnMut(&'a str) -> ParserResult<'a, R>>
//     where
//         F: FnMut(O) -> R;
// }
// impl<'a, P, O> ParserCandyExt<'a, O> for P
// where
//     P: Parser<&'a str, O, ErrorTree<&'a str>>,
// {
//     fn map<R, F, G>(&mut self, f: F) -> impl FnMut(&'a str) -> ParserResult<'a, R>
//     where
//         F: FnMut(O) -> R,
//     {
//         todo!()
//     }
// }
