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
if ($directory === '' || !is_dir($directory) || $maximumP95 === false || $maximumRssGrowth === false
    || $maximumP95 < 1 || $maximumRssGrowth < 0) {
    fwrite(STDERR, "usage: recovery-report.php RESULTS MAX_P95_MS MAX_RSS_GROWTH_BYTES\n");
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
}
fclose($handle);
if (count($latencies) < 3 || count($latencies) > 100) {
    fwrite(STDERR, "recovery evidence requires 3-100 rounds\n");
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
    'daemon_rss_growth_bytes' => $rssGrowth,
    'thresholds' => [
        'maximum_p95_millis' => $maximumP95,
        'maximum_rss_growth_bytes' => $maximumRssGrowth,
    ],
    'gate_codes' => [
        'success' => ($successGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'latency' => ($latencyGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
        'resources' => ($resourceGate ? RecoveryGate::Passed : RecoveryGate::Failed)->value,
    ],
    'passed' => $successGate && $latencyGate && $resourceGate,
];
file_put_contents(
    $directory.'/recovery-report.json',
    json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n",
);
fwrite(STDOUT, json_encode($report, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
exit($report['passed'] ? 0 : 1);
