<?php

declare(strict_types=1);

namespace Nimbus\Tests\Runtime;

use Nimbus\App;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\Runtime\Background;
use Nimbus\Scheduling\MemorySchedulerLock;
use Nimbus\Scheduling\Schedule;
use Nimbus\Scheduling\Scheduler;
use Nimbus\Tests\Jobs\ReportJob;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Background::class)]
#[CoversClass(App::class)]
final class BackgroundTest extends TestCase
{
    private const string TOKEN = '0123456789abcdef0123456789abcdef';

    protected function tearDown(): void
    {
        putenv('PAM_TASK_WORKER');
        putenv('PAM_TASK_TOKEN');
    }

    public function testATickRunsTheSchedulerThenDrainsTheQueue(): void
    {
        $clock = time(...);
        $queue = new MemoryJobDispatcher();
        $schedule = new Schedule($queue);
        $schedule->job(new ReportJob(1))->everyMinute();
        $handled = [];
        $background = (new Background($clock))
            ->scheduler(new Scheduler($schedule, new MemorySchedulerLock($clock), $clock))
            ->queue(new JobWorker($queue, static function (object $job) use (&$handled): JobOutcome {
                $handled[] = $job;
                return JobOutcome::Complete;
            }));

        $background->tick();

        self::assertEquals([new ReportJob(1)], $handled, 'The job scheduled in this tick already ran.');
        self::assertSame([], $queue->pending());
    }

    public function testAFailingTaskDoesNotStopTheOthers(): void
    {
        $failures = [];
        $ran = [];
        $background = (new Background(onFailure: static function (string $task, \Throwable $error) use (&$failures): void {
            $failures[] = $task;
        }))
            ->add('broken', static fn (int $now): never => throw new \RuntimeException('boom'))
            ->add('healthy', static function (int $now) use (&$ran): void {
                $ran[] = $now;
            });

        $background->tick();

        self::assertSame(['broken'], $failures);
        self::assertCount(1, $ran);
        $this->expectException(\InvalidArgumentException::class);
        $background->add('healthy', static fn (): null => null);
    }

    public function testTheStandaloneLoopStopsAfterItsTicks(): void
    {
        $ticks = 0;
        $background = (new Background())->add('count', static function () use (&$ticks): void {
            ++$ticks;
        });

        $background->run(intervalSeconds: 0.001, maximumTicks: 3);
        self::assertSame(3, $ticks);

        $stopping = (new Background());
        $stopping->add('stop', static function () use ($stopping, &$ticks): void {
            ++$ticks;
            $stopping->stop();
        });
        $stopping->run(0.001);
        self::assertSame(4, $ticks);
    }

    public function testTaskWorkerTicksAreAnsweredOnlyWithTheClusterToken(): void
    {
        $ticks = 0;
        $app = new App(discoverPackages: false);
        $app->get('/health', static fn (Request $request, Response $response): Response => $response->json(['ok' => true]));
        $app->background()->add('count', static function () use (&$ticks): void {
            ++$ticks;
        });

        $tick = self::request(['x-pam-task' => ['tick'], 'x-pam-task-token' => [self::TOKEN]]);
        self::assertSame(404, $app->handle($tick, new Response())->export()['status'], 'Outside a task worker the header means nothing.');

        putenv('PAM_TASK_WORKER=1');
        putenv('PAM_TASK_TOKEN=' . self::TOKEN);
        $forged = self::request(['x-pam-task' => ['tick'], 'x-pam-task-token' => ['forged']]);
        self::assertSame(403, $app->handle($forged, new Response())->export()['status']);
        self::assertSame(0, $ticks);
        self::assertSame(204, $app->handle($tick, new Response())->export()['status']);
        self::assertSame(1, $ticks);
        $task = self::request(['x-pam-task' => ['run'], 'x-pam-task-token' => [self::TOKEN]]);
        self::assertSame(404, $app->handle($task, new Response())->export()['status']);
        self::assertSame(200, $app->handle(new Request('GET', '/health', [], [], ''), new Response())->export()['status']);
    }

    /** @param array<string, list<string>> $headers */
    private static function request(array $headers): Request
    {
        return new Request('POST', '/', [], $headers, '');
    }
}
