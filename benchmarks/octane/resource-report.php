<?php

declare(strict_types=1);

$directory = $argv[1] ?? null;
if ($directory === null || !is_dir($directory)) {
    fwrite(STDERR, "usage: php resource-report.php <results-directory>\n");
    exit(64);
}

$report = [];
foreach (glob(rtrim($directory, '/').'/resources.*.csv') ?: [] as $path) {
    $runtime = preg_replace('/^resources\.|\.csv$/', '', basename($path));
    $rows = array_map('str_getcsv', file($path, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: []);
    array_shift($rows);
    $rss = array_map(static fn (array $row): int => (int) ($row[1] ?? 0), $rows);
    $cpu = array_map(static fn (array $row): float => (float) ($row[2] ?? 0), $rows);
    sort($rss);
    sort($cpu);
    $percentile = static fn (array $values, float $p): int|float => $values === []
        ? 0
        : $values[(int) floor((count($values) - 1) * $p)];
    $report[$runtime] = [
        'samples' => count($rows),
        'rss_peak_bytes' => $rss === [] ? 0 : max($rss),
        'rss_p50_bytes' => $percentile($rss, .50),
        'cpu_p50_percent' => $percentile($cpu, .50),
        'cpu_p95_percent' => $percentile($cpu, .95),
    ];
}

file_put_contents(
    rtrim($directory, '/').'/resources.json',
    json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES)."\n",
);
echo json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES)."\n";
