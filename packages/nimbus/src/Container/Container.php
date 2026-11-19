<?php

declare(strict_types=1);

namespace Nimbus\Container;

use Nimbus\Routing\RouteBindable;

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

    /**
     * Parameter plans compiled once per worker: reflection never runs on the request path.
     *
     * @var array<string, list<ParameterPlan>>
     */
    private array $plans = [];

    /** @var \WeakMap<\Closure, list<ParameterPlan>> */
    private \WeakMap $closurePlans;

    public function __construct()
    {
        $this->fiberScopes = new \WeakMap();
        $this->closurePlans = new \WeakMap();
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

    /** Whether the entry is bound, registered as an instance or set in the current scope. */
    public function has(string $id): bool
    {
        if (isset($this->bindings[$id]) || array_key_exists($id, $this->singletons)) {
            return true;
        }
        $scope = $this->currentScope();
        return $scope !== null && array_key_exists($id, $scope);
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
            throw new \LogicException('A Nimbus container scope is already active.');
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
        [$object, $method] = $callable;
        $key = $object::class . '::' . $method;
        $plan = $this->plans[$key] ??= ParameterPlan::compile(
            (new \ReflectionMethod($object, $method))->getParameters(),
        );
        return $object->{$method}(...$this->resolveParameters($plan, $named, $provided));
    }

    /**
     * Calls a closure, injecting route parameters by name, the provided objects by type
     * and everything else from the container.
     *
     * @param array<string, mixed> $named
     * @param list<mixed> $provided
     */
    public function callFunction(\Closure $function, array $named = [], array $provided = []): mixed
    {
        $plan = $this->closurePlans[$function] ??= ParameterPlan::compile(
            (new \ReflectionFunction($function))->getParameters(),
        );
        return $function(...$this->resolveParameters($plan, $named, $provided));
    }

    /** @param class-string $class */
    private function build(string $class): object
    {
        $plan = $this->plans[$class] ??= self::constructorPlan($class);
        return $plan === [] ? new $class() : new $class(...$this->resolveParameters($plan));
    }

    /**
     * @param class-string $class
     * @return list<ParameterPlan>
     */
    private static function constructorPlan(string $class): array
    {
        $reflection = new \ReflectionClass($class);
        if (!$reflection->isInstantiable()) {
            throw new \RuntimeException("Container entry {$class} is not instantiable.");
        }
        return ParameterPlan::compile($reflection->getConstructor()?->getParameters() ?? []);
    }

    /**
     * @param list<ParameterPlan> $parameters
     * @param array<string, mixed> $named
     * @param list<mixed> $provided
     * @return list<mixed>
     */
    private function resolveParameters(array $parameters, array $named = [], array $provided = []): array
    {
        $arguments = [];
        foreach ($parameters as $parameter) {
            if (array_key_exists($parameter->name, $named)) {
                $value = $named[$parameter->name];
                $class = $parameter->class;
                if ($class !== null) {
                    $resolver = $this->routeBindings[$class] ?? null;
                    if ($resolver !== null) {
                        $resolved = $resolver(self::routeValue($value), $this);
                    } elseif (is_a($class, RouteBindable::class, true)) {
                        $resolved = $class::resolveRouteBinding(self::routeValue($value));
                    } else {
                        throw new \RuntimeException(
                            "Route parameter {$parameter->name} cannot resolve {$class}; register bindRoute() or implement RouteBindable.",
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
            if ($parameter->class !== null) {
                $class = $parameter->class;
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
            if ($parameter->hasDefault) {
                $arguments[] = $parameter->defaultValue();
                continue;
            }
            if ($parameter->nullable) {
                $arguments[] = null;
                continue;
            }
            throw new \RuntimeException("Unable to resolve parameter \${$parameter->name} for {$parameter->function}().");
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
