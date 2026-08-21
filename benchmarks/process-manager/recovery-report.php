<?php

declare(strict_types=1);

enum RecoveryGate: int
{
    case Passed = 1;
    case Failed = 2;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
$maximumP95 = filter_var($argv[2] ?? null, FILTER_VALIDATE_INT);
$maximumRssGrowth = filter_var($argv[3] ?? null, FILTER_VALIDATE_INT);
$maximumDetectionP95 = filter_var($argv[4] ?? null, FILTER_VALIDATE_INT);
$maximumBackoffP95 = filter_var($argv[5] ?? null, FILTER_VALIDATE_INT);
$maximumReadinessP95 = filter_var($argv[6] ?? null, FILTER_VALIDATE_INT);
if ($directory === '' || !is_dir($directory) || $maximumP95 === false || $maximumRssGrowth === false
    || $maximumDetectionP95 === false || $maximumBackoffP95 === false
    || $maximumReadinessP95 === false || min($maximumP95, $maximumDetectionP95,
        $maximumBackoffP95, $maximumReadinessP95) < 1 || $maximumRssGrowth < 0) {
    fwrite(STDERR, "usage: recovery-report.php RESULTS MAX_P95_MS MAX_RSS_GROWTH_BYTES MAX_DETECTION_P95_MS MAX_BACKOFF_P95_MS MAX_READINESS_P95_MS\n");
    exit(64);
}

$csv = $directory.'/recovery.csv';
if (!is_file($csv) || is_link($csv) || filesize($csv) > 1024 * 1024) {
    fwrite(STDERR, "recovery CSV is missing, unsafe, or oversized\n");
    exit(1);
}
$handle = fopen($csv, 'rb');
if ($handle === false || fgetcsv($handle) !== ['round', 'recovery_millis', 'success']) {
    fwrite(STDERR, "recovery CSV header is invalid\n");
    exit(1);
}
$latencies = [];
$successes = 0;
$recoveryOutcomes = [];
while (($row = fgetcsv($handle)) !== false) {
    $expectedRound = count($latencies) + 1;
    if (count($row) !== 3
        || filter_var($row[0], FILTER_VALIDATE_INT) !== $expectedRound
        || ($latency = filter_var($row[1], FILTER_VALIDATE_INT)) === false
        || $latency < 0
        || !in_array($row[2], ['0', '1'], true)) {
        fwrite(STDERR, "recovery CSV contains an invalid row\n");
        exit(1);
    }
    $latencies[] = (int) $row[1];
    $successes += (int) $row[2];
    $recoveryOutcomes[] = (int) $row[2];
}
fclose($handle);
if (count($latencies) < 3 || count($latencies) > 100) {
    fwrite(STDERR, "recovery evidence requires 3-100 rounds\n");
    exit(1);
}
$phaseCsv = $directory.'/recovery-phases.csv';
if (!is_file($phaseCsv) || is_link($phaseCsv) || filesize($phaseCsv) > 1024 * 1024) {
    fwrite(STDERR, "recovery phase CSV is missing, unsafe, or oversized\n");
    exit(1);
}
$phaseHandle = fopen($phaseCsv, 'rb');
if ($phaseHandle === false || fgetcsv($phaseHandle) !== [
    'round', 'detection_millis', 'backoff_millis', 'readiness_millis',
    'accounted_millis', 'success',
]) {
    fwrite(STDERR, "recovery phase CSV header is invalid\n");
    exit(1);
}
$phases = ['detection' => [], 'backoff' => [], 'readiness' => [], 'accounted' => []];
$phaseRows = 0;
while (($row = fgetcsv($phaseHandle)) !== false) {
    ++$phaseRows;
    $values = array_map(
        static fn (mixed $value): int|false => filter_var($value, FILTER_VALIDATE_INT),
        $row,
    );
    if (count($row) !== 6 || $values[0] !== $phaseRows
        || in_array(false, $values, true) || min(array_slice($values, 1, 4)) < 0
        || !in_array($values[5], [0, 1], true)
        || $values[5] !== $recoveryOutcomes[$phaseRows - 1]
        || ($values[5] === 1 && $values[4] !== $values[1] + $values[2] + $values[3])) {
        fwrite(STDERR, "recovery phase CSV contains an invalid row\n");
        exit(1);
    }
    if ($values[5] === 1) {
        foreach (array_keys($phases) as $index => $phase) {
            $phases[$phase][] = $values[$index + 1];
        }
    }
}
fclose($phaseHandle);
if ($phaseRows !== count($latencies)) {
    fwrite(STDERR, "recovery and phase round counts differ\n");
    exit(1);
}
$startupCsv = $directory.'/worker-startup.csv';
if (!is_file($startupCsv) || is_link($startupCsv) || filesize($startupCsv) > 1024 * 1024) {
    fwrite(STDERR, "worker startup CSV is missing, unsafe, or oversized\n");
    exit(1);
}
$startupHandle = fopen($startupCsv, 'rb');
if ($startupHandle === false || fgetcsv($startupHandle) !== [
    'round', 'workers', 'spawn_spread_millis', 'spawn_to_ready_p95_millis',
    'spawn_to_ready_maximum_millis', 'success',
]) {
    fwrite(STDERR, "worker startup CSV header is invalid\n");
    exit(1);
}
$startup = ['spawn_spread' => [], 'spawn_to_ready_p95' => [], 'spawn_to_ready_maximum' => []];
$startupRows = 0;
$workerCount = null;
while (($row = fgetcsv($startupHandle)) !== false) {
    ++$startupRows;
    $values = array_map(
        static fn (mixed $value): int|false => filter_var($value, FILTER_VALIDATE_INT),
        $row,
    );
    if (count($row) !== 6 || in_array(false, $values, true) || $values[0] !== $startupRows
        || $values[1] < 1 || min(array_slice($values, 2, 3)) < 0
        || $values[4] < $values[3] || !in_array($values[5], [0, 1], true)
        || $values[5] !== $recoveryOutcomes[$startupRows - 1]
        || ($workerCount !== null && $values[1] !== $workerCount)) {
        fwrite(STDERR, "worker startup CSV contains an invalid row\n");
        exit(1);
    }
    $workerCount ??= $values[1];
    if ($values[5] === 1) {
        foreach (array_keys($startup) as $index => $metric) {
            $startup[$metric][] = $values[$index + 2];
        }
    }
}
fclose($startupHandle);
if ($startupRows !== count($latencies)) {
    fwrite(STDERR, "recovery and worker startup round counts differ\n");
    exit(1);
}
sort($latencies, SORT_NUMERIC);
$percentile = static function (array $values, float $quantile): int {
    $index = max(0, (int) ceil(count($values) * $quantile) - 1);

    return $values[$index];
};
$resourcesPath = $directory.'/resources.json';
if (!is_file($resourcesPath) || is_link($resourcesPath) || filesize($resourcesPath) > 64 * 1024) {
    fwrite(STDERR, "resource evidence is missing, unsafe, or oversized\n");
    exit(1);
}
$rss = json_decode(
    (string) file_get_contents($resourcesPath),
    true,
    flags: JSON_THROW_ON_ERROR,
);
$rssBefore = $rss['daemon_rss_before_bytes'] ?? null;
$rssAfter = $rss['daemon_rss_after_bytes'] ?? null;
if (!is_int($rssBefore) || !is_int($rssAfter) || $rssBefore < 0 || $rssAfter < 0) {
    fwrite(STDERR, "resource evidence is invalid\n");
    exit(1);
}
$rssGrowth = max(0, $rssAfter - $rssBefore);
$p95 = $percentile($latencies, 0.95);
$successGate = $successes === count($latencies);
$latencyGate = $p95 <= $maximumP95;
$resourceGate = $rssGrowth <= $maximumRssGrowth;
$phaseSummary = [];
foreach ($phases as $name => $values) {
    sort($values, SORT_NUMERIC);
    $phaseSummary[$name.'_millis'] = $values === [] ? null : [
        'p50' => $percentile($values, 0.50),
        'p95' => $percentile($values, 0.95),
        'maximum' => max($values),
    ];
}
$startupSummary = [];
foreach ($startup as $name => $values) {
    sort($values, SORT_NUMERIC);
    $startupSummary[$name.'_millis'] = $values === [] ? null : [
        'p50' => $percentile($values, 0.50),
        'p95' => $percentile($values, 0.95),
        'maximum' => max($values),
    ];
}
$detectionGate = ($phaseSummary['detection_millis']['p95'] ?? PHP_INT_MAX) <= $maximumDetectionP95;
$backoffGate = ($phaseSummary['backoff_millis']['p95'] ?? PHP_INT_MAX) <= $maximumBackoffP95;
$readinessGate = ($phaseSummary['readiness_millis']['p95'] ?? PHP_INT_MAX) <= $maximumReadinessP95;
$report = [
    'schema_version' => 1,
    'suite_code' => 5,
    'rounds' => count($latencies),
    'successful_rounds' => $successes,
    'recovery_millis' => [
        'p50' => $percentile($latencies, 0.50),
        'p95' => $p95,
        'maximum' => max($latencies),
    ],
    'recovery_phases' => $phaseSummary,
    'worker_startup' => [
        'workers' => $workerCount,
        ...$startupSummary,
    ],
    'daemon_rss_growth_bytes' => $rssGrowth,
    'thresholds' => [
        'maximum_p95_millis' => $maximumP95,
        'maximum_detection_p95_millis' => $maximumDetectionP95,
        'maximum_backoff_p95_millis' => $maximumBackoffP95,
        'maximum_readiness_p95_millis' => $maximumReadinessP95,
        'maximum_rss_growth_bytes' => $maximumRssGrowth,
    ],
    'gate_codes' => [
        'success' => ($successGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'latency' => ($latencyGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'detection' => ($detectionGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'backoff' => ($backoffGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'readiness' => ($readinessGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'resources' => ($resourceGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
    ],
    'passed' => $successGate && $latencyGate && $detectionGate && $backoffGate
        && $readinessGate && $resourceGate,
];
file_put_contents(
    $directory.'/recovery-report.json',
    json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n",
);
fwrite(STDOUT, json_encode($report, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
exit($report['passed'] ? 0 : 1);
