<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Nimbus\Container\Container;
use Nimbus\Jobs\ExponentialBackoff;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Jobs\JobRunResult;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\MemoryJobDispatcher;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ExponentialBackoff::class)]
#[CoversClass(JobWorker::class)]
#[CoversClass(InvokableJobHandler::class)]
final class RetryPolicyTest extends TestCase
{
    public function testDelaysDoubleUpToTheCeiling(): void
    {
        $backoff = new ExponentialBackoff(baseSeconds: 5, maximumSeconds: 60, jitter: false);

        self::assertSame([5, 10, 20, 40, 60, 60], array_map($backoff->delaySeconds(...), [1, 2, 3, 4, 5, 6]));
        self::assertSame(60, $backoff->delaySeconds(500));
    }

    public function testJitterDrawsBetweenHalfAndTheFullDelay(): void
    {
        $bounds = [];
        $backoff = new ExponentialBackoff(10, 3_600, true, static function (int $low, int $high) use (&$bounds): int {
            $bounds[] = [$low, $high];
            return $low;
        });

        self::assertSame(20, $backoff->delaySeconds(3));
        self::assertSame([[20, 40]], $bounds);
        $random = new ExponentialBackoff(10);
        foreach (range(1, 50) as $attempt) {
            $delay = $random->delaySeconds(2);
            self::assertGreaterThanOrEqual(10, $delay);
            self::assertLessThanOrEqual(20, $delay);
        }
    }

    public function testInvalidBoundsAreRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        new ExponentialBackoff(baseSeconds: 10, maximumSeconds: 5);
    }

    public function testTheWorkerUsesThePolicyForEachAttempt(): void
    {
        $queue = new MemoryJobDispatcher();
        $envelope = $queue->dispatch(new ReportJob(1), maximumAttempts: 3);
        $worker = new JobWorker(
            $queue,
            static fn (object $job): JobOutcome => JobOutcome::Retry,
            retryPolicy: new ExponentialBackoff(baseSeconds: 7, jitter: false),
        );

        $start = $envelope->availableAt;
        self::assertSame(JobRunResult::Retried, $worker->runOne($start));
        self::assertSame($start + 7, $envelope->availableAt);
        self::assertSame(JobRunResult::Retried, $worker->runOne($start + 7));
        self::assertSame($start + 21, $envelope->availableAt);
        self::assertSame(JobRunResult::DeadLettered, $worker->runOne($start + 21));
    }

    public function testInvokableJobsRunThroughTheContainer(): void
    {
        $container = new Container();
        $container->instance(\ArrayObject::class, $log = new \ArrayObject());
        $handler = new InvokableJobHandler($container);

        self::assertSame(JobOutcome::Complete, $handler->handle(new InvokableReportJob(3)));
        self::assertSame(JobOutcome::Retry, $handler->handle(new InvokableReportJob(0)));
        self::assertSame([3, 0], $log->getArrayCopy());

        $this->expectException(\LogicException::class);
        $handler->handle(new \stdClass());
    }
}

final readonly class InvokableReportJob
{
    public function __construct(public int $reportId)
    {
    }

    /** @param \ArrayObject<int, int> $log */
    public function handle(\ArrayObject $log): ?JobOutcome
    {
        $log[] = $this->reportId;
        return $this->reportId === 0 ? JobOutcome::Retry : null;
    }
}
