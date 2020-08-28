import 'package:meta/meta.dart';
import 'package:petitparser/petitparser.dart'
    hide ChoiceParser, ChoiceParserExtension;

import '../lexer/lexer.dart';
import '../syntactic_entity.dart';
import '../utils.dart';
import 'ast/expressions/expression.dart';
import 'ast/statements.dart';
import 'ast/types.dart';

// ignore: avoid_classes_with_only_static_members
@immutable
class ParserGrammar {
  static void init() {
    assert(!_isInitialized, 'Already initialized.');
    _isInitialized = true;

    _initType();
    _initExpression();
  }

  static bool _isInitialized = false;

  // SECTION: types

  static final _type = undefined<Type>();
  static Parser<Type> get type => _type;
  static void _initType() {
    _type.set(
      // ignore: unnecessary_cast
      (userType.map($Type.user) as Parser<Type>) | tupleType.map($Type.tuple),
    );
  }

  static final userType =
      simpleUserType.separatedList(LexerGrammar.DOT).map((values) => UserType(
            simpleTypes: values.first as List<SimpleUserType>,
            dots: values[1] as List<OperatorToken>,
          ));
  static final Parser<SimpleUserType> simpleUserType =
      LexerGrammar.Identifier.map($SimpleUserType);

  static final tupleType = (LexerGrammar.LPAREN &
          LexerGrammar.NLs &
          type.fullCommaSeparatedList(2) &
          LexerGrammar.NLs &
          LexerGrammar.RPAREN)
      .map((value) => TupleType(
            leftParenthesis: value[0] as OperatorToken,
            types: value[2][0] as List<Type>,
            commata: value[2][1] as List<OperatorToken>,
            rightParenthesis: value[4] as OperatorToken,
          ));

  // SECTION: statements

  static final Parser<List<Statement>> statements =
      ((statement & (semis & statement).map((v) => v[1] as Statement).star())
                  .map((v) => [v[0] as Statement, ...v[1] as List<Statement>])
                  .optional() &
              semis.optional())
          .map((values) => values[0] as List<Statement> ?? []);
  static final Parser<Statement> statement =
      expression.map($Statement.expression);

  static final block = (LexerGrammar.LCURL &
          LexerGrammar.NLs &
          statements &
          LexerGrammar.NLs &
          LexerGrammar.RCURL)
      .map((values) => Block(
            leftBrace: values.first as OperatorToken,
            statements: values[2] as List<Statement>,
            rightBrace: values[4] as OperatorToken,
          ));

  // ignore: unnecessary_cast
  static final Parser<void> semi = (LexerGrammar.WS.optional() &
          (LexerGrammar.SEMICOLON | LexerGrammar.NL) &
          LexerGrammar.NLs as Parser<void>) |
      endOfInput();
  static final Parser<void> semis =
      // ignore: unnecessary_cast
      ((LexerGrammar.WS.optional() &
                  (LexerGrammar.SEMICOLON | LexerGrammar.NL) &
                  LexerGrammar.WS.optional())
              .plus() as Parser<void>) |
          endOfInput();

  // SECTION: expressions

  static final _expression = undefined<Expression>();
  static Parser<Expression> get expression => _expression;
  static void _initExpression() {
    final builder = ExpressionBuilder()
      ..primitive(
          // ignore: unnecessary_cast, Without the cast the compiler complains…
          (literalConstant as Parser<Expression>) |
              LexerGrammar.Identifier.map((t) => Identifier(t)))
      // grouping
      ..wrapper(LexerGrammar.LPAREN, LexerGrammar.RPAREN)
      // unary postfix
      ..postfix(LexerGrammar.PLUS_PLUS |
          LexerGrammar.MINUS_MINUS |
          LexerGrammar.QUESTION |
          LexerGrammar.EXCLAMATION)
      ..complexPostfix<List<SyntacticEntity>, NavigationExpression>(
        navigationPostfix,
        mapper: (expression, postfix) {
          return NavigationExpression(
            target: expression,
            dot: postfix.first as OperatorToken,
            name: postfix[1] as IdentifierToken,
          );
        },
      )
      ..complexPostfix<List<SyntacticEntity>, CallExpression>(
        invocationPostfix,
        mapper: (expression, postfix) {
          return CallExpression(
            target: expression,
            leftParenthesis: postfix.first as OperatorToken,
            arguments: postfix.sublist(1, postfix.length - 1).cast<Argument>(),
            rightParenthesis: postfix.last as OperatorToken,
          );
        },
      )
      ..complexPostfix<List<SyntacticEntity>, IndexExpression>(
        indexingPostfix,
        mapper: (expression, postfix) {
          return IndexExpression(
            target: expression,
            leftSquareBracket: postfix.first as OperatorToken,
            indices: postfix.sublist(1, postfix.length - 2) as List<Expression>,
            rightSquareBracket: postfix.last as OperatorToken,
          );
        },
      )
      // unary prefix
      ..prefix(LexerGrammar.EXCLAMATION |
          LexerGrammar.TILDE |
          LexerGrammar.PLUS_PLUS |
          LexerGrammar.MINUS_MINUS |
          LexerGrammar.MINUS)
      // implicit multiplication
      // TODO(JonasWanke): add implicit multiplication
      // multiplicative
      ..left(LexerGrammar.ASTERISK |
          LexerGrammar.SLASH |
          LexerGrammar.TILDE_SLASH |
          LexerGrammar.PERCENT)
      // additive
      ..left(LexerGrammar.PLUS | LexerGrammar.MINUS)
      // shift
      ..left(LexerGrammar.LESS_LESS |
          LexerGrammar.GREATER_GREATER |
          LexerGrammar.GREATER_GREATER_GREATER)
      // bitwise and
      ..left(LexerGrammar.AMPERSAND)
      // bitwise or
      ..left(LexerGrammar.CARET)
      // bitwise not
      ..left(LexerGrammar.BAR)
      // type check
      ..left(LexerGrammar.AS | LexerGrammar.AS_SAFE)
      // range
      ..left(LexerGrammar.DOT_DOT | LexerGrammar.DOT_DOT_EQUALS)
      // infix function
      // TODO(JonasWanke): infix function
      // named checks
      ..left(LexerGrammar.IN |
          LexerGrammar.EXCLAMATION_IN |
          LexerGrammar.IS |
          LexerGrammar.EXCLAMATION_IS)
      // comparison
      ..left(LexerGrammar.LESS |
          LexerGrammar.LESS_EQUAL |
          LexerGrammar.GREATER |
          LexerGrammar.GREATER_EQUAL)
      // equality
      ..left(LexerGrammar.EQUALS_EQUALS |
          LexerGrammar.EXCLAMATION_EQUALS_EQUALS |
          LexerGrammar.EQUALS_EQUALS_EQUALS |
          LexerGrammar.EXCLAMATION_EQUALS_EQUALS)
      // logical and
      ..left(LexerGrammar.AMPERSAND_AMPERSAND)
      // logical or
      ..left(LexerGrammar.BAR_BAR)
      // logical implication
      ..left(LexerGrammar.DASH_GREATER | LexerGrammar.LESS_DASH)
      // spread
      ..prefix(LexerGrammar.DOT_DOT_DOT)
      // assignment
      ..right(LexerGrammar.EQUALS |
          LexerGrammar.ASTERISK_EQUALS |
          LexerGrammar.SLASH_EQUALS |
          LexerGrammar.TILDE_SLASH_EQUALS |
          LexerGrammar.PERCENT_EQUALS |
          LexerGrammar.PLUS_EQUALS |
          LexerGrammar.MINUS_EQUALS |
          LexerGrammar.AMPERSAND_EQUALS |
          LexerGrammar.BAR_EQUALS |
          LexerGrammar.CARET_EQUALS |
          LexerGrammar.AMPERSAND_AMPERSAND_EQUALS |
          LexerGrammar.BAR_BAR_EQUALS |
          LexerGrammar.LESS_LESS_EQUALS |
          LexerGrammar.GREATER_GREATER_EQUALS |
          LexerGrammar.GREATER_GREATER_GREATER_EQUALS);

    _expression.set(builder.build().map((dynamic e) => e as Expression));
  }

  static final navigationPostfix = (LexerGrammar.NLs &
          LexerGrammar.DOT &
          LexerGrammar.NLs &
          LexerGrammar.Identifier)
      .map<List<SyntacticEntity>>((value) {
    return [
      value[1] as OperatorToken, // dot
      value[3] as IdentifierToken, // name
    ];
  });

  // TODO(JonasWanke): typeArguments? valueArguments? annotatedLambda | typeArguments? valueArguments
  static final invocationPostfix = (LexerGrammar.LPAREN &
          valueArguments &
          LexerGrammar.NLs &
          LexerGrammar.RPAREN)
      .map<List<SyntacticEntity>>((value) {
    return [
      value[0] as OperatorToken, // leftParenthesis
      ...value[1] as List<Argument>, // arguments
      value[3] as OperatorToken, // rightParenthesis
    ];
  });

  static final valueArguments = (LexerGrammar.NLs &
          valueArgument.commaSeparatedList().optional() &
          LexerGrammar.NLs)
      .map((value) => value[1] as List<Argument> ?? []);

  static final valueArgument = (LexerGrammar.NLs &
          (LexerGrammar.Identifier &
                  LexerGrammar.NLs &
                  LexerGrammar.EQUALS &
                  LexerGrammar.NLs)
              .optional() &
          LexerGrammar.NLs &
          expression)
      .map<Argument>((value) {
    return Argument(
      name: (value[1] as List<dynamic>)?.first as IdentifierToken,
      equals: (value[1] as List<dynamic>)?.elementAt(2) as OperatorToken,
      expression: value[3] as Expression,
    );
  });

  static final indexingPostfix = (LexerGrammar.LSQUARE &
          LexerGrammar.NLs &
          expression.commaSeparatedList() &
          LexerGrammar.NLs &
          LexerGrammar.RSQUARE)
      .map<List<SyntacticEntity>>((value) {
    return [
      value[0] as OperatorToken, // leftSquareBracket
      ...value[2] as List<Expression>, // indices
      value[4] as OperatorToken, // rightSquareBracket
    ];
  });

  static final literalConstant = ChoiceParser<Literal<dynamic>>([
    LexerGrammar.IntegerLiteral.map((l) => Literal<int>(l)),
    LexerGrammar.BooleanLiteral.map((l) => Literal<bool>(l)),
  ]);
}

extension<T> on Parser<T> {
  Parser<List<dynamic>> separatedList(Parser<OperatorToken> separator) {
    return (this &
            (LexerGrammar.NLs & separator & LexerGrammar.NLs & this)
                .map<dynamic>((v) => [v[1] as OperatorToken, v[3] as T])
                .star())
        .map((value) {
      final trailing = (value[1] as List<dynamic>).cast<List<dynamic>>();
      return <dynamic>[
        [value.first as T, ...trailing.map((dynamic v) => v[1] as T)],
        [...trailing.map((dynamic v) => v[0] as OperatorToken)],
      ];
    });
  }

  Parser<List<dynamic>> fullCommaSeparatedList([int minimum = 1]) {
    assert(minimum != null);
    assert(minimum >= 1);

    return (this &
            (LexerGrammar.NLs & LexerGrammar.COMMA & LexerGrammar.NLs & this)
                .map<dynamic>((v) => [v[1] as OperatorToken, v[3] as T])
                .repeat(minimum - 1, unbounded) &
            (LexerGrammar.NLs & LexerGrammar.COMMA).optional())
        .map((value) {
      final trailing = (value[1] as List<dynamic>).cast<List<dynamic>>();
      final trailingComma =
          (value[2] as List<dynamic>)?.elementAt(1) as OperatorToken;
      return <dynamic>[
        [value.first as T, ...trailing.map((dynamic v) => v[1] as T)],
        [
          ...trailing.map((dynamic v) => v[0] as OperatorToken),
          if (trailingComma != null) trailingComma,
        ],
      ];
    });
  }

  Parser<List<T>> commaSeparatedList() {
    return (this &
            (LexerGrammar.NLs & LexerGrammar.COMMA & LexerGrammar.NLs & this)
                .map<T>((v) => v[3] as T)
                .star() &
            (LexerGrammar.NLs & LexerGrammar.COMMA).optional())
        .map((value) {
      return [value.first as T, ...value[1] as List<T>];
    });
  }
}

extension on ExpressionBuilder {
  void primitive(Parser<Expression> primitive) =>
      group().primitive<Expression>(primitive);

  void wrapper(Parser<OperatorToken> left, Parser<OperatorToken> right) {
    group().wrapper<OperatorToken, Expression>(
      left,
      right,
      (left, expression, right) {
        return GroupExpression(
          leftParenthesis: left,
          expression: expression,
          rightParenthesis: right,
        );
      },
    );
  }

  void postfix(Parser<OperatorToken> operator) {
    group().postfix<OperatorToken, Expression>(
      operator,
      (operand, operator) =>
          PostfixExpression(operand: operand, operatorToken: operator),
    );
  }

  void complexPostfix<T, R>(
    Parser<T> postfix, {
    @required R Function(Expression expression, T postfix) mapper,
  }) =>
      group().postfix<T, Expression>(postfix, mapper);

  void prefix(Parser<OperatorToken> operator) {
    group().prefix<OperatorToken, Expression>(
      operator,
      (operator, operand) =>
          PrefixExpression(operatorToken: operator, operand: operand),
    );
  }

  void left(Parser<OperatorToken> operator) {
    group().left<OperatorToken, Expression>(
      operator,
      (left, operator, right) => BinaryExpression(left, operator, right),
    );
  }

  void right(Parser<OperatorToken> operator) {
    group().right<OperatorToken, Expression>(
      operator,
      (left, operator, right) => BinaryExpression(left, operator, right),
    );
  }
}
