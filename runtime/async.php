<?php

declare(strict_types=1);

namespace Pam\Async {
    final class CancelledException extends \RuntimeException
    {
    }

    enum OperationKind: int
    {
        case Timer = 1;
        case Stream = 2;
        case Dns = 3;
        case FileRead = 4;
        case FileWrite = 5;
        case Process = 6;
        case Signal = 7;
        case ResponseChunk = 8;
    }

    enum FutureState: int
    {
        case Pending = 1;
        case Running = 2;
        case Fulfilled = 3;
        case Rejected = 4;
        case Cancelled = 5;
    }

    final readonly class Suspension
    {
        public function __construct(
            public float $resumeAt,
            public OperationKind $kind = OperationKind::Timer,
            /** @var array<string, mixed> */
            public array $payload = [],
        ) {
        }

        /** @return array<string, mixed> */
        public function export(): array
        {
            return [
                'kind' => $this->kind->value,
                'delayMicros' => is_finite($this->resumeAt)
                    ? max(0, (int) ceil(($this->resumeAt - microtime(true)) * 1_000_000))
                    : PHP_INT_MAX,
                ...$this->payload,
            ];
        }

        /**
         * @param list<int> $readFileDescriptors
         * @param list<int> $writeFileDescriptors
         */
        public static function stream(
            array $readFileDescriptors,
            array $writeFileDescriptors,
            float $resumeAt,
        ): self {
            return new self($resumeAt, OperationKind::Stream, [
                'readFileDescriptors' => $readFileDescriptors,
                'writeFileDescriptors' => $writeFileDescriptors,
            ]);
        }
    }

    final class NativeOperation
    {
        public static function available(): bool
        {
            $fiber = \Fiber::getCurrent();
            $rootFiberId = FiberContext::get('pam.native_root_fiber_id');
            return $fiber !== null
                && is_int($rootFiberId)
                && spl_object_id($fiber) === $rootFiberId;
        }

        /** @param array<string, mixed> $payload */
        public static function execute(
            OperationKind $kind,
            array $payload,
            float $timeout,
        ): mixed {
            if ($timeout <= 0) {
                throw new \InvalidArgumentException('Native operation timeout must be positive.');
            }
            if (!self::available()) {
                throw new \LogicException('Native operations require a Pam HTTP request Fiber.');
            }
            $result = \Fiber::suspend(new Suspension(
                microtime(true) + $timeout,
                $kind,
                $payload,
            ));
            if (is_array($result) && is_string($result['error'] ?? null)) {
                throw new \RuntimeException($result['error']);
            }
            return $result;
        }
    }

    final readonly class Deadline
    {
        private function __construct(public float $timestamp)
        {
        }

        public static function after(float $seconds): self
        {
            if ($seconds < 0) {
                throw new \InvalidArgumentException('Deadline duration cannot be negative.');
            }
            return new self(microtime(true) + $seconds);
        }

        public function remaining(): float
        {
            return max(0.0, $this->timestamp - microtime(true));
        }

        public function isExpired(): bool
        {
            return microtime(true) >= $this->timestamp;
        }

        public function throwIfExpired(): void
        {
            if ($this->isExpired()) {
                throw new \RuntimeException('The operation deadline was exceeded.');
            }
        }
    }

    final class CancellationToken
    {
        private bool $cancelled = false;

        public function __construct(private readonly ?Deadline $deadline = null)
        {
        }

        public function cancel(): void { $this->cancelled = true; }
        public function isCancelled(): bool
        {
            return $this->cancelled || $this->deadline?->isExpired() === true;
        }

        public function throwIfCancelled(): void
        {
            if ($this->cancelled) {
                throw new \RuntimeException('The asynchronous operation was cancelled.');
            }
            if ($this->deadline?->isExpired() === true) {
                throw new \RuntimeException('The asynchronous operation deadline was exceeded.');
            }
        }
    }

    final class FiberContext
    {
        /** @var \WeakMap<object, array<string, mixed>>|null */
        private static ?\WeakMap $fiberValues = null;

        /** @var array<string, mixed> */
        private static array $rootValues = [];

        public static function set(string $key, mixed $value): void
        {
            if ($key === '') {
                throw new \InvalidArgumentException('Context keys cannot be empty.');
            }
            $fiber = \Fiber::getCurrent();
            if ($fiber === null) {
                self::$rootValues[$key] = $value;
                return;
            }
            self::$fiberValues ??= new \WeakMap();
            $values = self::$fiberValues[$fiber] ?? [];
            $values[$key] = $value;
            self::$fiberValues[$fiber] = $values;
        }

        public static function get(string $key, mixed $default = null): mixed
        {
            $fiber = \Fiber::getCurrent();
            $values = $fiber === null ? self::$rootValues : (self::$fiberValues[$fiber] ?? []);
            return $values[$key] ?? $default;
        }

        /** @return array<string, mixed> */
        public static function snapshot(): array
        {
            $fiber = \Fiber::getCurrent();
            return $fiber === null ? self::$rootValues : (self::$fiberValues[$fiber] ?? []);
        }

        /** @param array<string, mixed> $values */
        public static function replace(array $values): void
        {
            $fiber = \Fiber::getCurrent();
            if ($fiber === null) {
                self::$rootValues = $values;
                return;
            }
            self::$fiberValues ??= new \WeakMap();
            self::$fiberValues[$fiber] = $values;
        }

        public static function clear(): void
        {
            $fiber = \Fiber::getCurrent();
            if ($fiber === null) {
                if (self::$rootValues !== []) {
                    self::$rootValues = [];
                }
            } elseif (self::$fiberValues !== null) {
                unset(self::$fiberValues[$fiber]);
            }
        }

        public static function remove(string $key): void
        {
            $fiber = \Fiber::getCurrent();
            if ($fiber === null) {
                unset(self::$rootValues[$key]);
                return;
            }
            if (self::$fiberValues === null) {
                return;
            }
            $values = self::$fiberValues[$fiber] ?? [];
            unset($values[$key]);
            self::$fiberValues[$fiber] = $values;
        }
    }

    final class Future
    {
        /** @var \Fiber<null, mixed, void, Suspension|null> */
        private readonly \Fiber $fiber;
        private FutureState $state = FutureState::Pending;
        private mixed $result = null;
        private ?\Throwable $error = null;
        private float $resumeAt = 0.0;

        public function __construct(callable $operation)
        {
            $context = FiberContext::snapshot();
            $this->fiber = new \Fiber(function () use ($operation, $context): void {
                FiberContext::replace($context);
                try {
                    $this->result = $operation();
                    $this->state = FutureState::Fulfilled;
                } catch (CancelledException $error) {
                    $this->error = $error;
                    $this->state = FutureState::Cancelled;
                } catch (\Throwable $error) {
                    $this->error = $error;
                    $this->state = FutureState::Rejected;
                } finally {
                    FiberContext::clear();
                }
            });
            Scheduler::register($this);
        }

        public function state(): FutureState { return $this->state; }
        public function isComplete(): bool { return $this->state->value >= FutureState::Fulfilled->value; }
        public function resumeAt(): float { return $this->resumeAt; }

        public function advance(float $now): void
        {
            if ($this->isComplete() || $this->resumeAt > $now) {
                return;
            }
            try {
                if ($this->state === FutureState::Pending) {
                    $this->state = FutureState::Running;
                    $suspension = $this->fiber->start();
                } elseif ($this->fiber->isSuspended()) {
                    $suspension = $this->fiber->resume();
                } else {
                    return;
                }
                $this->resumeAt = $suspension instanceof Suspension ? $suspension->resumeAt : $now;
            } catch (\Throwable $error) {
                $this->error = $error;
                $this->state = FutureState::Rejected;
            }
        }

        public function await(?float $timeout = null): mixed
        {
            Scheduler::runUntil($this, $timeout);
            return $this->unwrap();
        }

        public function cancel(): void
        {
            if ($this->isComplete()) {
                Scheduler::unregister($this);
                return;
            }

            $error = new CancelledException('The asynchronous operation was cancelled.');
            if ($this->state === FutureState::Running && $this->fiber->isSuspended()) {
                try {
                    $this->fiber->throw($error);
                } catch (CancelledException) {
                    // The Fiber did not intercept cancellation. The state below still
                    // records the terminal result and the Fiber has been unwound.
                } catch (\Throwable $thrown) {
                    $this->error = $thrown;
                    $this->state = FutureState::Rejected;
                }
            }
            if (!$this->isComplete()) {
                $this->state = FutureState::Cancelled;
                $this->error = $error;
            }
            Scheduler::unregister($this);
        }

        public function unwrap(): mixed
        {
            if (!$this->isComplete()) {
                throw new \LogicException('Cannot unwrap an incomplete future.');
            }
            if ($this->error !== null) {
                throw $this->error;
            }
            return $this->result;
        }
    }

    final class Scheduler
    {
        private const ROOT_SCOPE = 'root';

        /** @var array<string, array<int, Future>> */
        private static array $futures = [];

        private static function scope(): string
        {
            $scope = FiberContext::get('pam.request_id', self::ROOT_SCOPE);
            return is_string($scope) && $scope !== '' ? $scope : self::ROOT_SCOPE;
        }

        public static function register(Future $future): void
        {
            self::$futures[self::scope()][spl_object_id($future)] = $future;
        }

        public static function unregister(Future $future): void
        {
            $scope = self::scope();
            unset(self::$futures[$scope][spl_object_id($future)]);
            if ((self::$futures[$scope] ?? []) === []) {
                unset(self::$futures[$scope]);
            }
        }

        public static function reset(?string $scope = null): void
        {
            $scope ??= self::scope();
            if ((self::$futures[$scope] ?? []) === []) {
                return;
            }
            $futures = self::$futures[$scope];
            unset(self::$futures[$scope]);
            foreach ($futures as $future) {
                if (!$future->isComplete()) {
                    $future->cancel();
                }
            }
        }

        public static function pendingCount(): int
        {
            return array_sum(array_map('count', self::$futures));
        }

        public static function runUntil(Future $target, ?float $timeout): void
        {
            $scope = self::scope();
            $deadline = $timeout === null ? INF : microtime(true) + $timeout;
            while (!$target->isComplete()) {
                $now = microtime(true);
                if ($now >= $deadline) {
                    $target->cancel();
                    throw new \RuntimeException('Asynchronous operation timed out.');
                }
                $next = $deadline;
                $target->advance($now);
                foreach (self::$futures[$scope] ?? [] as $id => $future) {
                    if ($future === $target) {
                        if ($future->isComplete()) {
                            unset(self::$futures[$scope][$id]);
                        }
                        continue;
                    }
                    $future->advance($now);
                    if ($future->isComplete()) {
                        unset(self::$futures[$scope][$id]);
                    } else {
                        $next = min($next, max($now, $future->resumeAt()));
                    }
                }
                if (!$target->isComplete()) {
                    $sleep = max(0.0, min(0.01, $next - microtime(true)));
                    if ($sleep > 0) {
                        delay($sleep);
                    }
                }
            }
            self::unregister($target);
        }
    }

    final class Channel
    {
        /** @var \SplQueue<mixed> */
        private \SplQueue $values;
        private bool $closed = false;

        public function __construct(private readonly int $capacity = 0)
        {
            if ($capacity < 0) {
                throw new \InvalidArgumentException('Channel capacity cannot be negative.');
            }
            $this->values = new \SplQueue();
        }

        public function send(mixed $value): void
        {
            while (!$this->closed && $this->capacity > 0 && $this->values->count() >= $this->capacity) {
                delay(0.001);
            }
            if ($this->closed) {
                throw new \RuntimeException('Cannot send to a closed channel.');
            }
            $this->values->enqueue($value);
        }

        public function receive(): mixed
        {
            while ($this->values->isEmpty()) {
                if ($this->closed) {
                    return null;
                }
                delay(0.001);
            }
            return $this->values->dequeue();
        }

        public function close(): void { $this->closed = true; }
    }

    /**
     * A lexical lifetime for asynchronous work.
     *
     * Every child must finish before the group returns. The first failure or a
     * shared deadline cancels every unfinished sibling and is rethrown to the
     * caller after child Fibers have executed their cleanup blocks.
     */
    final class TaskGroup
    {
        /** @var array<array-key, Future> */
        private array $children = [];
        private bool $joined = false;

        public function __construct(private readonly ?Deadline $deadline = null)
        {
        }

        public function spawn(string|int $key, callable $operation): Future
        {
            if ($this->joined) {
                throw new \LogicException('Cannot add work after a task group has joined.');
            }
            if (array_key_exists($key, $this->children)) {
                throw new \InvalidArgumentException("Task group key {$key} already exists.");
            }

            return $this->children[$key] = async(function () use ($operation): mixed {
                $this->deadline?->throwIfExpired();
                $result = $operation();
                $this->deadline?->throwIfExpired();
                return $result;
            });
        }

        /** @return array<array-key, mixed> */
        public function join(): array
        {
            if ($this->joined) {
                throw new \LogicException('A task group can only be joined once.');
            }
            $this->joined = true;
            try {
                while (true) {
                    $this->deadline?->throwIfExpired();
                    $now = microtime(true);
                    $next = $this->deadline?->timestamp ?? INF;
                    $pending = false;

                    foreach ($this->children as $future) {
                        if (!$future->isComplete()) {
                            $future->advance($now);
                        }
                        if ($future->state() === FutureState::Rejected
                            || $future->state() === FutureState::Cancelled) {
                            $future->unwrap();
                        }
                        if (!$future->isComplete()) {
                            $pending = true;
                            $next = min($next, max($now, $future->resumeAt()));
                        }
                    }

                    if (!$pending) {
                        break;
                    }
                    delay(max(0.0, min(0.01, $next - microtime(true))));
                }
            } catch (\Throwable $error) {
                $this->cancel();
                throw $error;
            }

            $results = [];
            foreach ($this->children as $key => $future) {
                $results[$key] = $future->unwrap();
                Scheduler::unregister($future);
            }

            return $results;
        }

        public function cancel(): void
        {
            foreach ($this->children as $future) {
                $future->cancel();
            }
        }

        public function __destruct()
        {
            if (!$this->joined) {
                $this->cancel();
            }
        }
    }

    final class Mutex
    {
        private bool $locked = false;

        public function synchronized(callable $operation): mixed
        {
            while ($this->locked) {
                delay(0.001);
            }
            $this->locked = true;
            try {
                return $operation();
            } finally {
                $this->locked = false;
            }
        }
    }

    final class SignalWatcher
    {
        private bool $active = true;
        private bool $previousAsyncSignals;
        private bool $wasBlocked = false;
        private int|\Closure $previousHandler = SIG_DFL;

        public function __construct(public readonly int $signal, callable $handler)
        {
            if (
                !function_exists('pcntl_signal')
                || !function_exists('pcntl_async_signals')
                || !function_exists('pcntl_sigprocmask')
            ) {
                throw new \LogicException('The pcntl extension is required for signal watchers.');
            }
            $this->previousAsyncSignals = pcntl_async_signals();
            if (function_exists('pcntl_signal_get_handler')) {
                $previousHandler = pcntl_signal_get_handler($signal);
                $this->previousHandler = is_int($previousHandler)
                    ? $previousHandler
                    : \Closure::fromCallable($previousHandler);
            }
            $previousMask = [];
            if (!pcntl_sigprocmask(SIG_BLOCK, [$signal], $previousMask)) {
                throw new \RuntimeException("Unable to inspect signal {$signal}.");
            }
            $this->wasBlocked = in_array($signal, $previousMask, true);
            pcntl_async_signals(true);
            if (!pcntl_signal($signal, $handler)) {
                if (!$this->wasBlocked) {
                    pcntl_sigprocmask(SIG_UNBLOCK, [$signal]);
                }
                pcntl_async_signals($this->previousAsyncSignals);
                throw new \RuntimeException("Unable to register signal {$signal}.");
            }
            if (!pcntl_sigprocmask(SIG_UNBLOCK, [$signal])) {
                pcntl_signal($signal, $this->previousHandler);
                pcntl_async_signals($this->previousAsyncSignals);
                throw new \RuntimeException("Unable to unblock signal {$signal}.");
            }
        }

        public function cancel(): void
        {
            if (!$this->active) {
                return;
            }
            pcntl_signal($this->signal, $this->previousHandler);
            if ($this->wasBlocked) {
                pcntl_sigprocmask(SIG_BLOCK, [$this->signal]);
            }
            pcntl_async_signals($this->previousAsyncSignals);
            $this->active = false;
        }

        public function __destruct()
        {
            $this->cancel();
        }
    }

    function async(callable $operation): Future
    {
        return new Future($operation);
    }

    /**
     * @param iterable<array-key, mixed> $operations
     * @return array<array-key, mixed>
     */
    function concurrently(iterable $operations, ?Deadline $deadline = null): array
    {
        $group = new TaskGroup($deadline);
        foreach ($operations as $key => $operation) {
            if (!is_callable($operation)) {
                throw new \InvalidArgumentException('Concurrent operations must be callable.');
            }
            $group->spawn($key, $operation);
        }
        return $group->join();
    }

    function await(object $future, ?float $timeout = null): mixed
    {
        if ($future instanceof Future) {
            return $future->await($timeout);
        }
        if (method_exists($future, 'await')) {
            return $future->await();
        }
        throw new \InvalidArgumentException('Await requires a Pam or compatible Composer future.');
    }

    function delay(float $seconds): void
    {
        if ($seconds < 0) {
            throw new \InvalidArgumentException('Delay cannot be negative.');
        }
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            usleep((int) ($seconds * 1_000_000));
            return;
        }
        \Fiber::suspend(new Suspension(microtime(true) + $seconds));
    }

    /**
     * @param list<resource> $read
     * @param list<resource> $write
     */
    function waitForStreams(
        array $read,
        array $write = [],
        ?float $timeout = null,
        ?CancellationToken $cancellation = null,
    ): void {
        if ($read === [] && $write === []) {
            throw new \InvalidArgumentException('At least one stream is required.');
        }
        if ($timeout !== null && $timeout < 0) {
            throw new \InvalidArgumentException('Stream timeout cannot be negative.');
        }
        $deadline = $timeout === null ? INF : microtime(true) + $timeout;
        while (true) {
            $cancellation?->throwIfCancelled();
            $readable = $read;
            $writable = $write;
            $except = [];
            $ready = @stream_select($readable, $writable, $except, 0, 0);
            if ($ready === false) {
                throw new \RuntimeException('Unable to poll streams.');
            }
            if ($ready > 0) {
                return;
            }
            if (microtime(true) >= $deadline) {
                throw new \RuntimeException('Stream operation timed out.');
            }
            $fiber = \Fiber::getCurrent();
            $nativeRootFiberId = FiberContext::get('pam.native_root_fiber_id');
            if (
                $fiber !== null
                && is_int($nativeRootFiberId)
                && spl_object_id($fiber) === $nativeRootFiberId
                && function_exists('pam_native_stream_fd')
            ) {
                $readFileDescriptors = array_values(array_unique(array_filter(array_map(
                    static fn ($stream): int|false => pam_native_stream_fd($stream),
                    $read,
                ), is_int(...))));
                $writeFileDescriptors = array_values(array_unique(array_filter(array_map(
                    static fn ($stream): int|false => pam_native_stream_fd($stream),
                    $write,
                ), is_int(...))));
                if ($readFileDescriptors !== [] || $writeFileDescriptors !== []) {
                    \Fiber::suspend(Suspension::stream(
                        $readFileDescriptors,
                        $writeFileDescriptors,
                        $deadline,
                    ));
                    continue;
                }
            }
            delay(min(0.001, max(0.0, $deadline - microtime(true))));
        }
    }

    /** @param resource $stream */
    function readable($stream, ?float $timeout = null, ?CancellationToken $cancellation = null): void
    {
        waitForStreams([$stream], timeout: $timeout, cancellation: $cancellation);
    }

    /** @param resource $stream */
    function writable($stream, ?float $timeout = null, ?CancellationToken $cancellation = null): void
    {
        waitForStreams([], [$stream], $timeout, $cancellation);
    }

    /** @param resource $stream */
    function read($stream, int $length = 8192, ?float $timeout = null, ?CancellationToken $cancellation = null): string
    {
        if ($length <= 0) {
            throw new \InvalidArgumentException('Read length must be positive.');
        }
        stream_set_blocking($stream, false);
        readable($stream, $timeout, $cancellation);
        $data = fread($stream, $length);
        if ($data === false) {
            throw new \RuntimeException('Unable to read from stream.');
        }
        return $data;
    }

    /** @param resource $stream */
    function write($stream, string $data, ?float $timeout = null, ?CancellationToken $cancellation = null): void
    {
        stream_set_blocking($stream, false);
        $offset = 0;
        $deadline = $timeout === null ? null : microtime(true) + $timeout;
        while ($offset < strlen($data)) {
            $remaining = $deadline === null ? null : max(0.0, $deadline - microtime(true));
            writable($stream, $remaining, $cancellation);
            $written = fwrite($stream, substr($data, $offset));
            if ($written === false) {
                throw new \RuntimeException('Unable to write to stream.');
            }
            $offset += $written;
        }
    }

    /**
     * @param array<string, array<string, mixed>> $contextOptions
     * @return resource
     */
    function connect(
        string $address,
        float $timeout = 10.0,
        bool $tls = false,
        ?CancellationToken $cancellation = null,
        array $contextOptions = [],
    ) {
        if ($address === '' || $timeout <= 0) {
            throw new \InvalidArgumentException('A network address and positive timeout are required.');
        }
        $target = str_contains($address, '://') ? $address : "tcp://{$address}";
        $errorCode = 0;
        $error = '';
        $context = stream_context_create($contextOptions);
        $stream = @stream_socket_client(
            $target,
            $errorCode,
            $error,
            $timeout,
            STREAM_CLIENT_CONNECT | STREAM_CLIENT_ASYNC_CONNECT,
            $context,
        );
        if ($stream === false) {
            throw new \RuntimeException("Unable to connect to {$address}: {$errorCode} {$error}");
        }
        stream_set_blocking($stream, false);
        writable($stream, $timeout, $cancellation);
        if ($tls) {
            $deadline = microtime(true) + $timeout;
            do {
                $enabled = @stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
                if ($enabled === true) {
                    break;
                }
                if ($enabled === false) {
                    fclose($stream);
                    throw new \RuntimeException('TLS negotiation failed.');
                }
                waitForStreams(
                    [$stream],
                    [$stream],
                    max(0.0, $deadline - microtime(true)),
                    $cancellation,
                );
            } while (microtime(true) < $deadline);
            if ($enabled !== true) {
                fclose($stream);
                throw new \RuntimeException('TLS negotiation timed out.');
            }
        }
        return $stream;
    }

    /** @return Future */
    function resolve(string $host, float $timeout = 5.0): Future
    {
        if ($host === '' || $timeout <= 0) {
            throw new \InvalidArgumentException('A hostname and positive timeout are required.');
        }
        return async(static function () use ($host, $timeout): array {
            $php = PHP_BINDIR . DIRECTORY_SEPARATOR . 'php';
            $source = '$addresses = gethostbynamel($argv[1]); echo json_encode($addresses === false ? [] : $addresses, JSON_THROW_ON_ERROR);';
            $result = (new \Pam\Task\ProcessPool())->run([$php, '-r', $source, $host], timeout: $timeout);
            if (!$result->successful()) {
                throw new \RuntimeException("DNS resolution failed: {$result->stderr}");
            }
            $addresses = json_decode($result->stdout, true, 32, JSON_THROW_ON_ERROR);
            if (!is_array($addresses)) {
                throw new \RuntimeException('DNS resolver returned an invalid response.');
            }
            return array_values(array_filter($addresses, is_string(...)));
        });
    }

    function onSignal(int $signal, callable $handler): SignalWatcher
    {
        return new SignalWatcher($signal, $handler);
    }

    /**
     * @param iterable<array-key, Future> $futures
     * @return array<array-key, mixed>
     */
    function all(iterable $futures, ?float $timeout = null): array
    {
        $results = [];
        foreach ($futures as $key => $future) {
            $results[$key] = $future->await($timeout);
        }
        return $results;
    }
}
