import 'package:parser/parser.dart' as ast;

import '../../errors.dart';
import '../../query.dart';
import '../ast.dart';
import '../hir.dart' as hir;
import '../hir/ids.dart';
import 'declarations/declarations.dart';
import 'declarations/function.dart';

final getBody = Query<DeclarationId, List<hir.Statement>>(
  'getBody',
  provider: (context, declarationId) {
    if (declarationId.isFunction) {
      final functionAst = getFunctionDeclarationAst(context, declarationId);

      final identifiers = <String, hir.Identifier>{
        for (final parameter in functionAst.valueParameters)
          parameter.name.name: hir.Identifier.parameter(parameter.name.name, 0),
      };
      return functionAst.body.statements.map<hir.Statement>((statement) {
        if (statement is ast.Expression) {
          return hir.Statement.expression(_mapExpression(
            statement,
            identifiers,
            declarationId.resourceId,
          ));
        } else {
          throw CompilerError.unsupportedFeature(
            'Unsupported statement.',
            location: ErrorLocation(declarationId.resourceId, statement.span),
          );
        }
      }).toList();
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

hir.Expression _mapExpression(
  ast.Expression expression,
  Map<String, hir.Identifier> identifiers,
  ResourceId resourceId,
) {
  hir.Expression map(ast.Expression expression) =>
      _mapExpression(expression, identifiers, resourceId);

  if (expression is ast.Literal) {
    return hir.Expression.literal(_mapLiteral(expression.value, resourceId));
  } else if (expression is ast.Identifier) {
    final identifier = expression.value.name;
    final known = identifiers[identifier];
    if (known != null) return hir.Expression.identifier(known);

    if (identifier == 'print') {
      return hir.Expression.identifier(hir.Identifier.printFunction());
    }
    throw CompilerError.undefinedIdentifier(
      "Couldn't resolve identifier `$identifier`",
      location: ErrorLocation(resourceId, expression.value.span),
    );
  } else if (expression is ast.CallExpression) {
    return hir.Expression.call(
      map(expression.target),
      expression.arguments
          .map((argument) => hir.ValueArgument(
                name: argument.name?.name,
                expression: map(argument.expression),
              ))
          .toList(),
    );
  } else {
    throw CompilerError.unsupportedFeature(
      'Unsupported expression.',
      location: ErrorLocation(resourceId, expression.span),
    );
  }
}

hir.Literal _mapLiteral(
    ast.LiteralToken<dynamic> token, ResourceId resourceId) {
  if (token is ast.BooleanLiteralToken) return hir.Literal.boolean(token.value);
  if (token is ast.IntegerLiteralToken) return hir.Literal.integer(token.value);
  throw CompilerError.unsupportedFeature(
    'Unsupported literal.',
    location: ErrorLocation(resourceId, token.span),
  );
}
