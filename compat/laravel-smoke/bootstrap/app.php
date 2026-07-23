<?php

declare(strict_types=1);

use Illuminate\Foundation\Application;
use Illuminate\Foundation\Configuration\Exceptions;
use Illuminate\Foundation\Configuration\Middleware;
use Illuminate\Console\Scheduling\Schedule;

require_once __DIR__ . '/compatibility.php';

return Application::configure(basePath: dirname(__DIR__))
    ->withProviders([PamLaravelCompatibilityServiceProvider::class])
    ->withRouting(
        web: __DIR__ . '/../routes/web.php',
        api: __DIR__ . '/../routes/api.php',
        commands: __DIR__ . '/../routes/console.php',
    )
    ->withMiddleware(static function (Middleware $middleware): void {
        // Exercise Laravel's unmodified web and API middleware groups.
    })
    ->withSchedule(static function (Schedule $schedule): void {
        $schedule->command('pam:schedule-probe')->everyMinute();
    })
    ->withExceptions(static function (Exceptions $exceptions): void {
        // Keep Laravel's default exception rendering contract.
    })
    ->create();
