<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum RemoteAction: int
{
    case Deploy = 1;
    case Rollback = 2;
    case Status = 3;
    case Logs = 4;
    case Top = 5;
    case Workers = 6;
    case Queues = 7;
    case Scheduler = 8;
    case Scale = 9;

    public static function fromName(string $name): ?self
    {
        return match (strtolower($name)) {
            'deploy' => self::Deploy,
            'rollback' => self::Rollback,
            'status' => self::Status,
            'logs' => self::Logs,
            'top' => self::Top,
            'workers' => self::Workers,
            'queues' => self::Queues,
            'scheduler' => self::Scheduler,
            'scale' => self::Scale,
            default => null,
        };
    }

    public function path(): string
    {
        return match ($this) {
            self::Deploy => 'deployments',
            self::Rollback => 'rollbacks',
            self::Status => 'status',
            self::Logs => 'logs',
            self::Top => 'top',
            self::Workers => 'workers',
            self::Queues => 'queues',
            self::Scheduler => 'scheduler',
            self::Scale => 'scale',
        };
    }

    public function mutates(): bool
    {
        return in_array($this, [self::Deploy, self::Rollback, self::Scale], true);
    }
}
