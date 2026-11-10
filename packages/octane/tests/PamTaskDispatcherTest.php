<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Laravel\Octane\Exceptions\TaskException;
use Laravel\Octane\Exceptions\TaskExceptionResult;
use Laravel\Octane\Exceptions\TaskTimeoutException;
use Laravel\Octane\Swoole\TaskResult;
use Laravel\SerializableClosure\SerializableClosure;
use Pam\Octane\PamTaskDispatcher;
use PHPUnit\Framework\TestCase;

final class PamTaskDispatcherTest extends TestCase
{
    /**
     * A fake task worker pool: runs each payload the way a PAM task worker
     * does and lets a test drop results (still running at the deadline).
     *
     * @param list<int> $unfinished
     */
    private function dispatcher(array $unfinished = [], ?int &$wait = null): PamTaskDispatcher
    {
        return new PamTaskDispatcher(static function (array $payloads, int $waitMilliseconds) use ($unfinished, &$wait): array {
            $wait = $waitMilliseconds;
            $results = [];
            foreach ($payloads as $index => $payload) {
                if (in_array($index, $unfinished, true)) {
                    $results[] = null;
                    continue;
                }
                $task = unserialize($payload);
                self::assertIsCallable($task);
                try {
                    $results[] = serialize(new TaskResult($task()));
                } catch (\Throwable $error) {
                    $results[] = serialize(TaskExceptionResult::from($error));
                }
            }

            return $results;
        });
    }

    public function testResolvesTasksKeepingTheirKeysAndOrder(): void
    {
        $wait = null;
        $results = $this->dispatcher([], $wait)->resolve([
            'first' => fn () => 1 + 1,
            'second' => fn () => 'pam',
            5 => fn () => ['nested' => true],
        ], 1500);

        self::assertSame(['first' => 2, 'second' => 'pam', 5 => ['nested' => true]], $results);
        self::assertSame(1500, $wait);
    }

    public function testUnfinishedTasksResolveToFalse(): void
    {
        $results = $this->dispatcher([1])->resolve(['a' => fn () => 'done', 'b' => fn () => 'late']);

        self::assertSame(['a' => 'done', 'b' => false], $results);
    }

    public function testATimeoutWithoutAnyResultThrows(): void
    {
        $this->expectException(TaskTimeoutException::class);
        $this->expectExceptionMessage('Task timed out after 250 milliseconds.');

        $this->dispatcher([0, 1])->resolve([fn () => 1, fn () => 2], 250);
    }

    public function testTaskExceptionsArePropagated(): void
    {
        $this->expectException(TaskException::class);
        $this->expectExceptionMessage('boom');

        $this->dispatcher()->resolve([fn () => throw new \LogicException('boom')]);
    }

    public function testUnreachableTaskWorkersBecomeTaskExceptions(): void
    {
        $dispatcher = new PamTaskDispatcher(
            static fn (): array => throw new \RuntimeException('cannot reach the task workers'),
            static fn () => throw new \RuntimeException('cannot reach the task workers'),
        );

        try {
            $dispatcher->resolve([fn () => 1]);
            self::fail('resolve() must throw');
        } catch (TaskException $error) {
            self::assertSame(\RuntimeException::class, $error->getClass());
        }

        $this->expectException(TaskException::class);
        $dispatcher->dispatch([fn () => 1]);
    }

    public function testDispatchSendsSerializedClosures(): void
    {
        $sent = [];
        $dispatcher = new PamTaskDispatcher(sender: static function (array $payloads) use (&$sent): void {
            $sent = $payloads;
        });

        // One closure per line: serializable closures are located by line.
        $dispatcher->dispatch([
            fn () => 'one',
            fn () => 'two',
        ]);

        self::assertCount(2, $sent);
        $task = unserialize($sent[1]);
        self::assertInstanceOf(SerializableClosure::class, $task);
        self::assertSame('two', $task());
        self::assertSame([], $dispatcher->resolve([]));
    }
}
