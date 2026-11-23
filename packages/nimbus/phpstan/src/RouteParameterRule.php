<?php

declare(strict_types=1);

namespace Nimbus\PHPStan;

use Nimbus\App;
use PhpParser\Node;
use PhpParser\Node\Expr\ArrowFunction;
use PhpParser\Node\Expr\Closure;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Identifier;
use PhpParser\Node\NullableType;
use PhpParser\Node\Expr\Variable;
use PHPStan\Analyser\Scope;
use PHPStan\Rules\Rule;
use PHPStan\Rules\RuleErrorBuilder;
use PHPStan\Type\ObjectType;

/**
 * Nimbus injects a route closure's scalar parameters from the route's `{placeholders}` by
 * name, as strings. This rule reports, for `$app->get('/path/{id}', fn (string $id) => ...)`
 * closures, a required scalar parameter that no placeholder provides and a placeholder
 * parameter declared as int, float or bool; both fail on every request.
 *
 * @implements Rule<MethodCall>
 */
final class RouteParameterRule implements Rule
{
    private const array METHODS = ['get', 'post', 'put', 'patch', 'delete', 'head', 'options'];

    private const array SCALARS = ['int', 'string', 'float', 'bool'];

    public function getNodeType(): string
    {
        return MethodCall::class;
    }

    public function processNode(Node $node, Scope $scope): array
    {
        if (!$node->name instanceof Identifier || !in_array($node->name->toLowerString(), self::METHODS, true)) {
            return [];
        }
        if (!(new ObjectType(App::class))->isSuperTypeOf($scope->getType($node->var))->yes()) {
            return [];
        }
        $arguments = $node->getArgs();
        if (count($arguments) < 2) {
            return [];
        }
        $handler = $arguments[1]->value;
        if (!$handler instanceof Closure && !$handler instanceof ArrowFunction) {
            return [];
        }
        $paths = $scope->getType($arguments[0]->value)->getConstantStrings();
        if (count($paths) !== 1) {
            return [];
        }
        $path = $paths[0]->getValue();
        preg_match_all('/\{([A-Za-z_][A-Za-z0-9_]*)\}/', $path, $matches);
        $placeholders = $matches[1];
        $errors = [];
        foreach ($handler->params as $parameter) {
            if ($parameter->default !== null || $parameter->variadic || !$parameter->var instanceof Variable || !is_string($parameter->var->name)) {
                continue;
            }
            $type = $parameter->type instanceof NullableType ? $parameter->type->type : $parameter->type;
            if (!$type instanceof Identifier || !in_array($type->toLowerString(), self::SCALARS, true)) {
                continue;
            }
            $name = $parameter->var->name;
            if (in_array($name, $placeholders, true) && $type->toLowerString() !== 'string') {
                $errors[] = RuleErrorBuilder::message(sprintf(
                    'Route parameter {%s} of %s is a string; declare $%s as string and convert it.',
                    $name,
                    $path,
                    $name,
                ))->identifier('nimbus.routeParameterType')->line($parameter->getStartLine())->build();
                continue;
            }
            if (!in_array($name, $placeholders, true)) {
                $errors[] = RuleErrorBuilder::message(sprintf(
                    'Route %s has no {%s} placeholder for the closure parameter $%s.',
                    $path,
                    $name,
                    $name,
                ))->identifier('nimbus.routeParameter')->line($parameter->getStartLine())->build();
            }
        }
        return $errors;
    }
}
