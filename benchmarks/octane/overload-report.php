<?php

declare(strict_types=1);

$input = $argv[1] ?? '';
$output = $argv[2] ?? '';
$recoveryStatus = filter_var($argv[3] ?? null, FILTER_VALIDATE_INT);
if ($input === '' || !is_file($input) || $output === '' || $recoveryStatus === false) {
    fwrite(STDERR, "usage: php overload-report.php <samples.tsv> <report.json> <recovery-status>\n");
    exit(64);
}

$statuses = [];
$latencies = [];
$retryAfterMissing = 0;
foreach (file($input, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [] as $line) {
    $fields = explode("\t", $line);
    if (count($fields) !== 3
        || filter_var($fields[0], FILTER_VALIDATE_INT) === false
        || !is_numeric($fields[2])) {
        throw new RuntimeException('invalid overload sample');
    }
    $status = (int) $fields[0];
    $statuses[$status] = ($statuses[$status] ?? 0) + 1;
    $latencies[] = (int) round((float) $fields[2] * 1_000);
    if ($status === 503 && trim($fields[1]) === '') {
        ++$retryAfterMissing;
    }
}
ksort($statuses, SORT_NUMERIC);
sort($latencies, SORT_NUMERIC);
$unexpected = array_filter(
    array_keys($statuses),
    static fn (int $status): bool => $status !== 200 && $status !== 503,
);
$passed = ($statuses[200] ?? 0) > 0
    && ($statuses[503] ?? 0) > 0
    && $retryAfterMissing === 0
    && $unexpected === []
    && $recoveryStatus === 200;
$report = [
    'schema_version' => 1,
    'passed' => $passed,
    'requests' => array_sum($statuses),
    'status_counts' => array_map(
        static fn (int $status, int $count): array => ['status' => $status, 'count' => $count],
        array_keys($statuses),
        array_values($statuses),
    ),
    'retry_after_missing' => $retryAfterMissing,
    'unexpected_statuses' => array_values($unexpected),
    'recovery_status' => $recoveryStatus,
    'latency_ms' => [
        'min' => $latencies[0] ?? null,
        'p50' => $latencies === [] ? null : $latencies[(int) floor((count($latencies) - 1) * 0.50)],
        'p99' => $latencies === [] ? null : $latencies[(int) floor((count($latencies) - 1) * 0.99)],
        'max' => $latencies === [] ? null : $latencies[array_key_last($latencies)],
    ],
];
file_put_contents($output, json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n");
fwrite(STDOUT, json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n");
exit($passed ? 0 : 1);
