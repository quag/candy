use std::fmt::{self, Display, Formatter};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Rcst {
    EqualsSign,         // =
    Comma,              // ,
    Colon,              // :
    OpeningParenthesis, // (
    ClosingParenthesis, // )
    OpeningBracket,     // [
    ClosingBracket,     // ]
    OpeningCurlyBrace,  // {
    ClosingCurlyBrace,  // }
    Arrow,              // ->
    DoubleQuote,        // "
    Octothorpe,         // #
    Whitespace(String), // contains only non-multiline whitespace
    Newline(String), // the associated `String` because some systems (such as Windows) have weird newlines
    Comment {
        octothorpe: Box<Rcst>,
        comment: String,
    },
    TrailingWhitespace {
        child: Box<Rcst>,
        whitespace: Vec<Rcst>,
    },
    Identifier(String),
    Symbol(String),
    Int(u64),
    Text {
        opening_quote: Box<Rcst>,
        parts: Vec<Rcst>,
        closing_quote: Box<Rcst>,
    },
    TextPart(String),
    Parenthesized {
        opening_parenthesis: Box<Rcst>,
        inner: Box<Rcst>,
        closing_parenthesis: Box<Rcst>,
    },
    Call {
        name: Box<Rcst>,
        arguments: Vec<Rcst>,
    },
    Struct {
        opening_bracket: Box<Rcst>,
        fields: Vec<Rcst>,
        closing_bracket: Box<Rcst>,
    },
    StructField {
        key: Box<Rcst>,
        colon: Box<Rcst>,
        value: Box<Rcst>,
        comma: Option<Box<Rcst>>,
    },
    Lambda {
        opening_curly_brace: Box<Rcst>,
        parameters_and_arrow: Option<(Vec<Rcst>, Box<Rcst>)>,
        body: Vec<Rcst>,
        closing_curly_brace: Box<Rcst>,
    },
    Assignment {
        name: Box<Rcst>,
        parameters: Vec<Rcst>,
        equals_sign: Box<Rcst>,
        body: Vec<Rcst>,
    },
    Error {
        unparsable_input: String,
        error: RcstError,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum RcstError {
    IdentifierContainsNonAlphanumericAscii,
    SymbolContainsNonAlphanumericAscii,
    IntContainsNonDigits,
    TextDoesNotEndUntilInputEnds,
    TextNotSufficientlyIndented,
    StructFieldMissesKey,
    StructFieldMissesColon,
    StructFieldMissesValue,
    StructNotClosed,
    WeirdWhitespace,
    WeirdWhitespaceInIndentation,
    ExpressionExpectedAfterOpeningParenthesis,
    ParenthesisNotClosed,
    TooMuchWhitespace,
    CurlyBraceNotClosed,
    UnparsedRest,
    UnexpectedPunctuation,
}

impl Display for Rcst {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Rcst::EqualsSign => "=".fmt(f),
            Rcst::Comma => ",".fmt(f),
            Rcst::Colon => ":".fmt(f),
            Rcst::OpeningParenthesis => "(".fmt(f),
            Rcst::ClosingParenthesis => ")".fmt(f),
            Rcst::OpeningBracket => "[".fmt(f),
            Rcst::ClosingBracket => "]".fmt(f),
            Rcst::OpeningCurlyBrace => "{".fmt(f),
            Rcst::ClosingCurlyBrace => "}".fmt(f),
            Rcst::Arrow => "->".fmt(f),
            Rcst::DoubleQuote => '"'.fmt(f),
            Rcst::Octothorpe => "#".fmt(f),
            Rcst::Whitespace(whitespace) => whitespace.fmt(f),
            Rcst::Newline(newline) => newline.fmt(f),
            Rcst::Comment {
                octothorpe,
                comment,
            } => {
                octothorpe.fmt(f)?;
                comment.fmt(f)
            }
            Rcst::TrailingWhitespace { child, whitespace } => {
                child.fmt(f)?;
                for w in whitespace {
                    w.fmt(f)?;
                }
                Ok(())
            }
            Rcst::Identifier(identifier) => identifier.fmt(f),
            Rcst::Symbol(symbol) => symbol.fmt(f),
            Rcst::Int(int) => int.fmt(f),
            Rcst::Text {
                opening_quote,
                parts,
                closing_quote,
            } => {
                opening_quote.fmt(f)?;
                for part in parts {
                    part.fmt(f)?;
                }
                closing_quote.fmt(f)
            }
            Rcst::TextPart(literal) => literal.fmt(f),
            Rcst::Parenthesized {
                opening_parenthesis,
                inner,
                closing_parenthesis,
            } => {
                opening_parenthesis.fmt(f)?;
                inner.fmt(f)?;
                closing_parenthesis.fmt(f)
            }
            Rcst::Call { name, arguments } => {
                name.fmt(f)?;
                for argument in arguments {
                    argument.fmt(f)?;
                }
                Ok(())
            }
            Rcst::Struct {
                opening_bracket,
                fields,
                closing_bracket,
            } => {
                opening_bracket.fmt(f)?;
                for field in fields {
                    field.fmt(f)?;
                }
                closing_bracket.fmt(f)
            }
            Rcst::StructField {
                key,
                colon,
                value,
                comma,
            } => {
                key.fmt(f)?;
                colon.fmt(f)?;
                value.fmt(f)?;
                if let Some(comma) = comma {
                    comma.fmt(f)?;
                }
                Ok(())
            }
            Rcst::Lambda {
                opening_curly_brace,
                parameters_and_arrow,
                body,
                closing_curly_brace,
            } => {
                opening_curly_brace.fmt(f)?;
                if let Some((parameters, arrow)) = parameters_and_arrow {
                    for parameter in parameters {
                        parameter.fmt(f)?;
                    }
                    arrow.fmt(f)?;
                }
                for expression in body {
                    expression.fmt(f)?;
                }
                closing_curly_brace.fmt(f)
            }
            Rcst::Assignment {
                name,
                parameters,
                equals_sign,
                body,
            } => {
                name.fmt(f)?;
                for parameter in parameters {
                    parameter.fmt(f)?;
                }
                equals_sign.fmt(f)?;
                for expression in body {
                    expression.fmt(f)?;
                }
                Ok(())
            }
            Rcst::Error {
                unparsable_input, ..
            } => unparsable_input.fmt(f),
        }
    }
}

pub trait IsMultiline {
    fn is_multiline(&self) -> bool;
}

impl IsMultiline for Rcst {
    fn is_multiline(&self) -> bool {
        log::info!("Is {:?} multiline?", self);
        match self {
            Rcst::EqualsSign => false,
            Rcst::Comma => false,
            Rcst::Colon => false,
            Rcst::OpeningParenthesis => false,
            Rcst::ClosingParenthesis => false,
            Rcst::OpeningBracket => false,
            Rcst::ClosingBracket => false,
            Rcst::OpeningCurlyBrace => false,
            Rcst::ClosingCurlyBrace => false,
            Rcst::Arrow => false,
            Rcst::DoubleQuote => false,
            Rcst::Octothorpe => false,
            Rcst::Whitespace(whitespace) => false,
            Rcst::Newline(_) => true,
            Rcst::Comment { .. } => false,
            Rcst::TrailingWhitespace { child, whitespace } => {
                log::info!("Is child multiline?");
                let c = child.is_multiline();
                log::info!("Is whitespace multiline?");
                let w = whitespace.is_multiline();
                log::info!("Combining");
                c || w
            }
            Rcst::Identifier(_) => false,
            Rcst::Symbol(_) => false,
            Rcst::Int(_) => false,
            Rcst::Text {
                opening_quote,
                parts,
                closing_quote,
            } => {
                opening_quote.is_multiline() || parts.is_multiline() || closing_quote.is_multiline()
            }
            Rcst::TextPart(_) => false,
            Rcst::Parenthesized {
                opening_parenthesis,
                inner,
                closing_parenthesis,
            } => {
                opening_parenthesis.is_multiline()
                    || inner.is_multiline()
                    || closing_parenthesis.is_multiline()
            }
            Rcst::Call { name, arguments } => name.is_multiline() || arguments.is_multiline(),
            Rcst::Struct {
                opening_bracket,
                fields,
                closing_bracket,
            } => {
                opening_bracket.is_multiline()
                    || fields.iter().any(|field| field.is_multiline())
                    || closing_bracket.is_multiline()
            }
            Rcst::StructField {
                key,
                colon,
                value,
                comma,
            } => {
                key.is_multiline()
                    || colon.is_multiline()
                    || value.is_multiline()
                    || comma
                        .as_ref()
                        .map(|comma| comma.is_multiline())
                        .unwrap_or(false)
            }
            Rcst::Lambda {
                opening_curly_brace,
                parameters_and_arrow,
                body,
                closing_curly_brace,
            } => {
                opening_curly_brace.is_multiline()
                    || parameters_and_arrow
                        .as_ref()
                        .map(|(parameters, arrow)| {
                            parameters.is_multiline() || arrow.is_multiline()
                        })
                        .unwrap_or(false)
                    || body.is_multiline()
                    || closing_curly_brace.is_multiline()
            }
            Rcst::Assignment {
                name,
                parameters,
                equals_sign,
                body,
            } => {
                name.is_multiline()
                    || parameters.is_multiline()
                    || equals_sign.is_multiline()
                    || body.is_multiline()
            }
            Rcst::Error {
                unparsable_input, ..
            } => unparsable_input.is_multiline(),
        }
    }
}

impl IsMultiline for str {
    fn is_multiline(&self) -> bool {
        self.contains('\n')
    }
}

impl IsMultiline for Vec<Rcst> {
    fn is_multiline(&self) -> bool {
        self.iter().any(|cst| cst.is_multiline())
    }
}

impl<T: IsMultiline> IsMultiline for Option<T> {
    fn is_multiline(&self) -> bool {
        match self {
            Some(it) => it.is_multiline(),
            None => false,
        }
    }
}

impl<A: IsMultiline, B: IsMultiline> IsMultiline for (A, B) {
    fn is_multiline(&self) -> bool {
        self.0.is_multiline() || self.1.is_multiline()
    }
}
