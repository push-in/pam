<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

use Nimbus\Events\CallQueuedListener;

/**
 * Records dispatched jobs instead of queueing them, with assertions for tests. Bind it in
 * place of the `JobDispatcher` (`Testing\Queue::fake()` does).
 */
final class QueueFake implements JobDispatcher
{
    /** @var list<JobEnvelope> */
    private array $pushed = [];

    public function dispatch(object $job, int $maximumAttempts = 3, int $delaySeconds = 0): JobEnvelope
    {
        if ($maximumAttempts < 1 || $delaySeconds < 0) {
            throw new \InvalidArgumentException('Jobs need a positive attempt count and a non-negative delay.');
        }
        $envelope = new JobEnvelope(bin2hex(random_bytes(16)), $job, $maximumAttempts, time() + $delaySeconds);
        $this->pushed[] = $envelope;
        return $envelope;
    }

    /**
     * The job was pushed: at least once, `$times` times when an int is given, or at least
     * once matching the callback, which also receives the envelope (attempts, availability).
     *
     * @template T of object
     * @param class-string<T> $job
     * @param (\Closure(T, JobEnvelope): bool)|int|null $callback
     */
    public function assertPushed(string $job, \Closure|int|null $callback = null): void
    {
        if (is_int($callback)) {
            $this->assertPushedTimes($job, $callback);
            return;
        }
        self::assert($this->pushed($job, $callback) !== [], "The expected [{$job}] job was not pushed.");
    }

    /** @param class-string $job */
    public function assertPushedTimes(string $job, int $times): void
    {
        $count = count($this->pushed($job));
        self::assert($count === $times, "The [{$job}] job was pushed {$count} times instead of {$times}.");
    }

    /**
     * @template T of object
     * @param class-string<T> $job
     * @param (\Closure(T, JobEnvelope): bool)|null $callback
     */
    public function assertNotPushed(string $job, ?\Closure $callback = null): void
    {
        self::assert($this->pushed($job, $callback) === [], "The unexpected [{$job}] job was pushed.");
    }

    public function assertNothingPushed(): void
    {
        self::assert($this->pushed === [], count($this->pushed) . ' unexpected jobs were pushed.');
    }

    /** A `ShouldQueue` listener was queued, optionally for the event class. */
    public function assertListenerQueued(string $listener, ?string $event = null): void
    {
        $this->assertPushed(
            CallQueuedListener::class,
            static fn (CallQueuedListener $job): bool => $job->listener === $listener && ($event === null || $job->event === $event),
        );
    }

    /**
     * @template T of object
     * @param class-string<T> $job
     * @param (\Closure(T, JobEnvelope): bool)|null $callback
     * @return list<T>
     */
    public function pushed(string $job, ?\Closure $callback = null): array
    {
        $matches = [];
        foreach ($this->pushed as $envelope) {
            $pushed = $envelope->job;
            if ($pushed instanceof $job && ($callback === null || $callback($pushed, $envelope) === true)) {
                $matches[] = $pushed;
            }
        }
        return $matches;
    }

    /** @return list<JobEnvelope> */
    public function envelopes(): array
    {
        return $this->pushed;
    }

    private static function assert(bool $condition, string $message): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \LogicException($message);
        }
    }
}
