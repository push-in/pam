<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Nimbus\Container\Container;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\SerializableJob;

/**
 * The application's recurring tasks:
 *
 *     $schedule->job(SendReport::class)->dailyAt('03:00');
 *     $schedule->call(fn () => $cache->prune())->everyFiveMinutes()->withoutOverlapping();
 *
 * Jobs are only dispatched to the queue when due, so a slow job never holds
 * up the tick that runs the scheduler.
 */
final class Schedule
{
    /** @var list<ScheduledTask> */
    private array $tasks = [];

    private readonly ?\DateTimeZone $timezone;

    public function __construct(
        private readonly ?JobDispatcher $dispatcher = null,
        string|\DateTimeZone|null $timezone = null,
        private readonly ?Container $container = null,
    ) {
        ScheduledTask::assertCronAvailable();
        $this->timezone = is_string($timezone) ? new \DateTimeZone($timezone) : $timezone;
    }

    /**
     * Dispatches a job when due. A class name is built for each run, through the
     * container when the schedule has one.
     *
     * @param object|class-string $job
     */
    public function job(object|string $job, int $maximumAttempts = 3): ScheduledTask
    {
        $dispatcher = $this->dispatcher
            ?? throw new \LogicException('Scheduling jobs requires a JobDispatcher on the Schedule.');
        if (is_string($job) && !class_exists($job)) {
            throw new \InvalidArgumentException("Job class {$job} does not exist.");
        }
        $class = is_string($job) ? $job : $job::class;
        $name = is_subclass_of($class, SerializableJob::class) ? 'job:' . $class::jobName() : 'job:' . $class;
        $container = $this->container;
        return $this->add(new ScheduledTask(
            static function () use ($dispatcher, $job, $container, $maximumAttempts): void {
                $instance = is_object($job) ? $job : ($container !== null ? $container->get($job) : new $job());
                if (!is_object($instance)) {
                    throw new \UnexpectedValueException('The scheduled job could not be built.');
                }
                $dispatcher->dispatch($instance, $maximumAttempts);
            },
            $name,
            $this->timezone,
        ));
    }

    /** Runs a callback when due; name it when the same file declares several. */
    public function call(callable $callback): ScheduledTask
    {
        $closure = \Closure::fromCallable($callback);
        $reflection = new \ReflectionFunction($closure);
        $file = $reflection->getFileName();
        $name = $file === false
            ? 'call:' . $reflection->getName()
            : 'call:' . basename($file) . ':' . $reflection->getStartLine();
        return $this->add(new ScheduledTask(static function () use ($closure): void {
            $closure();
        }, $name, $this->timezone));
    }

    /**
     * Runs a console command of the application in the worker when due, as Laravel's
     * `$schedule->command('buck:sync --force')->hourly()->withoutOverlapping()`. A non-zero
     * exit code fails the task with the command output. Requires a container on the Schedule.
     *
     * @param string|class-string $command A command line or a command class.
     * @param array<string, mixed>|list<string> $parameters
     */
    public function command(string $command, array $parameters = []): ScheduledTask
    {
        $container = $this->container
            ?? throw new \LogicException('Scheduling commands requires the application container on the Schedule.');
        if (trim($command) === '') {
            throw new \InvalidArgumentException('A scheduled command needs a name.');
        }
        $name = substr('command:' . trim($command . ' ' . (array_is_list($parameters)
            ? implode(' ', array_map(static fn (mixed $value): string => is_scalar($value) ? (string) $value : '', $parameters))
            : '')), 0, 255);
        return $this->add(new ScheduledTask(static function () use ($container, $command, $parameters): void {
            $kernel = $container->get(\Nimbus\Console\Kernel::class);
            if (!$kernel instanceof \Nimbus\Console\Kernel) {
                throw new \LogicException('The container binds another console Kernel.');
            }
            $status = $kernel->call($command, $parameters);
            if ($status !== 0) {
                throw new \RuntimeException(sprintf(
                    'Scheduled command %s exited with %d: %s',
                    $command,
                    $status,
                    trim(substr($kernel->output(), 0, 2_000)),
                ));
            }
        }, $name, $this->timezone));
    }

    /** @return list<ScheduledTask> */
    public function tasks(): array
    {
        return $this->tasks;
    }

    /** @return list<ScheduledTask> */
    public function dueTasks(\DateTimeInterface $now): array
    {
        return array_values(array_filter(
            $this->tasks,
            static fn (ScheduledTask $task): bool => $task->isDue($now),
        ));
    }

    private function add(ScheduledTask $task): ScheduledTask
    {
        if (count($this->tasks) >= 1_000) {
            throw new \LengthException('A schedule holds at most 1,000 tasks.');
        }
        $this->tasks[] = $task;
        return $task;
    }
}
