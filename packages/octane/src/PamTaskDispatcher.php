<?php

declare(strict_types=1);

namespace Pam\Octane;

use Closure;
use Laravel\Octane\Contracts\DispatchesTasks;
use Laravel\Octane\Exceptions\TaskException;
use Laravel\Octane\Exceptions\TaskExceptionResult;
use Laravel\Octane\Exceptions\TaskTimeoutException;
use Laravel\Octane\Swoole\TaskResult;
use Laravel\SerializableClosure\SerializableClosure;
use Pam\Shared\Tasks;

/**
 * Runs Octane::concurrently() and task dispatch on the PAM cluster's task
 * workers (`--task-workers`), concurrently, like Swoole's task workers.
 */
final class PamTaskDispatcher implements DispatchesTasks
{
    /** @var Closure(list<string>, int): list<string|null> */
    private Closure $resolver;

    /** @var Closure(list<string>): void */
    private Closure $sender;

    /**
     * @param (Closure(list<string>, int): list<string|null>)|null $resolver
     * @param (Closure(list<string>): void)|null $sender
     */
    public function __construct(?Closure $resolver = null, ?Closure $sender = null)
    {
        $this->resolver = $resolver ?? static fn (array $payloads, int $wait): array => Tasks::resolve(array_values($payloads), $wait);
        $this->sender = $sender ?? static function (array $payloads): void {
            Tasks::dispatch(array_values($payloads));
        };
    }

    /**
     * @param array<array-key, mixed> $tasks
     * @return array<array-key, mixed>
     */
    public function resolve(array $tasks, int $waitMilliseconds = 3000): array
    {
        if ($tasks === []) {
            return [];
        }

        try {
            $results = ($this->resolver)(self::payloads($tasks), max(1, $waitMilliseconds));
        } catch (\RuntimeException $error) {
            throw new TaskException($error::class, $error->getMessage(), (int) $error->getCode(), $error->getFile(), $error->getLine());
        }

        $finished = 0;
        $index = 0;
        foreach ($tasks as $key => $task) {
            $raw = $results[$index++] ?? null;
            if ($raw === null) {
                $tasks[$key] = false;
                continue;
            }
            $finished++;
            $result = unserialize($raw);
            if ($result instanceof TaskExceptionResult) {
                throw $result->getOriginal();
            }
            $tasks[$key] = $result instanceof TaskResult ? $result->result : $result;
        }

        if ($finished === 0) {
            throw TaskTimeoutException::after($waitMilliseconds);
        }

        return $tasks;
    }

    /** @param array<array-key, mixed> $tasks */
    public function dispatch(array $tasks): void
    {
        try {
            ($this->sender)(self::payloads($tasks));
        } catch (\RuntimeException $error) {
            throw new TaskException($error::class, $error->getMessage(), (int) $error->getCode(), $error->getFile(), $error->getLine());
        }
    }

    /**
     * @param array<array-key, mixed> $tasks
     * @return list<string>
     */
    private static function payloads(array $tasks): array
    {
        $payloads = [];
        foreach ($tasks as $task) {
            $payloads[] = serialize($task instanceof Closure ? new SerializableClosure($task) : $task);
        }

        return $payloads;
    }
}
