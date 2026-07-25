<?php

declare(strict_types=1);

if ($argc !== 2 || !is_dir($argv[1])) {
    fwrite(STDERR, "Usage: php aggregate.php <result-directory>\n");
    exit(2);
}

$directory = rtrim($argv[1], '/');
$groups = [];
foreach (glob($directory.'/*.round-*.json') ?: [] as $path) {
    $name = preg_replace('/\.round-\d+\.json$/', '', basename($path));
    $row = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    if (is_string($name) && is_array($row)) {
        $groups[$name][] = $row;
    }
}

$median = static function (array $values): float {
    sort($values, SORT_NUMERIC);
    $count = count($values);
    if ($count === 0) {
        return 0.0;
    }
    $middle = intdiv($count, 2);

    return $count % 2 === 0
        ? ((float) $values[$middle - 1] + (float) $values[$middle]) / 2
        : (float) $values[$middle];
};

$runtimes = [];
foreach ($groups as $name => $rounds) {
    $runtimes[$name] = [
        'rounds' => count($rounds),
        'requests_per_second_median' => $median(array_column($rounds, 'rps')),
        'p50_milliseconds_median' => $median(array_map(static fn (array $row): float => (float) $row['latency']['p50_us'] / 1_000, $rounds)),
        'p95_milliseconds_median' => $median(array_map(static fn (array $row): float => (float) $row['latency']['p95_us'] / 1_000, $rounds)),
        'p99_milliseconds_median' => $median(array_map(static fn (array $row): float => (float) $row['latency']['p99_us'] / 1_000, $rounds)),
        'errors' => array_sum(array_column($rounds, 'errors')),
    ];
    $memoryPath = $directory.'/'.$name.'.memory.json';
    if (is_file($memoryPath)) {
        $runtimes[$name]['container_stats'] = json_decode((string) file_get_contents($memoryPath), true, flags: JSON_THROW_ON_ERROR);
    }
}

$metadataPath = $directory.'/metadata.json';
$report = [
    'schema' => 1,
    'methodology' => [
        'warmup_seconds' => (int) (getenv('PAM_BENCH_WARMUP_SECONDS') ?: 10),
        'duration_seconds' => (int) (getenv('PAM_BENCH_DURATION_SECONDS') ?: 30),
        'threads' => (int) (getenv('PAM_BENCH_THREADS') ?: 4),
        'connections' => (int) (getenv('PAM_BENCH_CONNECTIONS') ?: 64),
        'workers' => (int) (getenv('PAM_BENCH_WORKERS') ?: 4),
        'application_cpu_set' => (string) (getenv('PAM_BENCH_APP_CPUSET') ?: '0,1'),
        'load_generator_cpu_set' => (string) (getenv('PAM_BENCH_LOAD_CPUSET') ?: '2,3'),
        'memory_limit_kilobytes' => (int) (getenv('PAM_BENCH_MEMORY_LIMIT_KB') ?: 1_048_576),
    ],
    'metadata' => is_file($metadataPath)
        ? json_decode((string) file_get_contents($metadataPath), true, flags: JSON_THROW_ON_ERROR)
        : [],
    'runtimes' => $runtimes,
];

file_put_contents(
    $directory.'/report.json',
    json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES).PHP_EOL,
    LOCK_EX,
);
fwrite(STDOUT, json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES).PHP_EOL);
