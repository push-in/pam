<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

/**
 * Runs the tasks of a Schedule that are due in the current minute.
 *
 * It is meant to be called often (every PAM task-worker tick, once a second):
 * each minute is evaluated once per process, and every due task first claims
 * its (task, minute) key in the SchedulerLock, so with a shared lock it runs on
 * exactly one worker of one server. Without a system cron.
 */
final class Scheduler
{
    private const string PREFIX = 'nimbus:schedule:';

    private ?int $lastMinute = null;

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @var \Closure(ScheduledTask, \Throwable): void */
    private readonly \Closure $onFailure;

    /**
     * @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time().
     * @param (\Closure(ScheduledTask, \Throwable): void)|null $onFailure Defaults to the runtime log.
     */
    public function __construct(
        private readonly Schedule $schedule,
        private readonly SchedulerLock $lock = new MemorySchedulerLock(),
        ?\Closure $clock = null,
        ?\Closure $onFailure = null,
    ) {
        $this->clock = $clock ?? time(...);
        $this->onFailure = $onFailure ?? static function (ScheduledTask $task, \Throwable $error): void {
            if (class_exists(\Pam\Observability\Telemetry::class)) {
                \Pam\Observability\Telemetry::log('error', 'Scheduled task failed', [
                    'task' => $task->getName(),
                    'exception' => $error::class,
                ]);
            }
        };
    }

    /**
     * @return list<string> Names of the tasks this call ran (including failed ones).
     */
    public function runDue(?int $now = null): array
    {
        $now ??= ($this->clock)();
        $minute = intdiv($now, 60);
        if ($minute === $this->lastMinute) {
            return [];
        }
        $this->lastMinute = $minute;
        $at = new \DateTimeImmutable('@' . ($minute * 60));
        $ran = [];
        foreach ($this->schedule->dueTasks($at) as $task) {
            $key = self::PREFIX . sha1($task->getName());
            // The claim is never released: it is what makes the minute run once.
            if (!$this->lock->acquire($key . ':' . $minute, 3_600)) {
                continue;
            }
            $overlap = $task->overlapMinutes();
            if ($overlap !== null && !$this->lock->acquire($key . ':running', $overlap * 60)) {
                continue;
            }
            try {
                $task->run();
            } catch (\Throwable $error) {
                ($this->onFailure)($task, $error);
            } finally {
                if ($overlap !== null) {
                    $this->lock->release($key . ':running');
                }
            }
            $ran[] = $task->getName();
        }
        return $ran;
    }
}
