<?php

declare(strict_types=1);

namespace Pam\Diagnostics {
    enum EventKind: int
    {
        case RequestStart = 1;
        case RequestEnd = 2;
        case FiberSuspend = 3;
        case FiberResume = 4;
        case IoStart = 5;
        case IoEnd = 6;
        case Cleanup = 7;
        case Error = 8;
    }

    final readonly class Event
    {
        /** @param array<string, mixed> $context */
        public function __construct(
            public EventKind $kind,
            public int $timestampNanoseconds,
            public ?string $requestId,
            public array $context,
        ) {
        }

        /** @return array<string, mixed> */
        public function export(): array
        {
            return [
                'kind' => $this->kind->value,
                'timestampNanoseconds' => $this->timestampNanoseconds,
                'requestId' => $this->requestId,
                'context' => $this->context,
            ];
        }
    }

    final class Channel
    {
        /** @var array<int, callable(Event):void> */
        private static array $subscribers = [];

        /** @var list<Event> */
        private static array $events = [];

        private static int $nextSubscriber = 1;
        private static bool $environmentEnabled = false;

        public static function initialize(): void
        {
            self::$environmentEnabled = getenv('PAM_DIAGNOSTICS') === '1'
                || getenv('PAM_TRACE') === '1'
                || getenv('PAM_PROFILE') === '1';
        }

        /** @param callable(Event):void $subscriber */
        public static function subscribe(callable $subscriber): int
        {
            $id = self::$nextSubscriber++;
            self::$subscribers[$id] = $subscriber;
            return $id;
        }

        public static function unsubscribe(int $subscriberId): void
        {
            unset(self::$subscribers[$subscriberId]);
        }

        /** @param array<string, mixed> $context */
        public static function publish(EventKind $kind, array $context = []): void
        {
            if (!self::$environmentEnabled && self::$subscribers === []) {
                return;
            }
            $requestId = \Pam\Async\FiberContext::get('pam.request_id');
            $event = new Event(
                $kind,
                hrtime(true),
                is_string($requestId) ? $requestId : null,
                $context,
            );
            self::$events[] = $event;
            if (count(self::$events) > 1024) {
                array_shift(self::$events);
            }
            foreach (self::$subscribers as $subscriber) {
                try {
                    $subscriber($event);
                } catch (\Throwable $error) {
                    error_log("pam diagnostics subscriber failed: {$error->getMessage()}");
                }
            }
            if (getenv('PAM_TRACE') === '1') {
                error_log(json_encode(
                    ['event' => 'pam_trace', ...$event->export()],
                    JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES,
                ));
            }
        }

        /** @return list<array<string, mixed>> */
        public static function events(): array
        {
            return array_map(static fn (Event $event): array => $event->export(), self::$events);
        }
    }

    final class Profiler
    {
        /** @var array<string, array{started: int, memory: int}> */
        private static array $active = [];

        /** @var array<string, array{count: int, durationNanoseconds: int, memoryDeltaBytes: int, peakMemoryBytes: int}> */
        private static array $profiles = [];

        public static function begin(string $requestId): void
        {
            if (getenv('PAM_PROFILE') !== '1') {
                return;
            }
            self::$active[$requestId] = [
                'started' => (int) hrtime(true),
                'memory' => memory_get_usage(false),
            ];
        }

        public static function finish(string $requestId): void
        {
            $active = self::$active[$requestId] ?? null;
            unset(self::$active[$requestId]);
            if ($active === null) {
                return;
            }
            $profile = self::$profiles['http.request'] ?? [
                'count' => 0,
                'durationNanoseconds' => 0,
                'memoryDeltaBytes' => 0,
                'peakMemoryBytes' => 0,
            ];
            ++$profile['count'];
            $profile['durationNanoseconds'] += (int) hrtime(true) - $active['started'];
            $profile['memoryDeltaBytes'] += memory_get_usage(false) - $active['memory'];
            $profile['peakMemoryBytes'] = max($profile['peakMemoryBytes'], memory_get_peak_usage(false));
            self::$profiles['http.request'] = $profile;
        }

        /** @return array<string, array<string, int>> */
        public static function profiles(): array
        {
            return self::$profiles;
        }
    }

    final class Diagnostics
    {
        /** @return array<string, mixed> */
        public static function snapshot(): array
        {
            $resourceTypes = [];
            foreach (get_resources() as $resource) {
                $type = get_resource_type($resource);
                $resourceTypes[$type] = ($resourceTypes[$type] ?? 0) + 1;
            }
            ksort($resourceTypes);
            return [
                'memory' => [
                    'usedBytes' => memory_get_usage(false),
                    'allocatedBytes' => memory_get_usage(true),
                    'peakBytes' => memory_get_peak_usage(true),
                    'gc' => gc_status(),
                ],
                'fibers' => [
                    'pending' => \Pam\Async\Scheduler::pendingCount(),
                    'activeRequestScopes' => \Pam\Runtime\LeakDetector::metrics()['activeScopes'],
                ],
                'resources' => $resourceTypes,
                'leaks' => \Pam\Runtime\LeakDetector::metrics(),
                'profiles' => Profiler::profiles(),
                'events' => Channel::events(),
            ];
        }
    }

    Channel::initialize();
}
