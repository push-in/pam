<?php

declare(strict_types=1);

namespace Pam\Cluster {
    enum NodeState: int
    {
        case Ready = 1;
        case Draining = 2;
        case Offline = 3;
    }

    enum CircuitState: int
    {
        case Closed = 1;
        case Open = 2;
        case HalfOpen = 3;
    }

    final readonly class Node
    {
        /** @param array<string, scalar|null> $metadata */
        public function __construct(
            public string $id,
            public string $endpoint,
            public NodeState $state,
            public array $metadata,
            public float $expiresAt,
        ) {
        }
    }

    final readonly class Lease
    {
        public function __construct(
            public string $name,
            public string $token,
            public int $fencingToken,
            public float $expiresAt,
        ) {
        }
    }

    final readonly class RateDecision
    {
        public function __construct(
            public bool $allowed,
            public int $remaining,
            public float $resetsAt,
        ) {
        }
    }

    final readonly class CircuitDecision
    {
        public function __construct(
            public bool $allowed,
            public CircuitState $state,
        ) {
        }
    }

    interface Coordinator
    {
        /** @param array<string, scalar|null> $metadata */
        public function heartbeat(
            string $nodeId,
            string $endpoint,
            NodeState $state = NodeState::Ready,
            array $metadata = [],
            float $ttlSeconds = 15.0,
        ): Node;

        /** @return list<Node> */
        public function discover(): array;

        public function acquire(string $name, float $ttlSeconds = 30.0): ?Lease;

        public function renew(Lease $lease, float $ttlSeconds = 30.0): ?Lease;

        public function release(Lease $lease): bool;

        public function singleton(string $name, int $intervalSeconds, ?int $epochSeconds = null): ?Lease;

        public function rateLimit(
            string $key,
            int $limit,
            float $windowSeconds = 1.0,
        ): RateDecision;

        public function circuitPermit(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitDecision;

        public function circuitSuccess(string $name): void;

        public function circuitFailure(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitState;

        public function enqueue(string $queue, string $payload, int $capacity = 10_000): int;

        public function dequeue(string $queue): ?string;

        public function presenceJoin(string $channel, string $member, float $ttlSeconds = 30.0): void;

        public function presenceLeave(string $channel, string $member): void;

        /** @return list<string> */
        public function presenceMembers(string $channel): array;
    }

    final class RedisCoordinator implements Coordinator
    {
        private readonly \Redis $redis;

        public function __construct(
            string $host = '127.0.0.1',
            int $port = 6379,
            private readonly string $prefix = 'pam:cluster:',
            float $timeout = 1.0,
            ?string $username = null,
            ?string $password = null,
            int $database = 0,
            bool $tls = false,
            ?string $caFile = null,
            ?string $certificateFile = null,
            ?string $privateKeyFile = null,
        ) {
            if (!class_exists(\Redis::class)) {
                throw new \LogicException('The redis PHP extension is required for RedisCoordinator.');
            }
            Validation::endpoint($host, $port, $timeout);
            Validation::key($prefix, 'cluster prefix');
            if ($database < 0) {
                throw new \InvalidArgumentException('Redis database must be zero or positive.');
            }
            if (($certificateFile === null) !== ($privateKeyFile === null)) {
                throw new \InvalidArgumentException('Redis mTLS requires both certificate and private key.');
            }

            $stream = [
                'verify_peer' => true,
                'verify_peer_name' => true,
                'peer_name' => $host,
            ];
            if ($caFile !== null) {
                $stream['cafile'] = $caFile;
            }
            if ($certificateFile !== null && $privateKeyFile !== null) {
                $stream['local_cert'] = $certificateFile;
                $stream['local_pk'] = $privateKeyFile;
            }
            $redis = new \Redis();
            $connected = $redis->connect(
                ($tls ? 'tls://' : '') . $host,
                $port,
                $timeout,
                null,
                0,
                $timeout,
                ['stream' => $stream],
            );
            if (!$connected) {
                throw new \RuntimeException('Unable to connect to the PAM cluster Redis backend.');
            }
            if ($password !== null) {
                $credentials = $username === null ? $password : [$username, $password];
                if (!$redis->auth($credentials)) {
                    $redis->close();
                    throw new \RuntimeException('Redis cluster authentication failed.');
                }
            }
            if ($database !== 0 && !$redis->select($database)) {
                $redis->close();
                throw new \RuntimeException('Unable to select the Redis cluster database.');
            }
            $this->redis = $redis;
        }

        public function heartbeat(
            string $nodeId,
            string $endpoint,
            NodeState $state = NodeState::Ready,
            array $metadata = [],
            float $ttlSeconds = 15.0,
        ): Node {
            Validation::identifier($nodeId, 'node');
            Validation::networkEndpoint($endpoint);
            Validation::metadata($metadata);
            Validation::duration($ttlSeconds, 'node TTL');
            $expiresAt = microtime(true) + $ttlSeconds;
            $payload = self::encode([
                'id' => $nodeId,
                'endpoint' => $endpoint,
                'state' => $state->value,
                'metadata' => $metadata,
                'expiresAt' => $expiresAt,
            ]);
            $this->redis->eval(
                "redis.call('ZADD', KEYS[1], ARGV[1], ARGV[2]); "
                . "redis.call('HSET', KEYS[2], ARGV[2], ARGV[3]); return 1",
                [$this->key('nodes:expiry'), $this->key('nodes:data'), $expiresAt, $nodeId, $payload],
                2,
            );
            return new Node($nodeId, $endpoint, $state, $metadata, $expiresAt);
        }

        public function discover(): array
        {
            $now = microtime(true);
            $expiryKey = $this->key('nodes:expiry');
            $dataKey = $this->key('nodes:data');
            $ids = $this->redis->eval(
                "local expired = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', ARGV[1]); "
                . 'for _, id in ipairs(expired) do '
                . "redis.call('ZREM', KEYS[1], id); redis.call('HDEL', KEYS[2], id); end; "
                . "return redis.call('ZRANGEBYSCORE', KEYS[1], '(' .. ARGV[1], '+inf')",
                [$expiryKey, $dataKey, $now],
                2,
            );
            if (!is_array($ids)) {
                throw new \RuntimeException('Redis node discovery returned an invalid response.');
            }
            $nodes = [];
            foreach ($ids as $id) {
                if (!is_string($id)) {
                    continue;
                }
                $payload = $this->redis->hGet($dataKey, $id);
                if (!is_string($payload)) {
                    continue;
                }
                $nodes[] = self::node($payload);
            }
            usort($nodes, static fn (Node $left, Node $right): int => $left->id <=> $right->id);
            return $nodes;
        }

        public function acquire(string $name, float $ttlSeconds = 30.0): ?Lease
        {
            Validation::identifier($name, 'lock');
            Validation::duration($ttlSeconds, 'lock TTL');
            $token = bin2hex(random_bytes(24));
            $ttl = self::milliseconds($ttlSeconds);
            $fencing = $this->redis->eval(
                "if redis.call('EXISTS', KEYS[1]) == 0 then "
                . "local fence = redis.call('INCR', KEYS[2]); "
                . "redis.call('PSETEX', KEYS[1], ARGV[1], ARGV[2] .. ':' .. fence); "
                . 'return fence end return 0',
                [$this->key("locks:{$name}"), $this->key("fences:{$name}"), $ttl, $token],
                2,
            );
            if (!is_int($fencing) || $fencing < 1) {
                return null;
            }
            return new Lease($name, $token, $fencing, microtime(true) + $ttlSeconds);
        }

        public function renew(Lease $lease, float $ttlSeconds = 30.0): ?Lease
        {
            Validation::duration($ttlSeconds, 'lock TTL');
            $ttl = self::milliseconds($ttlSeconds);
            $renewed = $this->redis->eval(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then "
                . "return redis.call('PEXPIRE', KEYS[1], ARGV[2]) end return 0",
                [
                    $this->key("locks:{$lease->name}"),
                    self::leaseValue($lease),
                    $ttl,
                ],
                1,
            );
            if ($renewed !== 1) {
                return null;
            }
            return new Lease(
                $lease->name,
                $lease->token,
                $lease->fencingToken,
                microtime(true) + $ttlSeconds,
            );
        }

        public function release(Lease $lease): bool
        {
            $released = $this->redis->eval(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then "
                . "return redis.call('DEL', KEYS[1]) end return 0",
                [$this->key("locks:{$lease->name}"), self::leaseValue($lease)],
                1,
            );
            return $released === 1;
        }

        public function singleton(string $name, int $intervalSeconds, ?int $epochSeconds = null): ?Lease
        {
            if ($intervalSeconds < 1) {
                throw new \InvalidArgumentException('Singleton interval must be positive.');
            }
            $epoch = $epochSeconds ?? time();
            $bucket = intdiv($epoch, $intervalSeconds);
            return $this->acquire("cron.{$name}.{$bucket}", $intervalSeconds * 2.0);
        }

        public function rateLimit(
            string $key,
            int $limit,
            float $windowSeconds = 1.0,
        ): RateDecision {
            Validation::identifier($key, 'rate limit');
            if ($limit < 1) {
                throw new \InvalidArgumentException('Rate limit must be positive.');
            }
            Validation::duration($windowSeconds, 'rate window');
            $now = microtime(true);
            $bucket = (int) floor($now / $windowSeconds);
            $count = $this->redis->eval(
                "local value = redis.call('INCR', KEYS[1]); "
                . "if value == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end; return value",
                [
                    $this->key("rates:{$key}:{$bucket}"),
                    self::milliseconds($windowSeconds * 2),
                ],
                1,
            );
            if (!is_int($count)) {
                throw new \RuntimeException('Redis rate limiter returned an invalid response.');
            }
            return new RateDecision(
                $count <= $limit,
                max(0, $limit - $count),
                ($bucket + 1) * $windowSeconds,
            );
        }

        public function circuitPermit(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitDecision {
            Validation::circuit($name, $failureThreshold, $openSeconds);
            $result = $this->redis->eval(
                "local state = tonumber(redis.call('HGET', KEYS[1], 'state') or '1'); "
                . "local until_at = tonumber(redis.call('HGET', KEYS[1], 'until') or '0'); "
                . "if state == 2 and tonumber(ARGV[1]) < until_at then return {0, 2} end; "
                . "if state == 2 then redis.call('HSET', KEYS[1], 'state', 3, 'probe', 1); return {1, 3} end; "
                . "if state == 3 then return {0, 3} end; return {1, 1}",
                [$this->key("circuits:{$name}"), microtime(true)],
                1,
            );
            [$allowed, $state] = self::integerPair($result, 'circuit permit');
            return new CircuitDecision($allowed === 1, CircuitState::from($state));
        }

        public function circuitSuccess(string $name): void
        {
            Validation::identifier($name, 'circuit');
            $this->redis->hMSet($this->key("circuits:{$name}"), [
                'state' => CircuitState::Closed->value,
                'failures' => 0,
                'until' => 0,
                'probe' => 0,
            ]);
        }

        public function circuitFailure(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitState {
            Validation::circuit($name, $failureThreshold, $openSeconds);
            $result = $this->redis->eval(
                "local state = tonumber(redis.call('HGET', KEYS[1], 'state') or '1'); "
                . "local failures = tonumber(redis.call('HGET', KEYS[1], 'failures') or '0') + 1; "
                . "if state == 3 or failures >= tonumber(ARGV[1]) then "
                . "redis.call('HSET', KEYS[1], 'state', 2, 'failures', failures, "
                . "'until', tonumber(ARGV[2]) + tonumber(ARGV[3]), 'probe', 0); return 2 end; "
                . "redis.call('HSET', KEYS[1], 'state', 1, 'failures', failures); return 1",
                [
                    $this->key("circuits:{$name}"),
                    $failureThreshold,
                    microtime(true),
                    $openSeconds,
                ],
                1,
            );
            if (!is_int($result)) {
                throw new \RuntimeException('Redis circuit breaker returned an invalid response.');
            }
            return CircuitState::from($result);
        }

        public function enqueue(string $queue, string $payload, int $capacity = 10_000): int
        {
            Validation::queue($queue, $payload, $capacity);
            $length = $this->redis->eval(
                "local length = redis.call('LPUSH', KEYS[1], ARGV[1]); "
                . "redis.call('LTRIM', KEYS[1], 0, tonumber(ARGV[2]) - 1); "
                . 'if length > tonumber(ARGV[2]) then return tonumber(ARGV[2]) end; return length',
                [$this->key("queues:{$queue}"), $payload, $capacity],
                1,
            );
            if (!is_int($length)) {
                throw new \RuntimeException('Redis queue returned an invalid response.');
            }
            return $length;
        }

        public function dequeue(string $queue): ?string
        {
            Validation::identifier($queue, 'queue');
            $payload = $this->redis->rPop($this->key("queues:{$queue}"));
            if ($payload === false) {
                return null;
            }
            if (!is_string($payload)) {
                throw new \RuntimeException('Redis queue payload is invalid.');
            }
            return $payload;
        }

        public function presenceJoin(string $channel, string $member, float $ttlSeconds = 30.0): void
        {
            Validation::identifier($channel, 'presence channel');
            Validation::identifier($member, 'presence member');
            Validation::duration($ttlSeconds, 'presence TTL');
            $this->redis->zAdd(
                $this->key("presence:{$channel}"),
                microtime(true) + $ttlSeconds,
                $member,
            );
        }

        public function presenceLeave(string $channel, string $member): void
        {
            Validation::identifier($channel, 'presence channel');
            Validation::identifier($member, 'presence member');
            $this->redis->zRem($this->key("presence:{$channel}"), $member);
        }

        public function presenceMembers(string $channel): array
        {
            Validation::identifier($channel, 'presence channel');
            $key = $this->key("presence:{$channel}");
            $now = microtime(true);
            $this->redis->zRemRangeByScore($key, '-inf', (string) $now);
            $members = $this->redis->zRangeByScore($key, (string) $now, '+inf');
            if (!is_array($members)) {
                throw new \RuntimeException('Redis presence returned an invalid response.');
            }
            $members = array_values(array_filter($members, 'is_string'));
            sort($members);
            return $members;
        }

        public function __destruct()
        {
            try {
                $this->redis->close();
            } catch (\Throwable) {
            }
        }

        private function key(string $suffix): string
        {
            return $this->prefix . $suffix;
        }

        private static function leaseValue(Lease $lease): string
        {
            return "{$lease->token}:{$lease->fencingToken}";
        }

        private static function milliseconds(float $seconds): int
        {
            return max(1, (int) ceil($seconds * 1000));
        }

        /** @return array{int, int} */
        private static function integerPair(mixed $result, string $operation): array
        {
            if (
                !is_array($result)
                || count($result) !== 2
                || !is_int($result[0] ?? null)
                || !is_int($result[1] ?? null)
            ) {
                throw new \RuntimeException("Redis {$operation} returned an invalid response.");
            }
            return [$result[0], $result[1]];
        }

        /** @param array<string, mixed> $value */
        private static function encode(array $value): string
        {
            return json_encode(
                $value,
                JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
            );
        }

        private static function node(string $payload): Node
        {
            $value = json_decode($payload, true, 32, JSON_THROW_ON_ERROR);
            if (
                !is_array($value)
                || !is_string($value['id'] ?? null)
                || !is_string($value['endpoint'] ?? null)
                || !is_int($value['state'] ?? null)
                || !is_array($value['metadata'] ?? null)
                || !(is_int($value['expiresAt'] ?? null) || is_float($value['expiresAt'] ?? null))
            ) {
                throw new \UnexpectedValueException('Cluster node payload is invalid.');
            }
            $metadata = [];
            foreach ($value['metadata'] as $key => $item) {
                if (!is_string($key) || !(is_scalar($item) || $item === null)) {
                    throw new \UnexpectedValueException('Cluster node metadata is invalid.');
                }
                $metadata[$key] = $item;
            }
            return new Node(
                $value['id'],
                $value['endpoint'],
                NodeState::from($value['state']),
                $metadata,
                (float) $value['expiresAt'],
            );
        }
    }

    final class InMemoryCoordinator implements Coordinator
    {
        /** @var array<string, Node> */
        private array $nodes = [];
        /** @var array<string, Lease> */
        private array $leases = [];
        /** @var array<string, int> */
        private array $fences = [];
        /** @var array<string, array{bucket: int, count: int}> */
        private array $rates = [];
        /** @var array<string, array{state: CircuitState, failures: int, until: float, probe: bool}> */
        private array $circuits = [];
        /** @var array<string, list<string>> */
        private array $queues = [];
        /** @var array<string, array<string, float>> */
        private array $presence = [];
        private readonly \Closure $clock;

        public function __construct(?callable $clock = null)
        {
            $this->clock = $clock === null
                ? static fn (): float => microtime(true)
                : \Closure::fromCallable($clock);
        }

        public function heartbeat(
            string $nodeId,
            string $endpoint,
            NodeState $state = NodeState::Ready,
            array $metadata = [],
            float $ttlSeconds = 15.0,
        ): Node {
            Validation::identifier($nodeId, 'node');
            Validation::networkEndpoint($endpoint);
            Validation::metadata($metadata);
            Validation::duration($ttlSeconds, 'node TTL');
            return $this->nodes[$nodeId] = new Node(
                $nodeId,
                $endpoint,
                $state,
                $metadata,
                $this->now() + $ttlSeconds,
            );
        }

        public function discover(): array
        {
            $now = $this->now();
            $this->nodes = array_filter(
                $this->nodes,
                static fn (Node $node): bool => $node->expiresAt > $now,
            );
            $nodes = array_values($this->nodes);
            usort($nodes, static fn (Node $left, Node $right): int => $left->id <=> $right->id);
            return $nodes;
        }

        public function acquire(string $name, float $ttlSeconds = 30.0): ?Lease
        {
            Validation::identifier($name, 'lock');
            Validation::duration($ttlSeconds, 'lock TTL');
            $now = $this->now();
            $current = $this->leases[$name] ?? null;
            if ($current !== null && $current->expiresAt > $now) {
                return null;
            }
            $fencing = ($this->fences[$name] ?? 0) + 1;
            $this->fences[$name] = $fencing;
            return $this->leases[$name] = new Lease(
                $name,
                bin2hex(random_bytes(24)),
                $fencing,
                $now + $ttlSeconds,
            );
        }

        public function renew(Lease $lease, float $ttlSeconds = 30.0): ?Lease
        {
            Validation::duration($ttlSeconds, 'lock TTL');
            $current = $this->leases[$lease->name] ?? null;
            if (
                $current === null
                || $current->token !== $lease->token
                || $current->fencingToken !== $lease->fencingToken
                || $current->expiresAt <= $this->now()
            ) {
                return null;
            }
            return $this->leases[$lease->name] = new Lease(
                $lease->name,
                $lease->token,
                $lease->fencingToken,
                $this->now() + $ttlSeconds,
            );
        }

        public function release(Lease $lease): bool
        {
            $current = $this->leases[$lease->name] ?? null;
            if (
                $current === null
                || $current->token !== $lease->token
                || $current->fencingToken !== $lease->fencingToken
            ) {
                return false;
            }
            unset($this->leases[$lease->name]);
            return true;
        }

        public function singleton(string $name, int $intervalSeconds, ?int $epochSeconds = null): ?Lease
        {
            if ($intervalSeconds < 1) {
                throw new \InvalidArgumentException('Singleton interval must be positive.');
            }
            $epoch = $epochSeconds ?? (int) floor($this->now());
            return $this->acquire(
                "cron.{$name}." . intdiv($epoch, $intervalSeconds),
                $intervalSeconds * 2.0,
            );
        }

        public function rateLimit(
            string $key,
            int $limit,
            float $windowSeconds = 1.0,
        ): RateDecision {
            Validation::identifier($key, 'rate limit');
            if ($limit < 1) {
                throw new \InvalidArgumentException('Rate limit must be positive.');
            }
            Validation::duration($windowSeconds, 'rate window');
            $now = $this->now();
            $bucket = (int) floor($now / $windowSeconds);
            $current = $this->rates[$key] ?? ['bucket' => $bucket, 'count' => 0];
            if ($current['bucket'] !== $bucket) {
                $current = ['bucket' => $bucket, 'count' => 0];
            }
            ++$current['count'];
            $this->rates[$key] = $current;
            return new RateDecision(
                $current['count'] <= $limit,
                max(0, $limit - $current['count']),
                ($bucket + 1) * $windowSeconds,
            );
        }

        public function circuitPermit(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitDecision {
            Validation::circuit($name, $failureThreshold, $openSeconds);
            $current = $this->circuits[$name] ?? self::closedCircuit();
            if ($current['state'] === CircuitState::Open) {
                if ($this->now() < $current['until']) {
                    return new CircuitDecision(false, CircuitState::Open);
                }
                $current['state'] = CircuitState::HalfOpen;
                $current['probe'] = true;
                $this->circuits[$name] = $current;
                return new CircuitDecision(true, CircuitState::HalfOpen);
            }
            if ($current['state'] === CircuitState::HalfOpen && $current['probe']) {
                return new CircuitDecision(false, CircuitState::HalfOpen);
            }
            return new CircuitDecision(true, $current['state']);
        }

        public function circuitSuccess(string $name): void
        {
            Validation::identifier($name, 'circuit');
            $this->circuits[$name] = self::closedCircuit();
        }

        public function circuitFailure(
            string $name,
            int $failureThreshold = 5,
            float $openSeconds = 30.0,
        ): CircuitState {
            Validation::circuit($name, $failureThreshold, $openSeconds);
            $current = $this->circuits[$name] ?? self::closedCircuit();
            ++$current['failures'];
            if (
                $current['state'] === CircuitState::HalfOpen
                || $current['failures'] >= $failureThreshold
            ) {
                $current['state'] = CircuitState::Open;
                $current['until'] = $this->now() + $openSeconds;
                $current['probe'] = false;
            }
            $this->circuits[$name] = $current;
            return $current['state'];
        }

        public function enqueue(string $queue, string $payload, int $capacity = 10_000): int
        {
            Validation::queue($queue, $payload, $capacity);
            $messages = $this->queues[$queue] ?? [];
            $messages[] = $payload;
            if (count($messages) > $capacity) {
                array_shift($messages);
            }
            $this->queues[$queue] = $messages;
            return count($messages);
        }

        public function dequeue(string $queue): ?string
        {
            Validation::identifier($queue, 'queue');
            $messages = $this->queues[$queue] ?? [];
            $payload = array_shift($messages);
            $this->queues[$queue] = $messages;
            return $payload;
        }

        public function presenceJoin(string $channel, string $member, float $ttlSeconds = 30.0): void
        {
            Validation::identifier($channel, 'presence channel');
            Validation::identifier($member, 'presence member');
            Validation::duration($ttlSeconds, 'presence TTL');
            $this->presence[$channel][$member] = $this->now() + $ttlSeconds;
        }

        public function presenceLeave(string $channel, string $member): void
        {
            Validation::identifier($channel, 'presence channel');
            Validation::identifier($member, 'presence member');
            unset($this->presence[$channel][$member]);
        }

        public function presenceMembers(string $channel): array
        {
            Validation::identifier($channel, 'presence channel');
            $now = $this->now();
            $members = array_filter(
                $this->presence[$channel] ?? [],
                static fn (float $expiresAt): bool => $expiresAt > $now,
            );
            $this->presence[$channel] = $members;
            $names = array_keys($members);
            sort($names);
            return $names;
        }

        private function now(): float
        {
            $value = ($this->clock)();
            if (!is_float($value) && !is_int($value)) {
                throw new \UnexpectedValueException('Cluster clock must return seconds.');
            }
            return (float) $value;
        }

        /** @return array{state: CircuitState, failures: int, until: float, probe: bool} */
        private static function closedCircuit(): array
        {
            return [
                'state' => CircuitState::Closed,
                'failures' => 0,
                'until' => 0.0,
                'probe' => false,
            ];
        }
    }

    final class Validation
    {
        public static function identifier(string $value, string $label): void
        {
            if (preg_match('/^[A-Za-z0-9][A-Za-z0-9._:-]{0,191}$/D', $value) !== 1) {
                throw new \InvalidArgumentException("Invalid {$label} identifier.");
            }
        }

        public static function key(string $value, string $label): void
        {
            if ($value === '' || strlen($value) > 192 || str_contains($value, "\0")) {
                throw new \InvalidArgumentException("Invalid {$label}.");
            }
        }

        public static function endpoint(string $host, int $port, float $timeout): void
        {
            if (
                $host === ''
                || strlen($host) > 253
                || str_contains($host, "\0")
                || str_contains($host, '://')
                || preg_match('/\s/', $host) === 1
                || $port < 1
                || $port > 65535
                || $timeout <= 0
            ) {
                throw new \InvalidArgumentException('Redis host, port, and timeout are invalid.');
            }
        }

        public static function networkEndpoint(string $endpoint): void
        {
            $parts = parse_url($endpoint);
            if (
                $parts === false
                || !in_array($parts['scheme'] ?? null, ['http', 'https'], true)
                || !is_string($parts['host'] ?? null)
            ) {
                throw new \InvalidArgumentException('Cluster endpoint must be an absolute HTTP URL.');
            }
        }

        /** @param array<string, mixed> $metadata */
        public static function metadata(array $metadata): void
        {
            if (count($metadata) > 64) {
                throw new \LengthException('Cluster metadata cannot exceed 64 fields.');
            }
            foreach ($metadata as $key => $value) {
                if (
                    preg_match('/^[A-Za-z][A-Za-z0-9_.-]{0,63}$/D', $key) !== 1
                    || !(is_scalar($value) || $value === null)
                ) {
                    throw new \InvalidArgumentException('Cluster metadata is invalid.');
                }
            }
            $encoded = json_encode($metadata, JSON_THROW_ON_ERROR);
            if (strlen($encoded) > 16 * 1024) {
                throw new \LengthException('Cluster metadata exceeds 16 KiB.');
            }
        }

        public static function duration(float $seconds, string $label): void
        {
            if (!is_finite($seconds) || $seconds <= 0 || $seconds > 86_400) {
                throw new \InvalidArgumentException("{$label} must be greater than zero and at most one day.");
            }
        }

        public static function circuit(
            string $name,
            int $failureThreshold,
            float $openSeconds,
        ): void {
            self::identifier($name, 'circuit');
            if ($failureThreshold < 1 || $failureThreshold > 10_000) {
                throw new \InvalidArgumentException('Circuit threshold is invalid.');
            }
            self::duration($openSeconds, 'circuit open duration');
        }

        public static function queue(string $queue, string $payload, int $capacity): void
        {
            self::identifier($queue, 'queue');
            if ($capacity < 1 || $capacity > 1_000_000) {
                throw new \InvalidArgumentException('Queue capacity is invalid.');
            }
            if (strlen($payload) > 8 * 1024 * 1024) {
                throw new \LengthException('Queue payload exceeds 8 MiB.');
            }
        }
    }
}
