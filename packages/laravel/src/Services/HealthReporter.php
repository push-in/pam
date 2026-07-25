<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Illuminate\Foundation\Application;
use Illuminate\Database\DatabaseManager;
use Throwable;

final readonly class HealthReporter
{
    public function __construct(
        private Application $app,
        private DatabaseManager $database,
        private ObservabilityRegistry $observability,
    ) {
    }

    /** @return array<string, mixed> */
    public function report(): array
    {
        $database = true;
        try {
            $this->database->connection()->getPdo();
        } catch (Throwable) {
            $database = false;
        }

        return [
            'ok' => $database,
            'runtime' => 'pam',
            'environment' => $this->app->environment(),
            'database' => $database,
            'memory_bytes' => memory_get_usage(true),
            'peak_memory_bytes' => memory_get_peak_usage(true),
            'observability' => $this->observability->snapshot(),
        ];
    }
}
