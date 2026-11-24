<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Collects the requests of `Http::pool()`. Requests are keyed by `as()` or by their order.
 */
final class Pool
{
    /** @var array<array-key, array{PendingRequest, string, string, array<array-key, mixed>, array<string, mixed>}> */
    private array $entries = [];

    private ?string $key = null;

    private ?PendingRequest $next = null;

    public function __construct(private readonly PendingRequest $request)
    {
    }

    /** Names the next request of the pool. */
    public function as(string $key): self
    {
        $this->key = $key;
        return $this;
    }

    /**
     * Configures only the next request: `$pool->with(fn ($r) => $r->withToken($t))->get($url)`.
     *
     * @param \Closure(PendingRequest): PendingRequest $configure
     */
    public function with(\Closure $configure): self
    {
        $this->next = $configure($this->request);
        return $this;
    }

    /** @param array<string, mixed> $query */
    public function get(string $url, array $query = []): self
    {
        return $this->add($this->next ?? $this->request, 'GET', $url, [], $query);
    }

    /** @param array<array-key, mixed> $data */
    public function post(string $url, array $data = []): self
    {
        return $this->add($this->next ?? $this->request, 'POST', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function put(string $url, array $data = []): self
    {
        return $this->add($this->next ?? $this->request, 'PUT', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function patch(string $url, array $data = []): self
    {
        return $this->add($this->next ?? $this->request, 'PATCH', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function delete(string $url, array $data = []): self
    {
        return $this->add($this->next ?? $this->request, 'DELETE', $url, $data);
    }

    /**
     * @param array<array-key, mixed> $data
     * @param array<string, mixed> $query
     * @internal
     */
    public function add(PendingRequest $request, string $method, string $url, array $data = [], array $query = []): self
    {
        $entry = [$request, $method, $url, $data, $query];
        $this->next = null;
        if ($this->key !== null) {
            $this->entries[$this->key] = $entry;
            $this->key = null;
        } else {
            $this->entries[] = $entry;
        }
        return $this;
    }

    /**
     * @return array<array-key, array{PendingRequest, string, string, array<array-key, mixed>, array<string, mixed>}>
     * @internal
     */
    public function entries(): array
    {
        return $this->entries;
    }
}
