<?php

declare(strict_types=1);

namespace Pam\Runtime {
    use Pam\Async\FiberContext;

    enum ScopeState: int
    {
        case Active = 1;
        case Closing = 2;
        case Closed = 3;
    }

    final class RequestScope
    {
        private const CONTEXT_KEY = 'pam.request_scope';

        /** @var list<callable():void> */
        private array $cleanups = [];

        /** @var array<string, mixed> */
        private array $values = [];

        private ScopeState $state = ScopeState::Active;
        private int $cleanupFailures = 0;

        private function __construct(
            public readonly string $id,
            private readonly ?LeakSnapshot $leakSnapshot,
        ) {
        }

        public static function begin(string $id, bool $sampleLeaks = false): self
        {
            if ($id === '') {
                throw new \InvalidArgumentException('Request scope ID cannot be empty.');
            }
            if (FiberContext::get(self::CONTEXT_KEY) instanceof self) {
                throw new \LogicException('A request scope is already active in this Fiber.');
            }
            $scope = new self($id, $sampleLeaks ? LeakDetector::snapshot() : null);
            FiberContext::set(self::CONTEXT_KEY, $scope);
            LeakDetector::scopeOpened();
            return $scope;
        }

        public static function current(): self
        {
            $scope = FiberContext::get(self::CONTEXT_KEY);
            if (!$scope instanceof self || $scope->state !== ScopeState::Active) {
                throw new \LogicException('No active Pam request scope exists in this Fiber.');
            }
            return $scope;
        }

        public static function currentOrNull(): ?self
        {
            $scope = FiberContext::get(self::CONTEXT_KEY);
            return $scope instanceof self && $scope->state === ScopeState::Active ? $scope : null;
        }

        public static function finish(int $memoryThresholdBytes = 8 * 1024 * 1024): void
        {
            $scope = FiberContext::get(self::CONTEXT_KEY);
            if (!$scope instanceof self) {
                return;
            }
            try {
                $scope->close($memoryThresholdBytes);
            } finally {
                FiberContext::remove(self::CONTEXT_KEY);
            }
        }

        public function state(): ScopeState
        {
            return $this->state;
        }

        public function set(string $key, mixed $value): self
        {
            $this->assertActive();
            if ($key === '') {
                throw new \InvalidArgumentException('Request scope keys cannot be empty.');
            }
            $this->values[$key] = $value;
            return $this;
        }

        public function get(string $key, mixed $default = null): mixed
        {
            return $this->values[$key] ?? $default;
        }

        public function forget(string $key): void
        {
            unset($this->values[$key]);
        }

        /** @param callable():void $cleanup */
        public function defer(callable $cleanup): self
        {
            $this->assertActive();
            $this->cleanups[] = $cleanup;
            return $this;
        }

        public function manage(mixed $resource, ?callable $cleanup = null): mixed
        {
            $this->assertActive();
            $cleanup ??= match (true) {
                is_resource($resource) => static function () use ($resource): void {
                    self::closeResource($resource);
                },
                is_object($resource) && method_exists($resource, 'close') => static function () use ($resource): void {
                    $resource->close();
                },
                is_object($resource) && method_exists($resource, 'dispose') => static function () use ($resource): void {
                    $resource->dispose();
                },
                default => throw new \InvalidArgumentException(
                    'Managed values require a cleanup callback, close(), or dispose().',
                ),
            };
            $this->defer($cleanup);
            return $resource;
        }

        private static function closeResource(mixed $resource): void
        {
            if (is_resource($resource)) {
                fclose($resource);
            }
        }

        private function close(int $memoryThresholdBytes): void
        {
            if ($this->state === ScopeState::Closed) {
                return;
            }
            $this->state = ScopeState::Closing;
            while (($cleanup = array_pop($this->cleanups)) !== null) {
                try {
                    $cleanup();
                } catch (\Throwable $error) {
                    ++$this->cleanupFailures;
                    \Pam\Observability\Telemetry::log('error', 'Request cleanup failed', [
                        'requestId' => $this->id,
                        'exception' => $error::class,
                        'message' => $error->getMessage(),
                    ]);
                }
            }
            $this->values = [];
            $this->state = ScopeState::Closed;
            LeakDetector::scopeClosed(
                $this->id,
                $this->leakSnapshot,
                $memoryThresholdBytes,
                $this->cleanupFailures,
            );
        }

        private function assertActive(): void
        {
            if ($this->state !== ScopeState::Active) {
                throw new \LogicException('The request scope is no longer active.');
            }
        }
    }

    final readonly class LeakSnapshot
    {
        public function __construct(
            public int $memoryBytes,
            public int $resourceCount,
        ) {
        }
    }

    final class LeakDetector
    {
        private static int $activeScopes = 0;
        private static int $sampledScopes = 0;
        private static int $suspectedLeaks = 0;
        private static int $cleanupFailures = 0;
        private static int $lastMemoryDelta = 0;
        private static int $lastResourceDelta = 0;

        public static function snapshot(): LeakSnapshot
        {
            return new LeakSnapshot(memory_get_usage(false), count(get_resources()));
        }

        public static function scopeOpened(): void
        {
            ++self::$activeScopes;
        }

        public static function scopeClosed(
            string $requestId,
            ?LeakSnapshot $snapshot,
            int $memoryThresholdBytes,
            int $cleanupFailures,
        ): void {
            self::$activeScopes = max(0, self::$activeScopes - 1);
            self::$cleanupFailures += $cleanupFailures;
            if ($snapshot === null) {
                return;
            }
            ++self::$sampledScopes;
            gc_collect_cycles();
            self::$lastMemoryDelta = memory_get_usage(false) - $snapshot->memoryBytes;
            self::$lastResourceDelta = count(get_resources()) - $snapshot->resourceCount;
            if (
                self::$lastMemoryDelta <= $memoryThresholdBytes
                && self::$lastResourceDelta <= 0
                && $cleanupFailures === 0
            ) {
                return;
            }
            ++self::$suspectedLeaks;
            \Pam\Observability\Telemetry::log('warning', 'Potential request resource leak', [
                'requestId' => $requestId,
                'memoryDeltaBytes' => self::$lastMemoryDelta,
                'resourceDelta' => self::$lastResourceDelta,
                'cleanupFailures' => $cleanupFailures,
            ]);
        }

        /** @return array<string, int> */
        public static function metrics(): array
        {
            return [
                'activeScopes' => self::$activeScopes,
                'sampledScopes' => self::$sampledScopes,
                'suspectedLeaks' => self::$suspectedLeaks,
                'cleanupFailures' => self::$cleanupFailures,
                'lastMemoryDeltaBytes' => self::$lastMemoryDelta,
                'lastResourceDelta' => self::$lastResourceDelta,
            ];
        }
    }
}
