import 'package:compiler/compiler.dart';
import 'package:dartx/dartx.dart';
import 'package:parser/parser.dart' as ast;

import '../../errors.dart';
import '../../query.dart';
import '../../utils.dart';
import '../ast.dart';
import '../hir.dart' as hir;
import '../hir/ids.dart';
import 'declarations/declarations.dart';
import 'declarations/function.dart';
import 'declarations/module.dart';
import 'declarations/property.dart';
import 'type.dart';

final getBody = Query<DeclarationId, Option<List<hir.Expression>>>(
  'getBody',
  provider: (context, declarationId) =>
      lowerBodyAstToHir(context, declarationId).mapValue((v) => v.first),
);
final getBodyAstToHirIds = Query<DeclarationId, Option<BodyAstToHirIds>>(
  'getBodyAstToHirIds',
  provider: (context, declarationId) =>
      lowerBodyAstToHir(context, declarationId).mapValue((v) => v.second),
);
final Query<DeclarationId,
        Option<Tuple2<List<hir.Expression>, BodyAstToHirIds>>>
    lowerBodyAstToHir =
    Query<DeclarationId, Option<Tuple2<List<hir.Expression>, BodyAstToHirIds>>>(
  'lowerBodyAstToHir',
  provider: (context, declarationId) {
    if (declarationId.isFunction) {
      final functionAst = getFunctionDeclarationAst(context, declarationId);
      if (functionAst.body == null) return None();

      final result = FunctionContext.lowerFunction(context, declarationId);
      // ignore: only_throw_errors, Iterables of errors are also handled.
      if (result is Error) throw result.error;

      return Some(result.value);
      // } else if (declarationId.isProperty) {
      //   final propertyAst = getPropertyDeclarationAst(context, declarationId);
      //   if (propertyAst.initializer == null) return None();

      //   var type = Option<hir.CandyType>.none();
      //   if (propertyAst.type != null) {
      //     final moduleId = declarationIdToModuleId(context, declarationId);
      //     type = Option.some(
      //       astTypeToHirType(context, Tuple2(moduleId, propertyAst.type)),
      //     );
      //   }
      //   final localContext =
      //       _LocalContext.forProperty(context, declarationId, type);

      //   final result = localContext.lowerToUnambiguous(propertyAst.initializer);
      //   // ignore: only_throw_errors, Iterables of errors are also handled.
      //   if (result is Error) throw result.error;

      //   final statement = hir.Statement.expression(
      //     localContext.getId(propertyAst.initializer),
      //     result.value,
      //   );
      //   return Some(Tuple2([statement], localContext.idMap));
    } else {
      throw CompilerError.unsupportedFeature(
        'Unsupported body.',
        location: ErrorLocation(
          declarationId.resourceId,
          getDeclarationAst(context, declarationId).span,
        ),
      );
    }
  },
);

abstract class Context {
  QueryContext get queryContext;
  DeclarationId get declarationId;
  ResourceId get resourceId => declarationId.resourceId;

  Option<Context> get parent;

  Option<hir.CandyType> get expressionType;
  bool isValidExpressionType(hir.CandyType type) {
    return expressionType.when(
      some: (expressionType) =>
          isAssignableTo(queryContext, Tuple2(type, expressionType)),
      none: () => true,
    );
  }

  DeclarationLocalId getId([
    dynamic /* ast.Expression | ast.ValueParameter */ expressionOrParameter,
  ]);
  BodyAstToHirIds get idMap;

  Option<hir.Identifier> resolveIdentifier(String name);
  void addIdentifier(hir.LocalPropertyIdentifier identifier);

  Option<Tuple2<DeclarationLocalId, Option<hir.CandyType>>> resolveReturn(
    Option<String> label,
  );

  Option<Tuple2<DeclarationLocalId, Option<hir.CandyType>>> resolveBreak(
    Option<String> label,
  );
  Option<DeclarationLocalId> resolveContinue(Option<String> label) =>
      resolveBreak(label).mapValue((values) => values.first);

  Result<List<hir.Expression>, List<ReportedCompilerError>> lower(
    ast.Expression expression,
  ) {
    Result<List<hir.Expression>, List<ReportedCompilerError>> result;
    if (expression is ast.Literal) {
      result = lowerLiteral(expression);
    } else if (expression is ast.StringLiteral) {
      result = lowerStringLiteral(expression);
      // }  else if (expression is ast.LambdaLiteral) {
      //   result = lowerLambdaLiteral(expression);
      // } else if (expression is ast.Identifier) {
      //   result = lowerIdentifier(expression);
      // } else if (expression is ast.CallExpression) {
      //   result = lowerCall(expression);
    } else if (expression is ast.ReturnExpression) {
      result = lowerReturn(expression);
    } else {
      throw CompilerError.unsupportedFeature(
        'Unsupported expression.',
        location: ErrorLocation(resourceId, expression.span),
      );
    }

    assert(result != null);
    assert(result is Error ||
        result.value.isNotEmpty &&
            result.value.every((r) => isValidExpressionType(r.type)));
    assert(result is Ok || result.error.isNotEmpty);
    return result;
  }

  Result<hir.Expression, List<ReportedCompilerError>> lowerUnambiguous(
    ast.Expression expression,
  ) {
    final result = lower(expression);
    if (result is Error) return Error(result.error);

    if (result.value.isEmpty) {
      assert(expressionType is Some);
      return Error([
        CompilerError.invalidExpressionType(
          'Expression could not be resolved to match type `${expressionType.value}`.',
          location: ErrorLocation(resourceId, expression.span),
        ),
      ]);
    } else if (result.value.length > 1) {
      return Error([
        CompilerError.ambiguousExpression(
          'Expression is ambiguous.',
          location: ErrorLocation(resourceId, expression.span),
        ),
      ]);
    }
    return Ok(result.value.single);
  }
}

extension<T, E> on Iterable<Result<T, List<E>>> {
  Result<List<T>, List<E>> merge() {
    final errors = whereType<Error<T, List<E>>>();
    if (errors.isNotEmpty) return Error(errors.expand((e) => e.error).toList());

    final oks = whereType<Ok<T, List<E>>>();
    return Ok(oks.map((ok) => ok.value).toList());
  }
}

extension<T, E> on Iterable<Result<List<T>, List<E>>> {
  Result<List<T>, List<E>> merge() {
    final errors = whereType<Error<List<T>, List<E>>>();
    if (errors.isNotEmpty) return Error(errors.expand((e) => e.error).toList());

    final oks = whereType<Ok<List<T>, List<E>>>();
    return Ok(oks.expand((ok) => ok.value).toList());
  }
}

abstract class InnerContext extends Context {
  InnerContext(Context parent)
      : assert(parent != null),
        parent = Some(parent);

  @override
  QueryContext get queryContext => parent.value.queryContext;
  @override
  DeclarationId get declarationId => parent.value.declarationId;

  @override
  final Option<Context> parent;

  @override
  Option<hir.CandyType> get expressionType => parent.value.expressionType;

  @override
  DeclarationLocalId getId([
    dynamic /* ast.Expression | ast.ValueParameter */ expressionOrParameter,
  ]) =>
      parent.value.getId(expressionOrParameter);
  @override
  BodyAstToHirIds get idMap => parent.value.idMap;

  @override
  Option<hir.Identifier> resolveIdentifier(String name) =>
      parent.value.resolveIdentifier(name);
  @override
  Option<Tuple2<DeclarationLocalId, Option<hir.CandyType>>> resolveReturn(
    Option<String> label,
  ) =>
      parent.value.resolveReturn(label);

  @override
  Option<Tuple2<DeclarationLocalId, Option<hir.CandyType>>> resolveBreak(
    Option<String> label,
  ) =>
      parent.value.resolveBreak(label);
}

class ContextContext extends Context {
  ContextContext(this.queryContext, this.declarationId)
      : assert(queryContext != null),
        assert(declarationId != null);

  @override
  final QueryContext queryContext;
  @override
  final DeclarationId declarationId;

  @override
  Option<Context> get parent => None();
  @override
  Option<hir.CandyType> get expressionType => None();

  var _nextId = 0;
  var _idMap = BodyAstToHirIds();
  @override
  BodyAstToHirIds get idMap => _idMap;
  @override
  DeclarationLocalId getId([
    dynamic /* ast.Expression | ast.ValueParameter */ expressionOrParameter,
  ]) {
    final existing = _idMap.map[expressionOrParameter];
    if (existing != null) return existing;

    final id = DeclarationLocalId(declarationId, _nextId++);
    if (expressionOrParameter == null) return id;

    int astId;
    if (expressionOrParameter is ast.Expression) {
      astId = expressionOrParameter.id;
    } else if (expressionOrParameter is ast.ValueParameter) {
      astId = expressionOrParameter.id;
    } else {
      throw CompilerError.internalError(
        '`ContextContext.getId()` called with an invalid `expressionOrParameter` argument: `$expressionOrParameter`.',
      );
    }
    _idMap = _idMap.withMapping(astId, id);
    return id;
  }

  @override
  Option<hir.Identifier> resolveIdentifier(String name) {
    if (name == 'this') {
      if (declarationId.isConstructor) {
        return None();
      } else if (declarationId.isFunction) {
        final function = getFunctionDeclarationHir(queryContext, declarationId);
        if (function.isStatic) return None();
      } else if (declarationId.isProperty) {
        final function = getPropertyDeclarationHir(queryContext, declarationId);
        if (function.isStatic) return None();
      } else {
        throw CompilerError.internalError(
          '`ContextContext` is not within a constructor, function or property: `$declarationId`.',
        );
      }

      if (!declarationId.hasParent) return None();
      final parent = declarationId.parent;
      if (parent.isTrait || parent.isImpl || parent.isClass) {
        return Some(hir.Identifier.this_());
      }
      return None();
    }

    // TODO(JonasWanke): resolve `field` in property accessors
    return None();
  }

  @override
  void addIdentifier(hir.LocalPropertyIdentifier identifier) {
    throw CompilerError.internalError(
      "Can't add an identifier to a `ContextContext`.",
    );
  }

  @override
  Option<Tuple2<DeclarationLocalId, Option<CandyType>>> resolveReturn(
    Option<String> label,
  ) =>
      None();
  @override
  Option<Tuple2<DeclarationLocalId, Option<CandyType>>> resolveBreak(
    Option<String> label,
  ) =>
      None();
}

class FunctionContext extends InnerContext {
  factory FunctionContext._create(QueryContext queryContext, DeclarationId id) {
    final parent = ContextContext(queryContext, id);
    final ast = getFunctionDeclarationAst(queryContext, id);
    final identifiers = {
      for (final parameter in ast.valueParameters)
        parameter.name.name: hir.Identifier.parameter(
          parent.getId(parameter),
          parameter.name.name,
          astTypeToHirType(
            parent.queryContext,
            Tuple2(
              declarationIdToModuleId(
                parent.queryContext,
                parent.declarationId,
              ),
              parameter.type,
            ),
          ),
        ),
    };

    return FunctionContext._(
      parent,
      identifiers,
      getFunctionDeclarationHir(queryContext, id).returnType,
      ast.body,
    );
  }
  FunctionContext._(
    Context parent,
    this._identifiers,
    this.returnType,
    this.body,
  )   : assert(_identifiers != null),
        assert(returnType != null),
        assert(body != null),
        super(parent);

  static Result<Tuple2<List<hir.Expression>, BodyAstToHirIds>,
      List<ReportedCompilerError>> lowerFunction(
    QueryContext queryContext,
    DeclarationId id,
  ) =>
      FunctionContext._create(queryContext, id)._lowerBody();

  final Map<String, hir.Identifier> _identifiers;
  final hir.CandyType returnType;
  final ast.LambdaLiteral body;

  @override
  void addIdentifier(hir.LocalPropertyIdentifier identifier) {
    _identifiers[identifier.name] = identifier;
  }

  @override
  Option<Identifier> resolveIdentifier(String name) {
    final result = _identifiers[name];
    if (result != null) return Some(result);
    return parent.value.resolveIdentifier(name);
  }

  @override
  Option<Tuple2<DeclarationLocalId, Option<hir.CandyType>>> resolveReturn(
    Option<String> label,
  ) {
    if (label is None ||
        label == Some(declarationId.simplePath.last.nameOrNull)) {
      return Some(Tuple2(getId(body), Some(returnType)));
    }
    return None();
  }

  Result<Tuple2<List<hir.Expression>, BodyAstToHirIds>,
      List<ReportedCompilerError>> _lowerBody() {
    final returnsUnit = returnType == hir.CandyType.unit;

    if (!returnsUnit && body.expressions.isEmpty) {
      return Error([
        CompilerError.missingReturn(
          "Function has a return type (different than `Unit`) but doesn't contain any expressions.",
          location: ErrorLocation(
            resourceId,
            getFunctionDeclarationAst(queryContext, declarationId)
                .representativeSpan,
          ),
        ),
      ]);
    }

    final results = <Result<hir.Expression, List<ReportedCompilerError>>>[];

    for (final expression in body.expressions.dropLast(returnsUnit ? 0 : 1)) {
      final lowered = innerExpressionContext(forwardsIdentifiers: true)
          .lowerUnambiguous(expression);
      results.add(lowered);
    }

    if (!returnsUnit) {
      final lowered = innerExpressionContext(expressionType: Some(returnType))
          .lowerUnambiguous(body.expressions.last);
      if (lowered is Error) {
        results.add(lowered);
      } else if (lowered.value is hir.ReturnExpression) {
        results.add(lowered);
      } else {
        results.add(Ok(
          hir.Expression.return_(getId(), getId(body), lowered.value),
        ));
      }
    }
    return results
        .merge()
        .mapValue((expressions) => Tuple2(expressions, idMap));
  }
}

class ExpressionContext extends InnerContext {
  ExpressionContext(
    Context parent, {
    this.expressionType = const None(),
    this.forwardsIdentifiers = false,
  })  : assert(expressionType != null),
        assert(forwardsIdentifiers != null),
        super(parent);

  @override
  final Option<hir.CandyType> expressionType;

  final bool forwardsIdentifiers;

  @override
  void addIdentifier(LocalPropertyIdentifier identifier) {
    if (!forwardsIdentifiers) return;

    parent.value.addIdentifier(identifier);
  }
}

extension on Context {
  ExpressionContext innerExpressionContext({
    Option<hir.CandyType> expressionType = const None(),
    bool forwardsIdentifiers = false,
  }) {
    return ExpressionContext(
      this,
      expressionType: expressionType,
      forwardsIdentifiers: forwardsIdentifiers,
    );
  }
}

extension on Context {
  Result<List<hir.Expression>, List<ReportedCompilerError>> lowerLiteral(
    ast.Literal<dynamic> expression,
  ) {
    final token = expression.value;
    hir.Literal literal;
    if (token is ast.BoolLiteralToken) {
      if (!isValidExpressionType(hir.CandyType.bool)) {
        return Error([
          CompilerError.invalidExpressionType(
            'Expected type `${expressionType.value}`, got `Bool`',
            location: ErrorLocation(resourceId, expression.span),
          ),
        ]);
      }
      literal = hir.Literal.boolean(token.value);
    } else if (token is ast.IntLiteralToken) {
      if (!isValidExpressionType(hir.CandyType.int)) {
        return Error([
          CompilerError.invalidExpressionType(
            'Expected type `${expressionType.value}`, got `Int`',
            location: ErrorLocation(resourceId, expression.span),
          ),
        ]);
      }
      literal = hir.Literal.integer(token.value);
    } else {
      throw CompilerError.unsupportedFeature(
        'Unsupported literal.',
        location: ErrorLocation(resourceId, token.span),
      );
    }
    return Ok([
      hir.Expression.literal(getId(expression), literal),
    ]);
  }

  Result<List<hir.Expression>, List<ReportedCompilerError>> lowerStringLiteral(
    ast.StringLiteral expression,
  ) {
    final parts = expression.parts
        .map<Result<List<hir.StringLiteralPart>, List<ReportedCompilerError>>>(
            (part) {
      if (part is ast.LiteralStringLiteralPart) {
        return Ok([hir.StringLiteralPart.literal(part.value.value)]);
      } else if (part is ast.InterpolatedStringLiteralPart) {
        return innerExpressionContext()
            .lowerUnambiguous(part.expression)
            .mapValue((expression) =>
                [hir.StringLiteralPart.interpolated(expression)]);
      } else {
        throw CompilerError.unsupportedFeature(
          'Unsupported String literal part.',
          location: ErrorLocation(resourceId, part.span),
        );
      }
    });
    return parts.merge().mapValue((parts) => [
          hir.Expression.literal(getId(expression), hir.StringLiteral(parts)),
        ]);
  }

  Result<List<hir.Expression>, List<ReportedCompilerError>> lowerReturn(
    ast.ReturnExpression expression,
  ) {
    // The type of a `ReturnExpression` is `Never` and that is, by definition,
    // assignable to anything because it's a bottom type. So, we don't need to
    // check that.

    final resolvedScope = resolveReturn(None());
    if (resolvedScope is None) {
      return Error([
        CompilerError.invalidReturnLabel(
          'Return label is invalid.',
          location: ErrorLocation(resourceId, expression.returnKeyword.span),
        ),
      ]);
    }

    return innerExpressionContext(expressionType: resolvedScope.value.second)
        .lowerUnambiguous(expression.expression)
        .mapValue((hirExpression) => [
              hir.Expression.return_(
                getId(expression),
                resolvedScope.value.first,
                hirExpression,
              ),
            ]);
  }
}
