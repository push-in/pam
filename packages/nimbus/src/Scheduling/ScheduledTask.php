<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Cron\CronExpression;

/** One entry of a Schedule: what runs, when, and how it is guarded. */
final class ScheduledTask
{
    private string $expression = '* * * * *';

    private ?CronExpression $cron = null;

    private ?\DateTimeZone $timezone;

    private string $name;

    private ?int $overlapMinutes = null;

    /** @param \Closure(): mixed $action */
    public function __construct(
        private readonly \Closure $action,
        string $name,
        ?\DateTimeZone $timezone = null,
    ) {
        $this->name = self::assertName($name);
        $this->timezone = $timezone;
    }

    /** A five-field cron expression (or a macro such as `@hourly`). */
    public function cron(string $expression): self
    {
        self::assertCronAvailable();
        if (!CronExpression::isValidExpression($expression)) {
            throw new \InvalidArgumentException("Invalid cron expression: {$expression}.");
        }
        $this->expression = $expression;
        $this->cron = null;
        return $this;
    }

    public function everyMinute(): self
    {
        return $this->cron('* * * * *');
    }

    public function everyTwoMinutes(): self
    {
        return $this->cron('*/2 * * * *');
    }

    public function everyFiveMinutes(): self
    {
        return $this->cron('*/5 * * * *');
    }

    public function everyTenMinutes(): self
    {
        return $this->cron('*/10 * * * *');
    }

    public function everyFifteenMinutes(): self
    {
        return $this->cron('*/15 * * * *');
    }

    public function everyThirtyMinutes(): self
    {
        return $this->cron('0,30 * * * *');
    }

    public function hourly(): self
    {
        return $this->cron('0 * * * *');
    }

    public function hourlyAt(int $minute): self
    {
        if ($minute < 0 || $minute > 59) {
            throw new \InvalidArgumentException('The minute must be between 0 and 59.');
        }
        return $this->cron("{$minute} * * * *");
    }

    public function daily(): self
    {
        return $this->cron('0 0 * * *');
    }

    /** @param string $time `HH:MM`, 24-hour clock. */
    public function dailyAt(string $time): self
    {
        [$hour, $minute] = self::parseTime($time);
        return $this->cron("{$minute} {$hour} * * *");
    }

    /** @param int $day 0 (Sunday) to 6 (Saturday). */
    public function weeklyOn(int $day, string $time = '00:00'): self
    {
        if ($day < 0 || $day > 6) {
            throw new \InvalidArgumentException('The day of the week must be between 0 and 6.');
        }
        [$hour, $minute] = self::parseTime($time);
        return $this->cron("{$minute} {$hour} * * {$day}");
    }

    public function weekly(): self
    {
        return $this->weeklyOn(0);
    }

    public function monthlyOn(int $day = 1, string $time = '00:00'): self
    {
        if ($day < 1 || $day > 31) {
            throw new \InvalidArgumentException('The day of the month must be between 1 and 31.');
        }
        [$hour, $minute] = self::parseTime($time);
        return $this->cron("{$minute} {$hour} {$day} * *");
    }

    public function monthly(): self
    {
        return $this->monthlyOn();
    }

    /** The timezone the expression is read in; defaults to the schedule's. */
    public function timezone(string|\DateTimeZone $timezone): self
    {
        $this->timezone = is_string($timezone) ? new \DateTimeZone($timezone) : $timezone;
        return $this;
    }

    /**
     * Skips a due run while the previous one still holds the lock. The lock
     * expires after `$expiresAfterMinutes` in case the worker running it died.
     */
    public function withoutOverlapping(int $expiresAfterMinutes = 1_440): self
    {
        if ($expiresAfterMinutes < 1) {
            throw new \InvalidArgumentException('The overlap lock must last at least one minute.');
        }
        $this->overlapMinutes = $expiresAfterMinutes;
        return $this;
    }

    /** The identity used for locks; must be the same on every worker and server. */
    public function name(string $name): self
    {
        $this->name = self::assertName($name);
        return $this;
    }

    public function getName(): string
    {
        return $this->name;
    }

    public function expression(): string
    {
        return $this->expression;
    }

    public function overlapMinutes(): ?int
    {
        return $this->overlapMinutes;
    }

    public function isDue(\DateTimeInterface $now): bool
    {
        self::assertCronAvailable();
        $this->cron ??= new CronExpression($this->expression);
        $local = \DateTimeImmutable::createFromInterface($now);
        if ($this->timezone !== null) {
            $local = $local->setTimezone($this->timezone);
        }
        return $this->cron->isDue($local, $local->getTimezone()->getName());
    }

    public function run(): void
    {
        ($this->action)();
    }

    /** @internal */
    public static function assertCronAvailable(): void
    {
        if (!class_exists(CronExpression::class)) {
            throw new \LogicException(
                'The Nimbus scheduler requires dragonmantank/cron-expression: composer require dragonmantank/cron-expression.',
            );
        }
    }

    /** @return array{int, int} */
    private static function parseTime(string $time): array
    {
        if (preg_match('/^([01]?\d|2[0-3]):([0-5]\d)$/D', $time, $match) !== 1) {
            throw new \InvalidArgumentException("Times are written as HH:MM, got {$time}.");
        }
        return [(int) $match[1], (int) $match[2]];
    }

    private static function assertName(string $name): string
    {
        if ($name === '' || strlen($name) > 255) {
            throw new \InvalidArgumentException('Scheduled task names must hold 1 to 255 bytes.');
        }
        return $name;
    }
}
