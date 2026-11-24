<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;
use Nimbus\Scheduling\Schedule;

/** @internal Built-in command of bin/nimbus. */
final class ScheduleListCommand extends Command
{
    protected $signature = 'schedule:list';

    protected $description = 'Lists the scheduled tasks of the Schedule bound in the container.';

    public function handle(): int
    {
        $container = $this->app()->container();
        $schedule = $container->has(Schedule::class) ? $container->get(Schedule::class) : null;
        if (!$schedule instanceof Schedule) {
            $this->info('No Schedule is bound in the container.');
            return self::SUCCESS;
        }
        $rows = [];
        foreach ($schedule->tasks() as $task) {
            $overlap = $task->overlapMinutes();
            $rows[] = [$task->expression(), $task->getName(), $overlap === null ? '' : "without overlapping ({$overlap} min)"];
        }
        $this->table(['Cron', 'Task', ''], $rows);
        return self::SUCCESS;
    }
}
