<?php

declare(strict_types=1);

enum RecoveryTopology: int
{
    case PamMasterWorker = 1;
    case Pm2SingleProcess = 2;
}

enum ComparisonGate: int
{
    case Passed = 1;
    case Failed = 2;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
if ($directory === '' || !is_dir($directory)) {
    fwrite(STDERR, "usage: comparison-report.php RESULTS\n");
    exit(64);
}

$readRecovery = static function (string $path): array {
    if (!is_file($path) || is_link($path) || filesize($path) > 1024 * 1024) {
        throw new RuntimeException("recovery CSV is missing, unsafe, or oversized");
    }
    $handle = fopen($path, 'rb');
    if ($handle === false || fgetcsv($handle) !== ['round', 'recovery_millis', 'success']) {
        throw new RuntimeException("recovery CSV header is invalid");
    }
    $latencies = [];
    $successes = 0;
    while (($row = fgetcsv($handle)) !== false) {
        $round = count($latencies) + 1;
        if (count($row) !== 3
            || filter_var($row[0], FILTER_VALIDATE_INT) !== $round
            || filter_var($row[1], FILTER_VALIDATE_INT) === false
            || (int) $row[1] < 0
            || !in_array($row[2], ['0', '1'], true)) {
            throw new RuntimeException("recovery CSV contains an invalid row");
        }
        $latencies[] = (int) $row[1];
        $successes += (int) $row[2];
    }
    fclose($handle);
    if (count($latencies) < 3 || count($latencies) > 100) {
        throw new RuntimeException("recovery evidence requires 3-100 rounds");
    }
    sort($latencies, SORT_NUMERIC);
    $percentile = static fn (float $quantile): int => $latencies[
        max(0, (int) ceil(count($latencies) * $quantile) - 1)
    ];
    return [
        'rounds' => count($latencies),
        'successful_rounds' => $successes,
        'p50' => $percentile(0.50),
        'p95' => $percentile(0.95),
        'maximum' => max($latencies),
    ];
};
$readRssGrowth = static function (string $path): int {
    if (!is_file($path) || is_link($path) || filesize($path) > 64 * 1024) {
        throw new RuntimeException("resource evidence is missing, unsafe, or oversized");
    }
    $value = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    $before = $value['daemon_rss_before_bytes'] ?? null;
    $after = $value['daemon_rss_after_bytes'] ?? null;
    if (!is_int($before) || !is_int($after) || $before < 0 || $after < 0) {
        throw new RuntimeException("resource evidence is invalid");
    }
    return max(0, $after - $before);
};

$pam = $readRecovery($directory.'/pam/recovery.csv');
$pm2 = $readRecovery($directory.'/pm2/recovery.csv');
if ($pam['rounds'] !== $pm2['rounds']) {
    throw new RuntimeException("PAM and PM2 round counts differ");
}
$pam['topology_code'] = RecoveryTopology::PamMasterWorker->value;
$pam['daemon_rss_growth_bytes'] = $readRssGrowth($directory.'/pam/resources.json');
$pm2['topology_code'] = RecoveryTopology::Pm2SingleProcess->value;
$pm2['daemon_rss_growth_bytes'] = $readRssGrowth($directory.'/pm2/resources.json');
$pamSuccess = $pam['successful_rounds'] === $pam['rounds'];
$pm2Success = $pm2['successful_rounds'] === $pm2['rounds'];
$report = [
    'schema_version' => 1,
    'suite_code' => 6,
    'systems' => ['pam' => $pam, 'pm2' => $pm2],
    'comparison' => [
        'p50_delta_millis' => $pam['p50'] - $pm2['p50'],
        'p95_delta_millis' => $pam['p95'] - $pm2['p95'],
        'maximum_delta_millis' => $pam['maximum'] - $pm2['maximum'],
        'rss_not_directly_comparable' => true,
    ],
    'gate_codes' => [
        'pam_success' => ($pamSuccess ? ComparisonGate::Passed : ComparisonGate::Failed)->value,
        'pm2_success' => ($pm2Success ? ComparisonGate::Passed : ComparisonGate::Failed)->value,
        'equivalent_rounds' => ComparisonGate::Passed->value,
    ],
    'passed' => $pamSuccess && $pm2Success,
];
file_put_contents(
    $directory.'/comparison-report.json',
    json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n",
);
fwrite(STDOUT, json_encode($report, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
exit($report['passed'] ? 0 : 1);
