<?php

declare(strict_types=1);

use Illuminate\Contracts\Console\Kernel;
use Pam\Laravel\Services\ProcessSupervisor;

require __DIR__.'/vendor/autoload.php';

$application = require __DIR__.'/bootstrap/app.php';
$application->make(Kernel::class)->bootstrap();

$directory = sys_get_temp_dir().'/pam-supervisor-smoke-'.bin2hex(random_bytes(6));
$state = $directory.'/state';
$logs = $directory.'/logs';
$manifest = $directory.'/processes.json';
if (!mkdir($directory, 0750, true)) {
    throw new RuntimeException('Could not create the supervisor smoke directory.');
}
file_put_contents($manifest, json_encode([
    'processes' => [
        'probe' => [
            'command' => [PHP_BINARY, '-r', 'sleep(30);'],
            'instances' => 1,
        ],
    ],
], JSON_THROW_ON_ERROR), LOCK_EX);
config()->set([
    'pam.supervisor.manifest' => $manifest,
    'pam.supervisor.state_path' => $state,
    'pam.supervisor.log_path' => $logs,
    'pam.supervisor.stop_timeout_seconds' => 2,
]);

$supervisor = new ProcessSupervisor();
try {
    $supervisor->start('probe');
    $status = $supervisor->status()[0] ?? null;
    if (!is_array($status) || $status['running'] !== true || $status['pid'] < 1) {
        throw new RuntimeException('The managed probe did not start.');
    }
    $stateDocument = json_decode((string) file_get_contents($state.'/probe-1.pid'), true, flags: JSON_THROW_ON_ERROR);
    if (!is_int($stateDocument['startTicks'] ?? null) || $stateDocument['startTicks'] < 1) {
        throw new RuntimeException('The PID reuse guard was not persisted.');
    }
    $supervisor->stop('probe');
    if (($supervisor->status()[0]['running'] ?? true) !== false) {
        throw new RuntimeException('The managed probe did not stop.');
    }
} finally {
    $supervisor->stop('probe');
}

fwrite(STDOUT, "Process supervisor smoke passed.\n");
