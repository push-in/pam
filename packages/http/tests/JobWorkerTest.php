<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\Api\Jobs\JobEnvelope;
use Pam\Api\Jobs\JobOutcome;
use Pam\Api\Jobs\JobRunResult;
use Pam\Api\Jobs\JobState;
use Pam\Api\Jobs\JobWorker;
use Pam\Api\Jobs\MemoryJobDispatcher;
use PHPUnit\Framework\TestCase;

final class JobWorkerTest extends TestCase
{
    public function testWorkerCompletesAndRemovesAJob(): void
    {
        $queue = new MemoryJobDispatcher();
        $envelope = $queue->dispatch(new QueueTestJob(1));
        $worker = new JobWorker($queue, static fn (object $job): JobOutcome => JobOutcome::Complete);

        self::assertSame(JobRunResult::Completed, $worker->runOne($envelope->availableAt));
        self::assertSame(JobState::Completed, $envelope->state);
        self::assertSame(1, $envelope->attempts);
        self::assertSame([], $queue->pending());
        self::assertSame(JobRunResult::Idle, $worker->runOne($envelope->availableAt));
    }

    public function testFailureRetriesThenMovesToDeadLetterWithoutLeakingTheMessage(): void
    {
        $queue = new MemoryJobDispatcher();
        $envelope = $queue->dispatch(new QueueTestJob(2), maximumAttempts: 2);
        $worker = new JobWorker(
            $queue,
            static fn (object $job): never => throw new QueueTestFailure('secret payload'),
            retryDelaySeconds: 5,
        );

        $startedAt = $envelope->availableAt;
        self::assertSame(JobRunResult::Retried, $worker->runOne($startedAt));
        self::assertSame(JobState::Pending, $envelope->state);
        self::assertSame(QueueTestFailure::class, $envelope->lastFailureClass);
        self::assertSame(JobRunResult::Idle, $worker->runOne($startedAt + 4));
        self::assertSame(JobRunResult::DeadLettered, $worker->runOne($startedAt + 5));
        self::assertSame(JobState::DeadLetter, $envelope->state);
        self::assertSame([$envelope], $queue->deadLetters());
    }

    public function testExpiredLeaseCanBeRecoveredByAnotherWorkerAttempt(): void
    {
        $queue = new MemoryJobDispatcher();
        $envelope = $queue->dispatch(new QueueTestJob(3), maximumAttempts: 2);
        $startedAt = $envelope->availableAt;

        self::assertSame($envelope, $queue->reserve($startedAt, leaseSeconds: 10));
        self::assertNull($queue->reserve($startedAt + 9, leaseSeconds: 10));
        self::assertSame($envelope, $queue->reserve($startedAt + 10, leaseSeconds: 10));
        self::assertSame(2, $envelope->attempts);
    }

    public function testQueueRejectsTransitionsForForeignOrUnreservedEnvelopes(): void
    {
        $queue = new MemoryJobDispatcher();
        $foreign = new JobEnvelope('foreign', new QueueTestJob(4));

        $this->expectException(\LogicException::class);
        $queue->complete($foreign);
    }
}

final readonly class QueueTestJob
{
    public function __construct(public int $id)
    {
    }
}

final class QueueTestFailure extends \RuntimeException
{
}
