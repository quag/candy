import 'package:code_builder/code_builder.dart' as dart;
import 'package:compiler/compiler.dart';

import 'constants.dart';
import 'declarations/module.dart';

final Query<CandyType, dart.Reference> compileType =
    Query<CandyType, dart.Reference>(
  'dart.compileType',
  evaluateAlways: true,
  provider: (context, type) {
    dart.Reference compile(CandyType type) => compileType(context, type);

    return type.map(
      this_: (_) {
        throw CompilerError.unsupportedFeature(
          'Compiling the `This`-type to Dart is not yet supported.',
        );
      },
      user: (type) {
        if (type == CandyType.any) return _createDartType('Object');
        if (type == CandyType.unit) return _createDartType('void', url: null);
        if (type == CandyType.never) return _createDartType('dynamic');
        if (type == CandyType.bool) return _createDartType('bool');
        if (type == CandyType.number) return _createDartType('Num');
        if (type == CandyType.int) return _createDartType('int');
        if (type == CandyType.float) return _createDartType('double');
        if (type == CandyType.string) return _createDartType('String');

        return _createDartType(
          type.name,
          url: moduleIdToImportUrl(context, type.parentModuleId),
        );
      },
      tuple: (type) {
        final url = moduleIdToImportUrl(context, ModuleId.corePrimitives);
        return dart.TypeReference((b) => b
          ..symbol = 'Tuple${type.items.length}'
          ..url = url
          ..types.addAll(type.items.map((i) => compileType(context, i)))
          ..isNullable = false);
      },
      function: (type) {
        return dart.FunctionType((b) {
          if (type.receiverType != null) {
            b.requiredParameters.add(compile(type.receiverType));
          }
          b
            ..requiredParameters.addAll(type.parameterTypes.map(compile))
            ..returnType = compile(type.returnType);
        });
      },
      union: (_) => dart.refer('dynamic', dartCoreUrl),
      intersection: (_) => dart.refer('dynamic', dartCoreUrl),
      parameter: (type) => dart.refer(type.name),
      reflection: (type) {
        final url = moduleIdToImportUrl(context, ModuleId.coreReflection);
        final id = type.declarationId;
        if (id.isModule) {
          return dart.refer('Module', url);
        } else if (id.isTrait || id.isClass) {
          return dart.refer('Type', url);
        } else if (id.isProperty) {
          final propertyHir = getPropertyDeclarationHir(context, id);
          assert(!propertyHir.isStatic);
          return compileType(
            context,
            CandyType.function(
              receiverType:
                  getPropertyDeclarationParentAsType(context, id).value,
              returnType: propertyHir.type,
            ),
          );
        } else if (id.isFunction) {
          final functionHir = getFunctionDeclarationHir(context, id);
          assert(!functionHir.isStatic);
          return compileType(
            context,
            functionHir.functionType.copyWith(
              receiverType:
                  getPropertyDeclarationParentAsType(context, id).value,
            ),
          );
        } else {
          throw CompilerError.internalError(
            'Invalid reflection target for compiling type: `$id`.',
          );
        }
      },
    );
  },
);

dart.TypeReference _createDartType(
  String name, {
  String url = dartCoreUrl,
  List<dart.TypeReference> typeArguments = const [],
}) {
  return dart.TypeReference((b) => b
    ..symbol = name
    ..url = url
    ..types.addAll(typeArguments)
    ..isNullable = false);
}
