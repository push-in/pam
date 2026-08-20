<?php

declare(strict_types=1);

namespace Pam\Api\Container;

use Pam\Api\Routing\RouteBindable;

final class Container
{
    /** @var array<class-string|string, Binding> */
    private array $bindings = [];

    /** @var array<class-string|string, mixed> */
    private array $singletons = [];

    /** @var \WeakMap<object, array<class-string|string, mixed>> */
    private \WeakMap $fiberScopes;

    /** @var array<class-string|string, mixed>|null */
    private ?array $mainScope = null;

    /** @var array<class-string, \Closure(string, Container): object> */
    private array $routeBindings = [];

    public function __construct()
    {
        $this->fiberScopes = new \WeakMap();
    }

    /** @param class-string $class @param callable(string, Container): object $resolver */
    public function bindRoute(string $class, callable $resolver): self
    {
        $this->routeBindings[$class] = \Closure::fromCallable($resolver);
        return $this;
    }

    /** @param class-string|string $id */
    public function bind(string $id, callable|string|null $factory = null): self
    {
        return $this->register($id, $factory, BindingLifetime::Transient);
    }

    /** @param class-string|string $id */
    public function singleton(string $id, callable|string|null $factory = null): self
    {
        return $this->register($id, $factory, BindingLifetime::Singleton);
    }

    /** @param class-string|string $id */
    public function scoped(string $id, callable|string|null $factory = null): self
    {
        return $this->register($id, $factory, BindingLifetime::Scoped);
    }

    public function instance(string $id, mixed $instance): self
    {
        $this->singletons[$id] = $instance;
        return $this;
    }

    public function scopedInstance(string $id, mixed $instance): self
    {
        $scope = $this->currentScope();
        if ($scope === null) {
            throw new \LogicException("Scoped entry {$id} cannot be registered outside a request scope.");
        }
        $scope[$id] = $instance;
        $this->replaceCurrentScope($scope);
        return $this;
    }

    public function scopedValue(string $id): mixed
    {
        return $this->currentScope()[$id] ?? null;
    }

    /** @return array{state: int, scopedEntries: int, singletonEntries: int, bindings: int} */
    public function diagnostics(): array
    {
        $scope = $this->currentScope();
        return [
            'state' => ($scope !== null ? ContainerState::RequestActive : ContainerState::Idle)->value,
            'scopedEntries' => count($scope ?? []),
            'singletonEntries' => count($this->singletons),
            'bindings' => count($this->bindings),
        ];
    }

    public function beginScope(): void
    {
        if ($this->currentScope() !== null) {
            throw new \LogicException('A PAM API container scope is already active.');
        }
        $this->replaceCurrentScope([]);
    }

    public function endScope(): void
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainScope = null;
            return;
        }
        unset($this->fiberScopes[$fiber]);
    }

    public function get(string $id): mixed
    {
        if (array_key_exists($id, $this->singletons)) {
            return $this->singletons[$id];
        }
        $scope = $this->currentScope();
        if ($scope !== null && array_key_exists($id, $scope)) {
            return $scope[$id];
        }

        $binding = $this->bindings[$id] ?? null;
        if ($binding === null) {
            if (!class_exists($id)) {
                throw new \RuntimeException("Container entry {$id} is not bound and is not a class.");
            }
            return $this->build($id);
        }

        $value = ($binding->factory)($this);
        if ($binding->lifetime === BindingLifetime::Singleton) {
            $this->singletons[$id] = $value;
        } elseif ($binding->lifetime === BindingLifetime::Scoped) {
            if ($scope === null) {
                throw new \LogicException("Scoped entry {$id} was resolved outside a request scope.");
            }
            $scope[$id] = $value;
            $this->replaceCurrentScope($scope);
        }
        return $value;
    }

    /**
     * @param array{object, string} $callable
     * @param array<string, mixed> $named
     * @param list<mixed> $provided
     */
    public function call(array $callable, array $named = [], array $provided = []): mixed
    {
        $reflection = new \ReflectionMethod($callable[0], $callable[1]);
        $arguments = $this->resolveParameters($reflection->getParameters(), $named, $provided);
        return $reflection->invokeArgs($callable[0], $arguments);
    }

    /** @param class-string $class */
    private function build(string $class): object
    {
        $reflection = new \ReflectionClass($class);
        if (!$reflection->isInstantiable()) {
            throw new \RuntimeException("Container entry {$class} is not instantiable.");
        }
        $constructor = $reflection->getConstructor();
        if ($constructor === null) {
            return $reflection->newInstance();
        }
        return $reflection->newInstanceArgs(
            $this->resolveParameters($constructor->getParameters()),
        );
    }

    /**
     * @param list<\ReflectionParameter> $parameters
     * @param array<string, mixed> $named
     * @param list<mixed> $provided
     * @return list<mixed>
     */
    private function resolveParameters(array $parameters, array $named = [], array $provided = []): array
    {
        $arguments = [];
        foreach ($parameters as $parameter) {
            $type = $parameter->getType();
            if (array_key_exists($parameter->getName(), $named)) {
                $value = $named[$parameter->getName()];
                if ($type instanceof \ReflectionNamedType && !$type->isBuiltin()) {
                    $class = $type->getName();
                    $resolver = $this->routeBindings[$class] ?? null;
                    if ($resolver !== null) {
                        $resolved = $resolver(self::routeValue($value), $this);
                    } elseif (is_a($class, RouteBindable::class, true)) {
                        $resolved = $class::resolveRouteBinding(self::routeValue($value));
                    } else {
                        throw new \RuntimeException(
                            "Route parameter {$parameter->getName()} cannot resolve {$class}; register bindRoute() or implement RouteBindable.",
                        );
                    }
                    if (!$resolved instanceof $class) {
                        throw new \UnexpectedValueException("Route binding for {$class} returned another type.");
                    }
                    $arguments[] = $resolved;
                } else {
                    $arguments[] = $value;
                }
                continue;
            }
            if ($type instanceof \ReflectionNamedType && !$type->isBuiltin()) {
                $class = $type->getName();
                $matched = null;
                foreach ($provided as $candidate) {
                    if ($candidate instanceof $class) {
                        $matched = $candidate;
                        break;
                    }
                }
                $arguments[] = $matched ?? $this->get($class);
                continue;
            }
            if ($parameter->isDefaultValueAvailable()) {
                $arguments[] = $parameter->getDefaultValue();
                continue;
            }
            if ($parameter->allowsNull()) {
                $arguments[] = null;
                continue;
            }
            throw new \RuntimeException(
                "Unable to resolve parameter \${$parameter->getName()} for {$parameter->getDeclaringFunction()->getName()}().",
            );
        }
        return $arguments;
    }

    private function register(
        string $id,
        callable|string|null $factory,
        BindingLifetime $lifetime,
    ): self {
        $resolver = match (true) {
            $factory === null => static function (self $container) use ($id): mixed {
                if (!class_exists($id)) {
                    throw new \RuntimeException("Container entry {$id} is not a class.");
                }
                return $container->build($id);
            },
            is_string($factory) => static fn (self $container): mixed => $container->get($factory),
            default => $factory,
        };
        $this->bindings[$id] = new Binding($resolver, $lifetime);
        unset($this->singletons[$id]);
        $scope = $this->currentScope();
        if ($scope !== null) {
            unset($scope[$id]);
            $this->replaceCurrentScope($scope);
        }
        return $this;
    }

    /** @return array<class-string|string, mixed>|null */
    private function currentScope(): ?array
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            return $this->mainScope;
        }
        return $this->fiberScopes[$fiber] ?? null;
    }

    /** @param array<class-string|string, mixed> $scope */
    private function replaceCurrentScope(array $scope): void
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainScope = $scope;
            return;
        }
        $this->fiberScopes[$fiber] = $scope;
    }

    private static function routeValue(mixed $value): string
    {
        if (is_string($value)) {
            return $value;
        }
        if (is_int($value) || is_float($value) || is_bool($value) || $value instanceof \Stringable) {
            return (string) $value;
        }
        throw new \UnexpectedValueException('Route binding values must be scalar or stringable.');
    }
}
