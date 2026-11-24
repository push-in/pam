<?php

declare(strict_types=1);

namespace Nimbus\PHPStan;

use Arcano\ORM\RepositoryInterface;
use Arcano\ORM\Select;
use Nimbus\Database\ArcanoManager;
use PhpParser\Node\Expr\MethodCall;
use PHPStan\Analyser\Scope;
use PHPStan\Reflection\MethodReflection;
use PHPStan\Reflection\ReflectionProvider;
use PHPStan\Type\DynamicMethodReturnTypeExtension;
use PHPStan\Type\Generic\GenericObjectType;
use PHPStan\Type\ObjectType;
use PHPStan\Type\Type;

/**
 * `$arcano->repository(Product::class)` is a `RepositoryInterface<Product>` and
 * `$arcano->select(Product::class)` a `Select<Product>`, so `findByPK()`, `fetchOne()` and
 * `fetchAll()` return typed entities whose properties PHPStan checks.
 */
final readonly class ArcanoManagerReturnTypeExtension implements DynamicMethodReturnTypeExtension
{
    private const array GENERICS = ['repository' => RepositoryInterface::class, 'select' => Select::class];

    public function __construct(private ReflectionProvider $reflection)
    {
    }

    public function getClass(): string
    {
        return ArcanoManager::class;
    }

    public function isMethodSupported(MethodReflection $methodReflection): bool
    {
        return isset(self::GENERICS[$methodReflection->getName()]);
    }

    public function getTypeFromMethodCall(MethodReflection $methodReflection, MethodCall $methodCall, Scope $scope): ?Type
    {
        $argument = $methodCall->getArgs()[0] ?? null;
        $generic = self::GENERICS[$methodReflection->getName()] ?? null;
        if ($argument === null || $generic === null) {
            return null;
        }
        $roles = $scope->getType($argument->value)->getConstantStrings();
        if (count($roles) !== 1 || !$this->reflection->hasClass($roles[0]->getValue())) {
            return null;
        }
        $entity = $this->reflection->getClass($roles[0]->getValue());
        if ($entity->isInterface() || $entity->isEnum()) {
            return null;
        }
        return new GenericObjectType($generic, [new ObjectType($entity->getName())]);
    }
}
