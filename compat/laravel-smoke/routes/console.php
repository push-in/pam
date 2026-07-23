<?php

declare(strict_types=1);

use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Symfony\Component\Console\Command\Command;

Artisan::command('pam:runtime {value}', function (string $value): int {
    fwrite(STDOUT, json_encode([
        'sapi' => PHP_SAPI,
        'console' => app()->runningInConsole(),
        'value' => $value,
        'binary' => basename(PHP_BINARY),
        'stdin' => defined('STDIN'),
        'stdout' => defined('STDOUT'),
        'stderr' => defined('STDERR'),
    ], JSON_THROW_ON_ERROR) . PHP_EOL);

    return Command::SUCCESS;
})->purpose('Report the PAM Artisan runtime compatibility contract.');

Artisan::command('pam:queue-seed {value=queued}', function (string $value): int {
    PamLaravelQueuedJob::dispatch($value);
    fwrite(STDOUT, "queued:{$value}" . PHP_EOL);

    return Command::SUCCESS;
})->purpose('Seed the PAM Laravel queue compatibility contract.');

Artisan::command('pam:queue-result', function (): int {
    $value = DB::table('pam_queue_results')->latest('id')->value('value');
    fwrite(STDOUT, (is_string($value) ? $value : 'empty') . PHP_EOL);

    return Command::SUCCESS;
})->purpose('Read the PAM Laravel queue compatibility result.');

Artisan::command('pam:schedule-probe', function (): int {
    fwrite(STDOUT, 'scheduled' . PHP_EOL);

    return Command::SUCCESS;
})->purpose('Exercise Laravel scheduler discovery under PAM.');

Artisan::command('pam:observer-counts', function (): int {
    $tables = [
        'telescope_entries',
        'pulse_values',
        'pulse_entries',
        'pulse_aggregates',
    ];
    foreach ($tables as $table) {
        if (!\Illuminate\Support\Facades\Schema::hasTable($table)) {
            throw new RuntimeException("Observer table [{$table}] is unavailable.");
        }
    }

    fwrite(STDOUT, json_encode([
        'telescope' => DB::table('telescope_entries')->count(),
        'pulse' => DB::table('pulse_values')->count()
            + DB::table('pulse_entries')->count()
            + DB::table('pulse_aggregates')->count(),
    ], JSON_THROW_ON_ERROR) . PHP_EOL);

    return Command::SUCCESS;
})->purpose('Report persisted Telescope and Pulse compatibility records.');

Artisan::command('pam:cache-roundtrip {store} {value}', function (
    string $store,
    string $value,
): int {
    $cache = Cache::store($store);
    $key = 'pam-laravel-cache-roundtrip';
    $cache->put($key, $value, 60);
    $stored = $cache->get($key);
    $cache->forget($key);
    fwrite(STDOUT, (is_string($stored) ? $stored : 'invalid') . PHP_EOL);

    return $stored === $value ? Command::SUCCESS : Command::FAILURE;
})->purpose('Exercise a named Laravel cache store through PAM.');
