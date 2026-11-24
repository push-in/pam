<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Nimbus\Container\Container;

/**
 * Values a middleware hands to the action of the same request, as Laravel's
 * `$request->attributes`. One context per request (a scoped binding); it is gone when
 * the request ends.
 *
 * ```php
 * // middleware
 * $context = $container->get(RequestContext::class);
 * $context->put($account);              // typed: the action receives `Account $account`
 * $context->set('monitor', $monitor);   // keyed: read with $context->get('monitor')
 *
 * // action
 * public function show(Account $account, RequestContext $context): array { ... }
 * ```
 *
 * A typed value wins over a route segment of the same name: with `put($account)` an action
 * parameter `Account $account` receives that instance even when the route has `{account}`.
 */
final class RequestContext
{
    /** @var array<string, mixed> */
    private array $values = [];

    public function __construct(private readonly Container $container)
    {
    }

    /** Stores `$value` by its class (or `$as`), injected into action parameters of that type. */
    public function put(object $value, ?string $as = null): self
    {
        $id = $as ?? $value::class;
        if ($as !== null && !$value instanceof $as) {
            throw new \InvalidArgumentException("The context value is not an instance of {$as}.");
        }
        $this->container->scopedInstance($id, $value);
        $this->values[$id] = $value;
        return $this;
    }

    public function set(string $key, mixed $value): self
    {
        if ($key === '') {
            throw new \InvalidArgumentException('A request context key cannot be empty.');
        }
        if (is_object($value) && $value instanceof $key) {
            return $this->put($value, $key);
        }
        $this->values[$key] = $value;
        return $this;
    }

    public function get(string $key, mixed $default = null): mixed
    {
        return array_key_exists($key, $this->values) ? $this->values[$key] : $default;
    }

    /**
     * The typed value stored with `put()`.
     *
     * @template T of object
     * @param class-string<T> $class
     * @return ?T
     */
    public function typed(string $class): ?object
    {
        $value = $this->values[$class] ?? null;
        return $value instanceof $class ? $value : null;
    }

    public function has(string $key): bool
    {
        return array_key_exists($key, $this->values);
    }

    public function forget(string $key): void
    {
        unset($this->values[$key]);
    }

    /** @return array<string, mixed> */
    public function all(): array
    {
        return $this->values;
    }
}
