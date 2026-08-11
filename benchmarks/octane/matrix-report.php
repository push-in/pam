<?php

declare(strict_types=1);

$directory = $argv[1] ?? __DIR__.'/results/matrix';
$reports = glob(rtrim($directory, '/').'/workers-*/report.json') ?: [];
$matrix = [];

foreach ($reports as $path) {
    $report = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    if (!is_array($report) || preg_match('/workers-(\d+)/', $path, $matches) !== 1) {
        continue;
    }
    $matrix[(int) $matches[1]] = $report;
}
ksort($matrix, SORT_NUMERIC);

$dynamicPassed = $matrix !== [];
$zeroErrors = true;
$p99Passed = $matrix !== [];
$measurementPassed = $matrix !== [];
foreach ($matrix as $report) {
    $dynamicPassed = $dynamicPassed && ($report['dynamic_gate']['passed_frankenphp'] ?? false) === true;
    $zeroErrors = $zeroErrors && ($report['dynamic_gate']['zero_errors'] ?? false) === true;
    $p99Passed = $p99Passed && ($report['dynamic_gate']['p99_passed'] ?? false) === true;
    $measurementPassed = $measurementPassed && ($report['measurement_gate']['passed'] ?? false) === true;
}

$result = [
    'schema_version' => 1,
    'generated_at' => gmdate(DATE_ATOM),
    'host' => [
        'cpu_count' => (int) trim((string) shell_exec('getconf _NPROCESSORS_ONLN')),
        'kernel' => trim((string) shell_exec('uname -srmo')),
    ],
    'workers' => $matrix,
    'release_gate' => [
        'dynamic_within_release_floor_at_every_worker_count' => $dynamicPassed,
        'zero_errors' => $zeroErrors,
        'p99_within_release_sla_at_every_worker_count' => $p99Passed,
        'statistically_stable_complete_measurements' => $measurementPassed,
    ],
];

$encoded = json_encode($result, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n";
file_put_contents(rtrim($directory, '/').'/matrix-report.json', $encoded);
fwrite(STDOUT, $encoded);

exit($dynamicPassed && $zeroErrors && $p99Passed && $measurementPassed ? 0 : 1);
