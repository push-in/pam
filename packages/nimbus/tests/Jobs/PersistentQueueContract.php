<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Nimbus\Jobs\ExponentialBackoff;
use Nimbus\Jobs\FailedJobRepository;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobEnvelope;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Jobs\JobQueue;
use Nimbus\Jobs\JobRunResult;
use Nimbus\Jobs\JobState;
use Nimbus\Jobs\JobWorker;

/** Behaviour every persistent queue driver shares, against a fake clock. */
trait PersistentQueueContract
{
    private int $now = 1_800_000_000;

    abstract private function queue(string $name = 'default', ?JobCodec $codec = null): JobQueue&FailedJobRepository;

    private function clock(): \Closure
    {
        return fn (): int => $this->now;
    }

    private static function codec(): JobCodec
    {
        return new JobCodec([ReportJob::class]);
    }

    public function testDispatchedJobsAreReservedOnceAndCompleted(): void
    {
        $queue = $this->queue();
        $dispatched = $queue->dispatch(new ReportJob(7));

        $envelope = $queue->reserve($this->now, 30);
        self::assertNotNull($envelope);
        self::assertSame($dispatched->id, $envelope->id);
        self::assertEquals(new ReportJob(7), $envelope->job);
        self::assertSame(1, $envelope->attempts);
        self::assertSame(JobState::Processing, $envelope->state);
        self::assertSame($this->now + 30, $envelope->leaseUntil);
        self::assertNull($queue->reserve($this->now, 30), 'A reserved job is invisible to other reservations.');

        $queue->complete($envelope);
        self::assertSame(JobState::Completed, $envelope->state);
        self::assertNull($queue->reserve($this->now + 3_600, 30));
    }

    public function testDelayedJobsWaitUntilAvailable(): void
    {
        $queue = $this->queue();
        $queue->dispatch(new ReportJob(1), delaySeconds: 30);

        self::assertNull($queue->reserve($this->now + 29, 30));
        self::assertNotNull($queue->reserve($this->now + 30, 30));
    }

    public function testFailuresBackOffExponentiallyAndEndInTheFailedJobs(): void
    {
        $queue = $this->queue();
        $queue->dispatch(new ReportJob(3), maximumAttempts: 3);
        $worker = new JobWorker(
            $queue,
            static fn (object $job): never => throw new ReportJobFailure('secret'),
            retryPolicy: new ExponentialBackoff(baseSeconds: 10, jitter: false),
        );

        $start = $this->now;
        self::assertSame(JobRunResult::Retried, $worker->runOne($start));
        self::assertSame(JobRunResult::Idle, $worker->runOne($start + 9));
        self::assertSame(JobRunResult::Retried, $worker->runOne($start + 10));
        self::assertSame(JobRunResult::Idle, $worker->runOne($start + 29));
        $this->now = $start + 30;
        self::assertSame(JobRunResult::DeadLettered, $worker->runOne($start + 30));
        self::assertSame(JobRunResult::Idle, $worker->runOne($start + 3_600));

        $failed = $queue->failed();
        self::assertCount(1, $failed);
        self::assertSame(ReportJobFailure::class, $failed[0]->failureClass);
        self::assertSame(3, $failed[0]->attempts);
        self::assertSame($start + 30, $failed[0]->failedAt);
        self::assertSame(self::codec()->encode(new ReportJob(3)), $failed[0]->payload);
        self::assertStringNotContainsString('secret', serialize($failed));
    }

    public function testFailedJobsCanBeRetriedOrForgotten(): void
    {
        $queue = $this->queue();
        $id = $queue->dispatch(new ReportJob(4), maximumAttempts: 1)->id;
        $envelope = $queue->reserve($this->now, 30);
        self::assertNotNull($envelope);
        $queue->deadLetter($envelope);
        self::assertSame($id, $queue->failed()[0]->id);
        self::assertNull($queue->failed()[0]->failureClass);

        self::assertTrue($queue->retryFailed($id));
        self::assertFalse($queue->retryFailed($id));
        self::assertSame([], $queue->failed());
        $retried = $queue->reserve($this->now, 30);
        self::assertNotNull($retried);
        self::assertSame(1, $retried->attempts, 'A retried job gets a fresh attempt budget.');

        $queue->deadLetter($retried);
        self::assertTrue($queue->forgetFailed($id));
        self::assertFalse($queue->forgetFailed($id));
        self::assertSame([], $queue->failed());
    }

    public function testAnExpiredLeaseIsTakenOverAndTheStaleWorkerCannotTouchTheJob(): void
    {
        $first = $this->queue();
        $second = $this->queue();
        $first->dispatch(new ReportJob(5));

        $stale = $first->reserve($this->now, 10);
        self::assertNotNull($stale);
        self::assertNull($second->reserve($this->now + 9, 10));
        $current = $second->reserve($this->now + 10, 10);
        self::assertNotNull($current);
        self::assertSame(2, $current->attempts);

        $first->complete($stale);
        self::assertNull($second->reserve($this->now + 15, 10), 'The new lease still holds the job.');
        $second->release($current, $this->now + 20);
        self::assertNotNull($second->reserve($this->now + 20, 10), 'The stale completion did not delete it.');
    }

    public function testAWorkerThatDiesOnTheLastAttemptLeavesAFailedJob(): void
    {
        $queue = $this->queue();
        $queue->dispatch(new ReportJob(6), maximumAttempts: 1);
        self::assertNotNull($queue->reserve($this->now, 10));

        self::assertNull($queue->reserve($this->now + 10, 10));
        self::assertCount(1, $queue->failed());
    }

    public function testPayloadsThatNoLongerDecodeGoStraightToTheFailedJobs(): void
    {
        $this->queue()->dispatch(new ReportJob(8));
        $queue = $this->queue(codec: new JobCodec([]));

        self::assertNull($queue->reserve($this->now, 10));
        self::assertSame(\UnexpectedValueException::class, $queue->failed()[0]->failureClass);
    }

    public function testQueuesAreIsolated(): void
    {
        $this->queue('mail')->dispatch(new ReportJob(9));

        self::assertNull($this->queue()->reserve($this->now, 10));
        self::assertNotNull($this->queue('mail')->reserve($this->now, 10));
    }

    public function testOnlySerializableJobsAreAccepted(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->queue()->dispatch(new \stdClass());
    }

    public function testEnvelopesNotReservedByTheQueueAreRejected(): void
    {
        $this->expectException(\LogicException::class);
        $this->queue()->complete(new JobEnvelope('foreign', new ReportJob(1)));
    }

    public function testTheWorkerDrainsWithinItsLimits(): void
    {
        $queue = $this->queue();
        foreach (range(1, 3) as $id) {
            $queue->dispatch(new ReportJob($id));
            ++$this->now;
        }
        $handled = [];
        $worker = new JobWorker($queue, static function (object $job) use (&$handled): JobOutcome {
            $handled[] = $job instanceof ReportJob ? $job->reportId : 0;
            return JobOutcome::Complete;
        });

        self::assertSame(2, $worker->work(maximumJobs: 2, clock: $this->clock()));
        self::assertSame(1, $worker->work(clock: $this->clock()));
        self::assertSame(0, $worker->work(clock: $this->clock()));
        self::assertSame([1, 2, 3], $handled);
    }
}
