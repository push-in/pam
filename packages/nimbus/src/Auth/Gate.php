<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

/**
 * Authorization rules: closures defined per ability, policies per entity class (resolved
 * through the container, one method per ability: `view-any` calls `viewAny()`) and `before()`
 * hooks such as a super-admin bypass. An ability with neither a policy nor a definition falls
 * back to the principal's own `can()`, which for tokens means their abilities (scopes), so
 * `can('products:write')` routes keep checking token scopes.
 *
 * Checks without an explicit principal use the request's authenticated one.
 */
final class Gate
{
    /** @var array<string, \Closure> */
    private array $abilities = [];

    /** @var array<class-string, class-string> */
    private array $policies = [];

    /** @var list<\Closure> */
    private array $before = [];

    public function __construct(private readonly Container $container)
    {
    }

    /**
     * @param \Closure $callback Receives the principal and the check's arguments; true allows.
     */
    public function define(string $ability, \Closure $callback): self
    {
        if ($ability === '') {
            throw new \InvalidArgumentException('Abilities cannot be empty.');
        }
        $this->abilities[$ability] = $callback;
        return $this;
    }

    /**
     * @param class-string $entity
     * @param class-string $policy
     */
    public function policy(string $entity, string $policy): self
    {
        if (!class_exists($entity) && !interface_exists($entity) || !class_exists($policy)) {
            throw new \InvalidArgumentException('Policies map an existing entity class to an existing policy class.');
        }
        $this->policies[$entity] = $policy;
        return $this;
    }

    /**
     * Runs before every check with the principal, the ability and the arguments; a boolean
     * result decides the check, null continues.
     *
     * @param \Closure(Principal, string, list<mixed>): ?bool $callback
     */
    public function before(\Closure $callback): self
    {
        $this->before[] = $callback;
        return $this;
    }

    public function has(string $ability): bool
    {
        return isset($this->abilities[$ability]);
    }

    /**
     * The entity class with a policy whose short name matches a route parameter (`product` for `App\Product`).
     *
     * @return class-string|null
     */
    public function entityForParameter(string $parameter): ?string
    {
        foreach (array_keys($this->policies) as $entity) {
            $short = substr($entity, (int) strrpos('\\' . $entity, '\\'));
            if (lcfirst($short) === $parameter || strtolower($short) === strtolower($parameter)) {
                return $entity;
            }
        }
        return null;
    }

    /** @param object|class-string $entity */
    public function policyFor(object|string $entity): ?object
    {
        $class = is_object($entity) ? $entity::class : $entity;
        $policy = $this->policies[$class] ?? null;
        if ($policy === null) {
            foreach ($this->policies as $candidate => $policyClass) {
                if (is_a($class, $candidate, true)) {
                    $policy = $policyClass;
                    break;
                }
            }
        }
        if ($policy === null) {
            return null;
        }
        $instance = $this->container->get($policy);
        return is_object($instance) ? $instance : null;
    }

    /** @param list<mixed> $arguments */
    public function check(?Principal $principal, string $ability, array $arguments = []): bool
    {
        if ($principal === null || $ability === '') {
            return false;
        }
        foreach ($this->before as $before) {
            $decision = $before($principal, $ability, $arguments);
            if (is_bool($decision)) {
                return $decision;
            }
        }
        $subject = $arguments[0] ?? null;
        $policy = is_object($subject) || (is_string($subject) && class_exists($subject))
            ? $this->policyFor($subject)
            : null;
        if ($policy !== null) {
            if (method_exists($policy, 'before')) {
                $decision = $policy->before($principal, $ability);
                if (is_bool($decision)) {
                    return $decision;
                }
            }
            $method = lcfirst(str_replace(['-', '_', ' '], '', ucwords($ability, '-_ ')));
            if (method_exists($policy, $method)) {
                $policyArguments = is_object($subject) ? $arguments : array_slice($arguments, 1);
                return $policy->{$method}($principal, ...$policyArguments) === true;
            }
        }
        $callback = $this->abilities[$ability] ?? null;
        if ($callback !== null) {
            return $callback($principal, ...$arguments) === true;
        }
        return $policy === null && $principal->can($ability);
    }

    public function allows(string $ability, mixed ...$arguments): bool
    {
        return $this->check($this->principal(), $ability, array_values($arguments));
    }

    public function denies(string $ability, mixed ...$arguments): bool
    {
        return !$this->allows($ability, ...$arguments);
    }

    /** @throws HttpException 401 without an authenticated principal, 403 when the check fails. */
    public function authorize(string $ability, mixed ...$arguments): void
    {
        $principal = $this->principal();
        if ($principal === null) {
            throw new HttpException(401, ProblemCode::Unauthenticated, 'Authentication is required.');
        }
        if (!$this->check($principal, $ability, array_values($arguments))) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
    }

    /** The request's authenticated principal, if any. */
    public function principal(): ?Principal
    {
        $auth = $this->container->scopedValue(AuthContext::class);
        if ($auth instanceof AuthContext) {
            return $auth->principal;
        }
        $principal = $this->container->scopedValue(Principal::class);
        return $principal instanceof Principal ? $principal : null;
    }
}
