<?php

declare(strict_types=1);

namespace Nimbus\Tests\Scheduling;

use Arcano\ORM\ORM;
use Nimbus\Container\Container;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\Scheduling\DatabaseSchedulerLock;
use Nimbus\Scheduling\MemorySchedulerLock;
use Nimbus\Scheduling\RedisSchedulerLock;
use Nimbus\Scheduling\Schedule;
use Nimbus\Scheduling\ScheduledTask;
use Nimbus\Scheduling\Scheduler;
use Nimbus\Tests\Jobs\QueueDatabase;
use Nimbus\Tests\Jobs\ReportJob;
use Nimbus\Tests\Redis\SocketConnection;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Schedule::class)]
#[CoversClass(ScheduledTask::class)]
#[CoversClass(Scheduler::class)]
#[CoversClass(MemorySchedulerLock::class)]
#[CoversClass(DatabaseSchedulerLock::class)]
#[CoversClass(RedisSchedulerLock::class)]
final class SchedulerTest extends TestCase
{
    /** 2027-01-15 03:00:00 UTC, a Friday. */
    private const int FRIDAY_3AM = 1_800_000_000 - (1_800_000_000 % 86_400) + 3 * 3_600;

    private int $now = self::FRIDAY_3AM;

    public function testFrequenciesCompileToCronExpressions(): void
    {
        $schedule = new Schedule();
        $task = $schedule->call(static fn (): null => null);

        self::assertSame('* * * * *', $task->expression());
        self::assertSame('*/5 * * * *', $task->everyFiveMinutes()->expression());
        self::assertSame('0,30 * * * *', $task->everyThirtyMinutes()->expression());
        self::assertSame('15 * * * *', $task->hourlyAt(15)->expression());
        self::assertSame('0 3 * * *', $task->dailyAt('03:00')->expression());
        self::assertSame('30 18 * * 5', $task->weeklyOn(5, '18:30')->expression());
        self::assertSame('0 0 1 * *', $task->monthly()->expression());
        self::assertSame('@hourly', $task->cron('@hourly')->expression());
    }

    public function testInvalidExpressionsAndTimesAreRejected(): void
    {
        $task = (new Schedule())->call(static fn (): null => null);

        try {
            $task->cron('61 * * * *');
            self::fail('An invalid cron expression was accepted.');
        } catch (\InvalidArgumentException) {
        }
        $this->expectException(\InvalidArgumentException::class);
        $task->dailyAt('24:00');
    }

    public function testDueTasksFollowTheirTimezone(): void
    {
        $schedule = new Schedule(timezone: 'America/Sao_Paulo');
        $local = $schedule->call(static fn (): null => null)->dailyAt('00:00');
        $utc = $schedule->call(static fn (): null => null)->dailyAt('03:00')->timezone('UTC');
        $at = new \DateTimeImmutable('@' . self::FRIDAY_3AM);

        self::assertTrue($local->isDue($at), 'Midnight in São Paulo is 03:00 UTC.');
        self::assertTrue($utc->isDue($at));
        self::assertFalse($utc->isDue($at->modify('+1 minute')));
        self::assertCount(2, $schedule->dueTasks($at));
    }

    public function testEachMinuteRunsOnceEvenWithManyWorkersAndTicks(): void
    {
        $runs = 0;
        $schedule = new Schedule();
        $schedule->call(static function () use (&$runs): void {
            ++$runs;
        })->everyMinute()->name('count');
        $lock = new MemorySchedulerLock($this->clock());
        $first = new Scheduler($schedule, $lock, $this->clock());
        $second = new Scheduler($schedule, $lock, $this->clock());

        self::assertSame(['count'], $first->runDue());
        self::assertSame([], $first->runDue($this->now + 30), 'A second tick in the same minute does nothing.');
        self::assertSame([], $second->runDue($this->now + 1), 'Another worker loses the claim of the minute.');
        self::assertSame(['count'], $second->runDue($this->now + 60));
        self::assertSame([], $first->runDue($this->now + 61));
        self::assertSame(2, $runs);
    }

    public function testOnlyDueTasksRun(): void
    {
        $schedule = new Schedule();
        $schedule->call(static fn (): null => null)->dailyAt('03:00')->name('report');
        $schedule->call(static fn (): null => null)->hourlyAt(30)->name('half-past');
        $scheduler = new Scheduler($schedule, new MemorySchedulerLock($this->clock()), $this->clock());

        self::assertSame(['report'], $scheduler->runDue($this->now));
        self::assertSame(['half-past'], $scheduler->runDue($this->now + 30 * 60));
        self::assertSame([], $scheduler->runDue($this->now + 31 * 60));
    }

    public function testWithoutOverlappingSkipsWhileAnotherRunHoldsTheLock(): void
    {
        $runs = 0;
        $schedule = new Schedule();
        $schedule->call(static function () use (&$runs): void {
            ++$runs;
        })->name('sync')->withoutOverlapping(10);
        $lock = new MemorySchedulerLock($this->clock());
        $scheduler = new Scheduler($schedule, $lock, $this->clock());
        $key = 'nimbus:schedule:' . sha1('sync') . ':running';

        self::assertTrue($lock->acquire($key, 600), 'Another worker is still running it.');
        self::assertSame([], $scheduler->runDue($this->now));
        $lock->release($key);
        self::assertSame(['sync'], $scheduler->runDue($this->now + 60));
        self::assertTrue($lock->acquire($key, 600), 'The lock is released after the run.');
        self::assertSame(1, $runs);
    }

    public function testScheduledJobsAreDispatchedToTheQueue(): void
    {
        $queue = new MemoryJobDispatcher();
        $container = new Container();
        $container->bind(ReportJob::class, static fn (): ReportJob => new ReportJob(11));
        $schedule = new Schedule($queue, container: $container);
        $byClass = $schedule->job(ReportJob::class, maximumAttempts: 5)->dailyAt('03:00');
        $schedule->job(new ReportJob(12))->dailyAt('03:00')->name('report-12');
        $scheduler = new Scheduler($schedule, new MemorySchedulerLock($this->clock()), $this->clock());

        self::assertSame(['job:reports.send', 'report-12'], $scheduler->runDue());
        self::assertSame('job:reports.send', $byClass->getName());
        $pending = $queue->pending();
        self::assertCount(2, $pending);
        self::assertEquals(new ReportJob(11), $pending[0]->job);
        self::assertSame(5, $pending[0]->maximumAttempts);
        self::assertEquals(new ReportJob(12), $pending[1]->job);
    }

    public function testJobsNeedADispatcher(): void
    {
        $this->expectException(\LogicException::class);
        (new Schedule())->job(ReportJob::class);
    }

    public function testAFailingTaskIsReportedAndTheOthersStillRun(): void
    {
        $failures = [];
        $schedule = new Schedule();
        $schedule->call(static fn (): never => throw new \RuntimeException('boom'))->name('broken');
        $schedule->call(static fn (): null => null)->name('healthy');
        $scheduler = new Scheduler(
            $schedule,
            new MemorySchedulerLock($this->clock()),
            $this->clock(),
            static function (ScheduledTask $task, \Throwable $error) use (&$failures): void {
                $failures[] = [$task->getName(), $error::class];
            },
        );

        self::assertSame(['broken', 'healthy'], $scheduler->runDue());
        self::assertSame([['broken', \RuntimeException::class]], $failures);
    }

    public function testCallbacksAreNamedAfterWhereTheyAreDeclared(): void
    {
        $task = (new Schedule())->call(static fn (): null => null);

        self::assertSame('call:SchedulerTest.php:' . (__LINE__ - 2), $task->getName());
    }

    public function testTheDatabaseLockIsSharedAndExpires(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $database = QueueDatabase::create();
        try {
            $first = new DatabaseSchedulerLock($database->databases, clock: $this->clock());
            $second = new DatabaseSchedulerLock($database->databases, clock: $this->clock());

            self::assertTrue($first->acquire('report', 60));
            self::assertFalse($second->acquire('report', 60));
            $second->release('report');
            self::assertFalse($second->acquire('report', 60), 'Only the holder releases a lock.');
            $first->release('report');
            self::assertTrue($second->acquire('report', 60));
            $this->now += 60;
            self::assertTrue($first->acquire('report', 60), 'An expired lock is taken over.');
        } finally {
            $database->drop();
        }
    }

    public function testTheRedisLockIsSharedAndOwned(): void
    {
        $host = getenv('NIMBUS_TEST_REDIS');
        if (!is_string($host) || $host === '') {
            self::markTestSkipped('Set NIMBUS_TEST_REDIS=host:port to run against Redis.');
        }
        $prefix = 'nimbus:test:' . bin2hex(random_bytes(6)) . ':';
        $first = new RedisSchedulerLock(new SocketConnection($host), $prefix);
        $second = new RedisSchedulerLock(new SocketConnection($host), $prefix);

        self::assertTrue($first->acquire('report', 60));
        self::assertFalse($second->acquire('report', 60));
        $second->release('report');
        self::assertFalse($second->acquire('report', 60));
        $first->release('report');
        self::assertTrue($second->acquire('report', 60));
        $second->release('report');
    }

    private function clock(): \Closure
    {
        return fn (): int => $this->now;
    }
}
