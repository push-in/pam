<?php

declare(strict_types=1);

// Loaded through phpunit.xml when OCTANE_DRIVER=pam. Writes the cross-check
// report consumed by run.sh.
require __DIR__.'/../../vendor/autoload.php';

register_shutdown_function(static function (): void {
    $report = getenv('PAM_OCTANE_MISMATCH_REPORT');
    if (!is_string($report) || $report === '') {
        return;
    }
    file_put_contents($report, json_encode([
        'exchanges' => \Laravel\Octane\Tests\Pam\PamFakeClient::$exchanges,
        'mismatches' => \Laravel\Octane\Tests\Pam\PamFakeClient::$mismatches,
    ], JSON_PRETTY_PRINT));
});
