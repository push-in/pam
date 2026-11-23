<?php

declare(strict_types=1);

namespace Nimbus\PHPStan;

use Nimbus\Container\Container;
use PhpParser\Node\Expr\MethodCall;
use PHPStan\Analyser\Scope;
use PHPStan\Reflection\MethodReflection;
use PHPStan\Reflection\ReflectionProvider;
use PHPStan\Type\DynamicMethodReturnTypeExtension;
use PHPStan\Type\ObjectType;
use PHPStan\Type\Type;
use PHPStan\Type\TypeCombinator;

/** `$container->get(Foo::class)` is a `Foo`, as Nimbus binds classes to their instances. */
final readonly class ContainerGetReturnTypeExtension implements DynamicMethodReturnTypeExtension
{
    public function __construct(private ReflectionProvider $reflection)
    {
    }

    public function getClass(): string
    {
        return Container::class;
    }

    public function isMethodSupported(MethodReflection $methodReflection): bool
    {
        return $methodReflection->getName() === 'get';
    }

    public function getTypeFromMethodCall(MethodReflection $methodReflection, MethodCall $methodCall, Scope $scope): ?Type
    {
        $argument = $methodCall->getArgs()[0] ?? null;
        if ($argument === null) {
            return null;
        }
        $types = [];
        foreach ($scope->getType($argument->value)->getConstantStrings() as $id) {
            if (!$this->reflection->hasClass($id->getValue())) {
                return null;
            }
            $types[] = new ObjectType($this->reflection->getClass($id->getValue())->getName());
        }
        return $types === [] ? null : TypeCombinator::union(...$types);
    }
}
