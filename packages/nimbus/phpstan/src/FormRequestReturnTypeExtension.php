<?php

declare(strict_types=1);

namespace Nimbus\PHPStan;

use Nimbus\Validation\FormRequest;
use PhpParser\Node\Expr\MethodCall;
use PHPStan\Analyser\Scope;
use PHPStan\Reflection\MethodReflection;
use PHPStan\Type\Accessory\AccessoryNonEmptyStringType;
use PHPStan\Type\ArrayType;
use PHPStan\Type\BooleanType;
use PHPStan\Type\Constant\ConstantArrayTypeBuilder;
use PHPStan\Type\Constant\ConstantStringType;
use PHPStan\Type\DynamicMethodReturnTypeExtension;
use PHPStan\Type\IntegerType;
use PHPStan\Type\IntersectionType;
use PHPStan\Type\MixedType;
use PHPStan\Type\NullType;
use PHPStan\Type\StringType;
use PHPStan\Type\Type;
use PHPStan\Type\TypeCombinator;

/**
 * Types the validated input of a FormRequest from its `rules()`: `input('price')` of a
 * `['required', 'integer']` field is an `int`, of an optional `string` field `string|null`
 * (or the default's type), and `validated()` is an array shape with optional keys for the
 * fields that are not required. Custom ValidationRule objects leave the field `mixed`.
 *
 * The rules are read by instantiating the request without its constructor and calling
 * `rules()`, so they must not depend on the request.
 */
final class FormRequestReturnTypeExtension implements DynamicMethodReturnTypeExtension
{
    /** @var array<string, array<string, array{Type, bool}>|null> */
    private array $fields = [];

    public function getClass(): string
    {
        return FormRequest::class;
    }

    public function isMethodSupported(MethodReflection $methodReflection): bool
    {
        return in_array($methodReflection->getName(), ['input', 'validated'], true);
    }

    public function getTypeFromMethodCall(MethodReflection $methodReflection, MethodCall $methodCall, Scope $scope): ?Type
    {
        $classes = $scope->getType($methodCall->var)->getObjectClassNames();
        if (count($classes) !== 1) {
            return null;
        }
        $fields = $this->fields($classes[0]);
        if ($fields === null) {
            return null;
        }
        if ($methodReflection->getName() === 'validated') {
            $shape = ConstantArrayTypeBuilder::createEmpty();
            foreach ($fields as $name => [$type, $required]) {
                $shape->setOffsetValueType(new ConstantStringType($name), $type, !$required);
            }
            return $shape->getArray();
        }
        $arguments = $methodCall->getArgs();
        $keys = isset($arguments[0]) ? $scope->getType($arguments[0]->value)->getConstantStrings() : [];
        if (count($keys) !== 1 || !isset($fields[$keys[0]->getValue()])) {
            return null;
        }
        [$type, $required] = $fields[$keys[0]->getValue()];
        if ($required) {
            return $type;
        }
        $default = isset($arguments[1]) ? $scope->getType($arguments[1]->value) : new NullType();
        return TypeCombinator::union($type, $default);
    }

    /**
     * @param class-string|string $class
     * @return array<string, array{Type, bool}>|null Field types and whether they are required.
     */
    private function fields(string $class): ?array
    {
        if (array_key_exists($class, $this->fields)) {
            return $this->fields[$class];
        }
        $rules = null;
        try {
            // Runtime reflection on purpose: the rules are values that only running rules() yields.
            if (class_exists($class) && is_subclass_of($class, FormRequest::class)) { // @phpstan-ignore phpstanApi.runtimeReflection
                $reflection = new \ReflectionClass($class); // @phpstan-ignore phpstanApi.runtimeReflection
                if (!$reflection->isAbstract()) {
                    $rules = $reflection->newInstanceWithoutConstructor()->rules();
                }
            }
        } catch (\Throwable) {
            $rules = null;
        }
        if ($rules === null) {
            return $this->fields[$class] = null;
        }
        $fields = [];
        foreach ($rules as $field => $fieldRules) {
            if (str_contains($field, '.')) {
                // Nested and wildcard paths live inside their parent field's array.
                continue;
            }
            $fieldRules = \Nimbus\Validation\Validator::parse($fieldRules);
            $names = [];
            foreach ($fieldRules as $rule) {
                if (is_string($rule)) {
                    $names[] = explode(':', $rule, 2)[0];
                }
            }
            $custom = count($names) !== count($fieldRules);
            $required = in_array('required', $names, true);
            $type = $custom ? new MixedType() : self::type($names, $required);
            if (in_array('nullable', $names, true)) {
                $type = TypeCombinator::addNull($type);
            }
            $fields[$field] = [$type, $required];
        }
        return $this->fields[$class] = $fields;
    }

    /** @param list<string> $rules */
    private static function type(array $rules, bool $required): Type
    {
        return match (true) {
            in_array('integer', $rules, true) => new IntegerType(),
            in_array('numeric', $rules, true) => TypeCombinator::union(new IntegerType(), new \PHPStan\Type\FloatType()),
            in_array('boolean', $rules, true) => new BooleanType(),
            in_array('array', $rules, true) => new ArrayType(new MixedType(), new MixedType()),
            in_array('string', $rules, true), in_array('email', $rules, true) => $required
                ? new IntersectionType([new StringType(), new AccessoryNonEmptyStringType()])
                : new StringType(),
            default => new MixedType(),
        };
    }
}
