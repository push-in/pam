<?php

declare(strict_types=1);

namespace Pam\Api\Profiler;

use Pam\Api\Lifecycle\RequestLifecycleObserver;
use Pam\Http\Request;
use Pam\Http\Response;

final class MemoryProfiler implements RequestLifecycleObserver
{
    /** @var \WeakMap<object, ProfileStart> */
    private \WeakMap $fiberStarts;

    private ?ProfileStart $mainStart = null;

    /** @var array<string, RequestProfile> */
    private array $profiles = [];

    public function __construct(
        private readonly ProfilerMode $mode = ProfilerMode::Disabled,
        private readonly int $maximumProfiles = 500,
    ) {
        if ($maximumProfiles < 1 || $maximumProfiles > 10_000) {
            throw new \InvalidArgumentException('Profiler capacity must be between 1 and 10,000 requests.');
        }
        $this->fiberStarts = new \WeakMap();
    }

    public function starting(Request $request): void
    {
        if ($this->mode === ProfilerMode::Disabled) {
            return;
        }
        $start = new ProfileStart(bin2hex(random_bytes(16)), hrtime(true), memory_get_usage(true));
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            if ($this->mainStart !== null) {
                throw new \LogicException('A profiler request is already active in the main scope.');
            }
            $this->mainStart = $start;
            return;
        }
        if (isset($this->fiberStarts[$fiber])) {
            throw new \LogicException('A profiler request is already active in this Fiber.');
        }
        $this->fiberStarts[$fiber] = $start;
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        $start = $this->takeStart();
        if ($start === null) {
            return;
        }
        $export = $response->export();
        $profile = new RequestProfile(
            $start->token,
            $request->method,
            $request->path,
            $export['status'],
            round((hrtime(true) - $start->nanoseconds) / 1_000_000, 3),
            memory_get_usage(true) - $start->memoryBytes,
            $failure === null ? null : $failure::class,
        );
        $this->profiles[$profile->token] = $profile;
        $response->header('x-debug-token', $profile->token);
        while (count($this->profiles) > $this->maximumProfiles) {
            array_shift($this->profiles);
        }
    }

    public function find(string $token): ?RequestProfile
    {
        return $this->profiles[$token] ?? null;
    }

    /** @return list<RequestProfile> */
    public function recent(int $limit = 50): array
    {
        if ($limit < 1 || $limit > 500) {
            throw new \InvalidArgumentException('Profiler result limit must be between 1 and 500.');
        }
        return array_values(array_slice($this->profiles, -$limit, preserve_keys: true));
    }

    private function takeStart(): ?ProfileStart
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $start = $this->mainStart;
            $this->mainStart = null;
            return $start;
        }
        $start = $this->fiberStarts[$fiber] ?? null;
        unset($this->fiberStarts[$fiber]);
        return $start;
    }
}
